//! Client for RecoverKit dongles over their USB vendor interface.
//!
//! The dongle is a small USB-C board that forces a cabled Mac into DFU (or
//! reboots it) by speaking Apple's USB-PD VDMs, so DFU can be triggered from any
//! host OS without an Apple Silicon Mac. It enumerates as a composite device: a
//! human CDC console (see the firmware README) plus a **vendor-specific
//! interface** (`bInterfaceClass = 0xFF`) that this module drives with `nusb`
//! control transfers — the same USB stack the rest of the SDK uses, so no serial
//! port, no OS serial driver, and no extra dependency.
//!
//! # Addressing
//!
//! Each dongle carries a unique USB serial (e.g. `DL-1A2B3C4D`), used as its id.
//! A Mac in DFU enumerates as a USB sibling of the dongle under the same hub, so
//! [`find_for_ecid`] maps a Mac (by ECID) to the dongle it is plugged into via
//! USB topology. [`find`] ties both together for callers.
//!
//! # Example
//!
//! ```no_run
//! # fn main() -> restorekit::Result<()> {
//! for d in restorekit::dongle::list()? {
//!     println!("{} ({})", d.serial, d.product);
//! }
//! // Trigger DFU on whatever Mac is cabled to the sole dongle.
//! restorekit::dongle::find(restorekit::dongle::DongleTarget::Auto)?.dfu()?;
//! # Ok(()) }
//! ```

use std::time::{Duration, Instant};

use nusb::transfer::{ControlIn, ControlOut, ControlType, Recipient};
use nusb::{Interface, MaybeFuture};
// The USB contract shared with the dongle firmware: VID/PID, string
// descriptors, and the vendor control protocol.
use restorekit_dongle_proto as proto;

use crate::device::{self, Device, APPLE_VID};
use crate::error::{Error, Result};

/// USB vendor ID the dongle enumerates with (MCS Electronics).
pub const DONGLE_VID: u16 = proto::VID;
/// USB product ID assigned to RecoverKit. Unique to us, but shared by every
/// RecoverKit model — the specific model is carried in the iProduct string
/// (see [`DongleModel::from_product`]), not the PID.
pub const DONGLE_PID: u16 = proto::PID;

/// Which RecoverKit device this is, derived from its USB iProduct string.
///
/// Adding a new model (e.g. Dongle Pro, RecoverKit Pro):
/// 1. Add its iProduct string to `restorekit-dongle-proto` and set it in that
///    model's firmware (`config.product`), keeping the shared VID/PID.
/// 2. Add a variant here and a match arm in [`DongleModel::from_product`].
///
/// Nothing else changes: discovery, udev, and the vendor protocol all key off
/// the shared VID/PID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DongleModel {
    /// `Dongle-Lite`
    Lite,
    /// `Dongle-Pro` — the USB 3.1 Gen 1 passthrough variant.
    Pro,
}

impl DongleModel {
    /// Identify the model from a USB iProduct string, e.g. `Dongle-Lite`.
    /// `None` if the string isn't one of ours.
    pub fn from_product(product: &str) -> Option<Self> {
        match product {
            proto::PRODUCT_LITE => Some(Self::Lite),
            proto::PRODUCT_PRO => Some(Self::Pro),
            _ => None,
        }
    }

    /// Git-tag prefix this model's firmware releases are published under.
    /// Lite and Pro build from the one firmware crate and share a release
    /// tag; the updater picks the per-model asset below, so a Pro never
    /// receives a Lite image (and vice versa).
    fn release_tag_prefix(self) -> &'static str {
        match self {
            Self::Lite | Self::Pro => "dongle-lite-fw-v",
        }
    }

    /// Release-asset name of this model's raw update image.
    fn release_asset(self) -> &'static str {
        match self {
            Self::Lite => "dongle-lite-fw.bin",
            Self::Pro => "dongle-pro-fw.bin",
        }
    }
}

/// GitHub repo firmware releases are published to, via tags like
/// `dongle-lite-fw-v0.2.0` (see .github/workflows/release-fw.yml).
const FW_RELEASE_REPO: &str = "fcjr/restorekit";

/// A firmware release published on GitHub.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FirmwareRelease {
    /// Firmware version, e.g. `0.2.0`.
    pub version: String,
    /// The release tag, e.g. `dongle-lite-fw-v0.2.0`.
    pub tag: String,
    /// Direct download URL of the update image.
    #[serde(skip)]
    url: String,
}

impl FirmwareRelease {
    /// Whether this release is newer than a dongle's reported version.
    pub fn newer_than(&self, fw_version: &str) -> bool {
        match (parse_version(&self.version), parse_version(fw_version)) {
            (Some(a), Some(b)) => a > b,
            // An unparseable device version means we can't claim it's current.
            (Some(_), None) => true,
            _ => false,
        }
    }

    /// Download the update image, ready for [`DongleHandle::update`].
    pub fn download(&self) -> Result<Vec<u8>> {
        let resp = crate::firmware::http_client()?
            .get(&self.url)
            .send()
            .map_err(Error::Http)?
            .error_for_status()
            .map_err(Error::Http)?;
        Ok(resp.bytes().map_err(Error::Http)?.to_vec())
    }
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut it = s.split('.').map(|p| p.parse::<u32>().ok());
    match (it.next(), it.next(), it.next(), it.next()) {
        (Some(Some(a)), Some(Some(b)), Some(Some(c)), None) => Some((a, b, c)),
        _ => None,
    }
}

/// The latest firmware release published for `model`, or `None` if there are
/// no published releases for it (yet).
pub fn latest_firmware(model: DongleModel) -> Result<Option<FirmwareRelease>> {
    let releases: serde_json::Value = crate::firmware::http_client()?
        .get(format!(
            "https://api.github.com/repos/{FW_RELEASE_REPO}/releases?per_page=100"
        ))
        .send()
        .map_err(Error::Http)?
        .error_for_status()
        .map_err(Error::Http)?
        .json()
        .map_err(Error::Http)?;

    let mut best: Option<((u32, u32, u32), FirmwareRelease)> = None;
    for rel in releases.as_array().map(Vec::as_slice).unwrap_or_default() {
        let tag = rel["tag_name"].as_str().unwrap_or_default();
        let Some(version) = tag.strip_prefix(model.release_tag_prefix()) else {
            continue;
        };
        let Some(parsed) = parse_version(version) else {
            continue;
        };
        let Some(url) = rel["assets"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .find(|a| a["name"].as_str() == Some(model.release_asset()))
            .and_then(|a| a["browser_download_url"].as_str())
        else {
            continue;
        };
        if best.as_ref().is_none_or(|(v, _)| parsed > *v) {
            best = Some((
                parsed,
                FirmwareRelease {
                    version: version.to_string(),
                    tag: tag.to_string(),
                    url: url.to_string(),
                },
            ));
        }
    }
    Ok(best.map(|(_, r)| r))
}

const CTRL_TIMEOUT: Duration = Duration::from_millis(500);
// Long enough for the firmware's CC-cycle re-establish + VDM spray (~4-5 s).
const CMD_TIMEOUT: Duration = Duration::from_secs(8);
// A firmware-update chunk completes only after the sector is erased and
// written (~50-100 ms); the final request also CRCs the whole staged image.
const FW_CHUNK_TIMEOUT: Duration = Duration::from_secs(3);
const FW_DONE_TIMEOUT: Duration = Duration::from_secs(10);

/// A discovered dongle. Cheap to hold; call [`Dongle::open`] (or the one-shot
/// [`Dongle::dfu`] / [`Dongle::reboot`]) to act on the cabled Mac.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Dongle {
    /// USB serial number, e.g. `DL-1A2B3C4D`. The stable dongle id.
    pub serial: String,
    /// USB product string, e.g. `Dongle-Lite`.
    pub product: String,
    /// Which RecoverKit model this is, derived from the product string.
    pub model: DongleModel,
    /// USB bus this dongle is on (used to correlate a Mac to its dongle).
    #[serde(skip)]
    bus_id: String,
    /// Physical port path from the root hub (topology correlation).
    #[serde(skip)]
    port_chain: Vec<u8>,
    /// Interface number of the vendor interface to claim.
    #[serde(skip)]
    vendor_iface: u8,
}

/// PD state the dongle reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PdState {
    Disconnected,
    VbusOn,
    Connected,
    Accept,
    Idle,
    Unknown,
}

/// A live status snapshot read from the dongle.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DongleStatus {
    /// The dongle's PD state machine position.
    pub pd_state: PdState,
    /// Whether a target Mac is currently attached to the dongle's USB-C port.
    pub target_attached: bool,
    /// Cable orientation: `true` = CC2 (flipped), `false` = CC1 (normal).
    pub polarity_cc2: bool,
    /// Result of the last command (raw firmware code, internal).
    #[serde(skip)]
    result: u8,
}

impl DongleStatus {
    fn parse(buf: &[u8]) -> Result<Self> {
        if buf.len() < 4 {
            return Err(Error::Dongle("short status response from dongle".into()));
        }
        let pd_state = match buf[1] {
            proto::PD_DISCONNECTED => PdState::Disconnected,
            proto::PD_VBUS_ON => PdState::VbusOn,
            proto::PD_CONNECTED => PdState::Connected,
            proto::PD_ACCEPT => PdState::Accept,
            proto::PD_IDLE => PdState::Idle,
            _ => PdState::Unknown,
        };
        Ok(DongleStatus {
            pd_state,
            target_attached: buf[2] & proto::FLAG_TARGET_ATTACHED != 0,
            polarity_cc2: buf[2] & proto::FLAG_POLARITY_CC2 != 0,
            result: buf[3],
        })
    }

    /// Whether a target Mac is attached and the dongle can act on it, without
    /// the caller having to reason about the PD state machine.
    pub fn target_ready(&self) -> bool {
        self.target_attached
    }
}

/// How to pick a dongle.
#[derive(Debug, Clone)]
pub enum DongleTarget {
    /// The only connected dongle; an error if several are present.
    Auto,
    /// A specific dongle by its USB serial id.
    Id(String),
    /// The dongle the DFU Mac with this ECID is plugged into (USB topology).
    Ecid(u64),
}

/// List every connected RecoverKit dongle. Cheap enumeration only.
pub fn list() -> Result<Vec<Dongle>> {
    let infos = nusb::list_devices()
        .wait()
        .map_err(|e| Error::Usb(e.to_string()))?;
    let mut out = Vec::new();
    for info in infos {
        if info.vendor_id() != DONGLE_VID || info.product_id() != DONGLE_PID {
            continue;
        }
        // All RecoverKit models share the VID/PID; the iProduct string says
        // which one this is. Models this build doesn't know are skipped.
        let product = info.product_string().unwrap_or("");
        let Some(model) = DongleModel::from_product(product) else {
            continue;
        };
        // Only usable if it exposes the vendor interface.
        let Some(vendor_iface) = info
            .interfaces()
            .find(|i| i.class() == proto::VENDOR_CLASS)
            .map(|i| i.interface_number())
        else {
            continue;
        };
        out.push(Dongle {
            serial: info.serial_number().unwrap_or("").to_string(),
            product: product.to_string(),
            model,
            bus_id: info.bus_id().to_string(),
            port_chain: info.port_chain().to_vec(),
            vendor_iface,
        });
    }
    Ok(out)
}

/// Find the single dongle a [`DongleTarget`] selects. Mirrors
/// [`device::find`](crate::device::find).
pub fn find(target: DongleTarget) -> Result<Dongle> {
    match target {
        DongleTarget::Id(id) => select_by_id(list()?, &id),
        DongleTarget::Ecid(ecid) => find_for_ecid(ecid),
        DongleTarget::Auto => {
            let mut ds = list()?;
            match ds.len() {
                0 => Err(Error::NoDongle),
                1 => Ok(ds.remove(0)),
                _ => Err(Error::MultipleDongles(serials(&ds))),
            }
        }
    }
}

/// Pick a dongle by serial: an exact match wins, otherwise any unambiguous
/// case-insensitive fragment of it (`5f41` for `DL-5F417536`).
fn select_by_id(ds: Vec<Dongle>, id: &str) -> Result<Dongle> {
    if let Some(i) = ds.iter().position(|d| d.serial.eq_ignore_ascii_case(id)) {
        let mut ds = ds;
        return Ok(ds.swap_remove(i));
    }
    let needle = id.to_ascii_lowercase();
    let mut matches: Vec<Dongle> = ds
        .into_iter()
        .filter(|d| d.serial.to_ascii_lowercase().contains(&needle))
        .collect();
    match matches.len() {
        0 => Err(Error::Dongle(format!(
            "no dongle matching '{id}' (see `restorekit dongle list`)"
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(Error::MultipleDongles(serials(&matches))),
    }
}

fn serials(ds: &[Dongle]) -> String {
    ds.iter()
        .map(|d| d.serial.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Block until a dongle matching `target` is connected, or `timeout` elapses.
/// Mirrors [`device::wait`](crate::device::wait).
pub fn wait(target: DongleTarget, timeout: std::time::Duration) -> Result<Dongle> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match find(target.clone()) {
            Ok(d) => return Ok(d),
            Err(Error::NoDongle) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            Err(Error::NoDongle) => return Err(Error::NoDongle),
            Err(e) => return Err(e),
        }
    }
}

/// Find the dongle a DFU Mac is plugged into, by USB topology.
///
/// The Mac in DFU enumerates as a sibling of the dongle under the same hub, so
/// the two share a USB bus and a parent port path. Requires the Mac to already
/// be USB-visible (in DFU) and cabled through the dongle's hub.
pub fn find_for_ecid(ecid: u64) -> Result<Dongle> {
    let infos: Vec<_> = nusb::list_devices()
        .wait()
        .map_err(|e| Error::Usb(e.to_string()))?
        .collect();

    let mac = infos
        .iter()
        .find(|&i| i.vendor_id() == APPLE_VID && device::from_usb(i).ecid == Some(ecid))
        .ok_or(Error::EcidNotConnected(ecid))?;

    list()?
        .into_iter()
        .find(|d| shares_parent_hub(d, mac))
        .ok_or_else(|| {
            Error::Dongle(format!(
                "the Mac with ECID {ecid:#x} is not behind a known dongle"
            ))
        })
}

/// True if `mac` and `dongle` are siblings under the same hub: same bus, same
/// depth, and identical port path except the final (per-port) element.
fn shares_parent_hub(dongle: &Dongle, mac: &nusb::DeviceInfo) -> bool {
    let mac_chain = mac.port_chain();
    mac.bus_id() == dongle.bus_id
        && !dongle.port_chain.is_empty()
        && mac_chain.len() == dongle.port_chain.len()
        && mac_chain[..mac_chain.len() - 1] == dongle.port_chain[..dongle.port_chain.len() - 1]
}

/// USB device class for a hub.
const USB_CLASS_HUB: u8 = 0x09;

/// How a device physically reaches this host, for DFU purposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    /// Directly on a host port — the host's own USB-PD DFU trigger can reach it.
    Direct,
    /// Behind a RecoverKit dongle (its id). Host DFU can't drive the target's CC
    /// through the dongle's hub — only dongle DFU works.
    Dongle(String),
    /// Behind a plain USB hub. Neither host nor dongle DFU can reach it.
    Hub,
}

impl Connection {
    /// Short kind label: `direct` | `dongle` | `hub`.
    pub fn kind(&self) -> &'static str {
        match self {
            Connection::Direct => "direct",
            Connection::Dongle(_) => "dongle",
            Connection::Hub => "hub",
        }
    }

    /// The dongle id when reached through one.
    pub fn dongle(&self) -> Option<&str> {
        match self {
            Connection::Dongle(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Whether the host's own USB-PD DFU trigger can reach this target (only
    /// when it's directly on a host port).
    pub fn host_reachable(&self) -> bool {
        matches!(self, Connection::Direct)
    }
}

/// Determine how `dev` reaches this host by USB topology. Distinguishes a Mac
/// cabled straight to a host port (host DFU works) from one reached through a
/// dongle (only dongle DFU works) or a plain hub (neither works) — all three
/// otherwise look like they're simply "on the DFU port".
pub fn connection_for(dev: &Device) -> Connection {
    let infos: Vec<_> = match nusb::list_devices().wait() {
        Ok(it) => it.collect(),
        Err(_) => return Connection::Direct,
    };
    let Some(me) = infos
        .iter()
        .find(|i| i.vendor_id() == APPLE_VID && i.serial_number() == Some(dev.serial.as_str()))
    else {
        return Connection::Direct;
    };

    // Behind a RecoverKit dongle? (Its hub also parents the dongle's MCU.)
    if let Ok(dongles) = list() {
        for d in dongles {
            if shares_parent_hub(&d, me) {
                return Connection::Dongle(d.serial);
            }
        }
    }

    // Behind any other external hub: a hub-class device on this bus whose port
    // chain is a strict prefix of ours (a real ancestor, not the root).
    let chain = me.port_chain();
    let behind_hub = infos.iter().any(|h| {
        h.class() == USB_CLASS_HUB
            && h.bus_id() == me.bus_id()
            && !h.port_chain().is_empty()
            && h.port_chain().len() < chain.len()
            && chain.starts_with(h.port_chain())
    });
    if behind_hub {
        Connection::Hub
    } else {
        Connection::Direct
    }
}

impl Dongle {
    /// Open the vendor interface for issuing commands.
    pub fn open(&self) -> Result<DongleHandle> {
        // Re-find the live device by serial; the list() snapshot may be stale.
        let info = nusb::list_devices()
            .wait()
            .map_err(|e| Error::Usb(e.to_string()))?
            .find(|i| {
                i.vendor_id() == DONGLE_VID
                    && i.product_id() == DONGLE_PID
                    && i.serial_number() == Some(self.serial.as_str())
            })
            .ok_or(Error::NoDongle)?;
        let dev = info.open().wait().map_err(|e| Error::Usb(e.to_string()))?;
        let iface = dev
            .claim_interface(self.vendor_iface)
            .wait()
            .map_err(|e| Error::Usb(e.to_string()))?;
        Ok(DongleHandle {
            iface,
            iface_num: self.vendor_iface,
        })
    }

    /// Put the cabled Mac into DFU mode.
    pub fn dfu(&self) -> Result<()> {
        self.open()?.dfu()
    }

    /// Reboot the cabled Mac.
    pub fn reboot(&self) -> Result<()> {
        self.open()?.reboot()
    }

    /// Put the cabled Mac into serial-console mode: the dongle muxes the target's
    /// debug UART onto SBU and bridges it to its target-serial CDC port.
    pub fn serial(&self) -> Result<()> {
        self.open()?.serial()
    }

    /// Read a live status snapshot.
    pub fn status(&self) -> Result<DongleStatus> {
        self.open()?.status()
    }

    /// Reboot the dongle itself into its USB bootloader (for firmware update).
    pub fn bootsel(&self) -> Result<()> {
        self.open()?.bootsel()
    }

    /// The firmware version the dongle reports, e.g. `0.1.0`.
    pub fn fw_version(&self) -> Result<String> {
        self.open()?.fw_version()
    }

    /// The Apple device currently cabled to this dongle and USB-visible on this
    /// host (in DFU or any mode), matched by USB topology — the forward of
    /// [`find_for_ecid`]. `None` if the target's USB data isn't routed to this
    /// host, or nothing Apple is attached.
    pub fn attached_device(&self) -> Result<Option<Device>> {
        let infos = nusb::list_devices()
            .wait()
            .map_err(|e| Error::Usb(e.to_string()))?;
        Ok(infos
            .filter(|i| i.vendor_id() == APPLE_VID)
            .find(|i| shares_parent_hub(self, i))
            .map(|i| device::from_usb(&i)))
    }
}

/// An open connection to a dongle's vendor interface.
pub struct DongleHandle {
    iface: Interface,
    iface_num: u8,
}

impl DongleHandle {
    /// Put the cabled Mac into DFU mode.
    pub fn dfu(&self) -> Result<()> {
        self.command(proto::VCMD_DFU, "dfu")
    }

    /// Reboot the cabled Mac.
    pub fn reboot(&self) -> Result<()> {
        self.command(proto::VCMD_REBOOT, "reboot")
    }

    /// Mux the Mac's debug UART onto the dongle's SBU serial bridge.
    pub fn serial(&self) -> Result<()> {
        self.command(proto::VCMD_SERIAL, "serial")
    }

    /// Switch the Mac's USB data lines to its debug-USB interface.
    pub fn debugusb(&self) -> Result<()> {
        self.command(proto::VCMD_DEBUGUSB, "debugusb")
    }

    /// Liveness check: no-op that confirms the dongle is responding.
    pub fn nop(&self) -> Result<()> {
        self.command(proto::VCMD_NOP, "nop")
    }

    /// Reboot the dongle itself into its USB bootloader for a firmware update.
    /// Fire-and-forget: the dongle drops off the bus and re-enumerates as the
    /// RP2040 bootloader, so there is no status to poll.
    pub fn bootsel(&self) -> Result<()> {
        self.vendor_out_raw(proto::VREQ_CMD, proto::VCMD_BOOTSEL, &[], CTRL_TIMEOUT)
            .map_err(|e| match e {
                // A stall is the firmware rejecting the request: it predates
                // USB bootsel.
                nusb::transfer::TransferError::Stall => Error::Dongle(
                    "this dongle's firmware predates USB bootsel; type `bootsel` on its \
                     serial console (CDC0) or replug it with the BOOTSEL button held"
                        .into(),
                ),
                e => Error::Dongle(e.to_string()),
            })
    }

    /// Stream a new firmware image to the dongle over the vendor interface —
    /// no bootloader mode, no mass-storage drive. The image is staged into
    /// the inactive flash slot, CRC-verified, and swapped in by the dongle's
    /// bootloader on the reboot this triggers; a bad image is rejected before
    /// the swap, and one that fails to boot is rolled back.
    ///
    /// `image` is the raw app binary (the ACTIVE-slot contents, e.g. from
    /// `llvm-objcopy -O binary --remove-section=.boot2`), NOT a UF2 or ELF.
    /// `progress` receives (bytes staged, total bytes).
    pub fn update(&self, image: &[u8], mut progress: impl FnMut(usize, usize)) -> Result<()> {
        if image.is_empty() {
            return Err(Error::Dongle("empty firmware image".into()));
        }
        let total = image.len();
        self.vendor_out_raw(
            proto::VREQ_FW_BEGIN,
            0,
            &(total as u32).to_le_bytes(),
            CTRL_TIMEOUT,
        )
        .map_err(|e| match e {
            nusb::transfer::TransferError::Stall => Error::Dongle(
                "the dongle rejected the update: its firmware predates USB updates \
                 (flash it once over the bootrom with `just fw-flash-full`), or the \
                 image is too big for its spare slot"
                    .into(),
            ),
            e => Error::Dongle(format!("starting the update: {e}")),
        })?;
        let mut chunk_buf = vec![0xFFu8; proto::FW_CHUNK];
        for (i, chunk) in image.chunks(proto::FW_CHUNK).enumerate() {
            chunk_buf.fill(0xFF);
            chunk_buf[..chunk.len()].copy_from_slice(chunk);
            self.vendor_out(proto::VREQ_FW_DATA, i as u16, &chunk_buf, FW_CHUNK_TIMEOUT)
                .map_err(|e| {
                    Error::Dongle(format!(
                        "update failed at {}/{} bytes: {e}",
                        i * proto::FW_CHUNK,
                        total
                    ))
                })?;
            progress((i * proto::FW_CHUNK + chunk.len()).min(total), total);
        }
        self.vendor_out(
            proto::VREQ_FW_DONE,
            0,
            &proto::crc32(image).to_le_bytes(),
            FW_DONE_TIMEOUT,
        )
        .map_err(|e| Error::Dongle(format!("update verification failed: {e}")))?;
        Ok(())
    }

    /// Vendor control OUT to the dongle's interface.
    fn vendor_out(&self, request: u8, value: u16, data: &[u8], timeout: Duration) -> Result<()> {
        self.vendor_out_raw(request, value, data, timeout)
            .map_err(|e| Error::Dongle(e.to_string()))
    }

    /// Like [`Self::vendor_out`], but keeps the raw transfer error so callers
    /// can tell a firmware rejection (stall) from a transport failure.
    fn vendor_out_raw(
        &self,
        request: u8,
        value: u16,
        data: &[u8],
        timeout: Duration,
    ) -> std::result::Result<(), nusb::transfer::TransferError> {
        self.iface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Interface,
                    request,
                    value,
                    index: self.iface_num as u16,
                    data,
                },
                timeout,
            )
            .wait()?;
        Ok(())
    }

    /// Read a live status snapshot.
    pub fn status(&self) -> Result<DongleStatus> {
        let buf = self.vendor_in(proto::VREQ_STATUS, proto::STATUS_LEN as u16)?;
        DongleStatus::parse(&buf)
    }

    /// The firmware version the dongle reports, e.g. `0.1.0`.
    pub fn fw_version(&self) -> Result<String> {
        let buf = self.vendor_in(proto::VREQ_VERSION, proto::FW_VERSION_MAX_LEN as u16)?;
        String::from_utf8(buf).map_err(|_| Error::Dongle("firmware version is not UTF-8".into()))
    }

    /// Vendor control IN from the dongle's interface.
    fn vendor_in(&self, request: u8, length: u16) -> Result<Vec<u8>> {
        self.iface
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Interface,
                    request,
                    value: 0,
                    index: self.iface_num as u16,
                    length,
                },
                CTRL_TIMEOUT,
            )
            .wait()
            .map_err(|e| Error::Dongle(e.to_string()))
    }

    /// Send a command and block until the firmware reports its outcome.
    fn command(&self, code: u16, name: &str) -> Result<()> {
        self.vendor_out(proto::VREQ_CMD, code, &[], CTRL_TIMEOUT)?;

        // The firmware marks the result pending synchronously in the OUT
        // handler, so we won't read a stale success from a prior command.
        let deadline = Instant::now() + CMD_TIMEOUT;
        loop {
            match self.status()?.result {
                proto::RES_PENDING => {}
                proto::RES_OK => return Ok(()),
                proto::RES_NOTARGET => return Err(Error::DongleNoTarget),
                _ => {}
            }
            if Instant::now() >= deadline {
                return Err(Error::Dongle(format!(
                    "{name}: timed out waiting for the dongle"
                )));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dongle(serial: &str) -> Dongle {
        Dongle {
            serial: serial.into(),
            product: proto::PRODUCT_LITE.into(),
            model: DongleModel::Lite,
            bus_id: String::new(),
            port_chain: Vec::new(),
            vendor_iface: 0,
        }
    }

    #[test]
    fn select_by_id_matches_fragments() {
        let ds = || vec![dongle("DL-5F417536"), dongle("DL-AA00BB11")];
        // Exact, case-insensitive exact, unique fragment.
        assert_eq!(
            select_by_id(ds(), "DL-5F417536").unwrap().serial,
            "DL-5F417536"
        );
        assert_eq!(
            select_by_id(ds(), "dl-aa00bb11").unwrap().serial,
            "DL-AA00BB11"
        );
        assert_eq!(select_by_id(ds(), "5f41").unwrap().serial, "DL-5F417536");
        // Ambiguous fragment lists the candidates; no match names the id.
        match select_by_id(ds(), "DL-") {
            Err(Error::MultipleDongles(s)) => {
                assert!(s.contains("DL-5F417536") && s.contains("DL-AA00BB11"))
            }
            other => panic!("expected MultipleDongles, got {other:?}"),
        }
        assert!(select_by_id(ds(), "zzz").is_err());
        // An exact serial that is also a fragment of another must win.
        let mut ds2 = ds();
        ds2.push(dongle("DL-5F41"));
        assert_eq!(select_by_id(ds2, "DL-5F41").unwrap().serial, "DL-5F41");
    }
}
