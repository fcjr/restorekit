//! Client for the RecoverKit **Dongle-Proto-Lite** over its USB vendor
//! interface.
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
//! Each dongle carries a unique USB serial (e.g. `DPL-1A2B3C4D`), used as its id.
//! A Mac in DFU enumerates as a USB sibling of the dongle under the same hub, so
//! [`find_for_ecid`] maps a Mac (by ECID) to the dongle it is plugged into via
//! USB topology. [`resolve`] ties both together for callers.
//!
//! # Example
//!
//! ```no_run
//! # fn main() -> restorekit::Result<()> {
//! for d in restorekit::dongle::list()? {
//!     println!("{} ({})", d.serial, d.product);
//! }
//! // Trigger DFU on whatever Mac is cabled to the sole dongle.
//! restorekit::dongle::resolve(restorekit::dongle::DongleTarget::Auto)?.dfu()?;
//! # Ok(()) }
//! ```

use std::time::{Duration, Instant};

use nusb::transfer::{ControlIn, ControlOut, ControlType, Recipient};
use nusb::{Interface, MaybeFuture};

use crate::device::{self, Device, APPLE_VID};
use crate::error::{Error, Result};

/// USB vendor ID the dongle enumerates with (Raspberry Pi / RP2040 default).
pub const DONGLE_VID: u16 = 0x2e8a;
/// USB product ID the Dongle-Proto-Lite firmware presents.
pub const DONGLE_PID: u16 = 0x000a;

// Vendor control protocol — must match the firmware (`src/main.rs`).
const VENDOR_CLASS: u8 = 0xff;
const VREQ_CMD: u8 = 0x01; // control OUT: wValue = command code
const VREQ_STATUS: u8 = 0x02; // control IN: status struct

const VCMD_NOP: u16 = 0;
const VCMD_DFU: u16 = 1;
const VCMD_REBOOT: u16 = 2;
const VCMD_SERIAL: u16 = 3;
const VCMD_DEBUGUSB: u16 = 4;

const RES_PENDING: u8 = 1;
const RES_OK: u8 = 2;
const RES_NOTARGET: u8 = 3;
const RES_NOACK: u8 = 4;

const CTRL_TIMEOUT: Duration = Duration::from_millis(500);
// Long enough for the firmware's CC-cycle re-establish + VDM spray (~4-5 s).
const CMD_TIMEOUT: Duration = Duration::from_secs(8);

/// A discovered dongle. Cheap to hold; call [`Dongle::open`] (or the one-shot
/// [`Dongle::dfu`] / [`Dongle::reboot`]) to act on the cabled Mac.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Dongle {
    /// USB serial number, e.g. `DPL-1A2B3C4D`. The stable dongle id.
    pub serial: String,
    /// USB product string, e.g. `Dongle-Proto-Lite`.
    pub product: String,
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
            0 => PdState::Disconnected,
            1 => PdState::VbusOn,
            2 => PdState::Connected,
            3 => PdState::Accept,
            4 => PdState::Idle,
            _ => PdState::Unknown,
        };
        Ok(DongleStatus {
            pd_state,
            target_attached: buf[2] & 0x01 != 0,
            polarity_cc2: buf[2] & 0x02 != 0,
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

/// List every connected Dongle-Proto-Lite. Cheap enumeration only.
pub fn list() -> Result<Vec<Dongle>> {
    let infos = nusb::list_devices()
        .wait()
        .map_err(|e| Error::Usb(e.to_string()))?;
    let mut out = Vec::new();
    for info in infos {
        if info.vendor_id() != DONGLE_VID || info.product_id() != DONGLE_PID {
            continue;
        }
        // Only ours if it exposes the vendor interface.
        let Some(vendor_iface) = info
            .interfaces()
            .find(|i| i.class() == VENDOR_CLASS)
            .map(|i| i.interface_number())
        else {
            continue;
        };
        out.push(Dongle {
            serial: info.serial_number().unwrap_or("").to_string(),
            product: info.product_string().unwrap_or("Dongle").to_string(),
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
        DongleTarget::Id(id) => list()?
            .into_iter()
            .find(|d| d.serial == id)
            .ok_or(Error::NoDongle),
        DongleTarget::Ecid(ecid) => find_for_ecid(ecid),
        DongleTarget::Auto => {
            let mut ds = list()?;
            match ds.len() {
                0 => Err(Error::NoDongle),
                1 => Ok(ds.remove(0)),
                n => Err(Error::MultipleDongles(n)),
            }
        }
    }
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
        .ok_or_else(|| Error::Dongle(format!("the Mac with ECID {ecid:#x} is not behind a known dongle")))
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

    /// Read a live status snapshot.
    pub fn status(&self) -> Result<DongleStatus> {
        self.open()?.status()
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
        self.command(VCMD_DFU, "dfu")
    }

    /// Reboot the cabled Mac.
    pub fn reboot(&self) -> Result<()> {
        self.command(VCMD_REBOOT, "reboot")
    }

    /// Mux the Mac's debug UART onto the dongle's SBU serial bridge.
    pub fn serial(&self) -> Result<()> {
        self.command(VCMD_SERIAL, "serial")
    }

    /// Switch the Mac's USB data lines to its debug-USB interface.
    pub fn debugusb(&self) -> Result<()> {
        self.command(VCMD_DEBUGUSB, "debugusb")
    }

    /// Liveness check: no-op that confirms the dongle is responding.
    pub fn nop(&self) -> Result<()> {
        self.command(VCMD_NOP, "nop")
    }

    /// Read a live status snapshot.
    pub fn status(&self) -> Result<DongleStatus> {
        let buf = self
            .iface
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Interface,
                    request: VREQ_STATUS,
                    value: 0,
                    index: self.iface_num as u16,
                    length: 8,
                },
                CTRL_TIMEOUT,
            )
            .wait()
            .map_err(|e| Error::Dongle(e.to_string()))?;
        DongleStatus::parse(&buf)
    }

    /// Send a command and block until the firmware reports its outcome.
    fn command(&self, code: u16, name: &str) -> Result<()> {
        self.iface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Interface,
                    request: VREQ_CMD,
                    value: code,
                    index: self.iface_num as u16,
                    data: &[],
                },
                CTRL_TIMEOUT,
            )
            .wait()
            .map_err(|e| Error::Dongle(e.to_string()))?;

        // The firmware marks the result pending synchronously in the OUT
        // handler, so we won't read a stale success from a prior command.
        let deadline = Instant::now() + CMD_TIMEOUT;
        loop {
            match self.status()?.result {
                RES_PENDING => {}
                RES_OK => return Ok(()),
                RES_NOTARGET => return Err(Error::DongleNoTarget),
                // Older firmware may still report no-ack; newer treats action
                // VDMs as fire-and-forget (no GoodCRC is expected).
                RES_NOACK => {
                    return Err(Error::Dongle(format!("{name}: target did not acknowledge")))
                }
                _ => {}
            }
            if Instant::now() >= deadline {
                return Err(Error::Dongle(format!("{name}: timed out waiting for the dongle")));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
