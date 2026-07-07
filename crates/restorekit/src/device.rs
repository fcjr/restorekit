use std::fmt;
use std::time::{Duration, Instant};

use nusb::MaybeFuture;
use serde::Serialize;

use crate::error::{Error, Result};

/// A known Apple Silicon Mac model, keyed by chip ID + board ID as reported
/// in the DFU-mode USB serial string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MacModel {
    pub cpid: u16,
    pub bdid: u8,
    /// Board config, e.g. "J313AP".
    pub board: &'static str,
    /// Model identifier, e.g. "MacBookAir10,1" — the key used by firmware APIs.
    pub identifier: &'static str,
    /// Marketing name, e.g. "MacBook Air (M1, Late 2020)".
    pub name: &'static str,
}

/// Generated from <https://api.ipsw.me/v4/devices> (cpid/bdid per board).
pub const MAC_MODELS: &[MacModel] = &[
    MacModel {
        cpid: 0x6000,
        bdid: 0x08,
        board: "J314sAP",
        identifier: "MacBookPro18,3",
        name: "MacBook Pro (M1 Pro, 14-inch, 2021)",
    },
    MacModel {
        cpid: 0x6000,
        bdid: 0x0a,
        board: "J316sAP",
        identifier: "MacBookPro18,1",
        name: "MacBook Pro (M1 Pro, 16-inch, 2021)",
    },
    MacModel {
        cpid: 0x6001,
        bdid: 0x04,
        board: "J375cAP",
        identifier: "Mac13,1",
        name: "Mac Studio (M1 Max)",
    },
    MacModel {
        cpid: 0x6001,
        bdid: 0x08,
        board: "J314cAP",
        identifier: "MacBookPro18,4",
        name: "MacBook Pro (M1 Max, 14-inch, 2021)",
    },
    MacModel {
        cpid: 0x6001,
        bdid: 0x0a,
        board: "J316cAP",
        identifier: "MacBookPro18,2",
        name: "MacBook Pro (M1 Max, 16-inch, 2021)",
    },
    MacModel {
        cpid: 0x6002,
        bdid: 0x0c,
        board: "J375dAP",
        identifier: "Mac13,2",
        name: "Mac Studio (M1 Ultra)",
    },
    MacModel {
        cpid: 0x6020,
        bdid: 0x02,
        board: "J474sAP",
        identifier: "Mac14,12",
        name: "Mac mini (M2 Pro, 2023)",
    },
    MacModel {
        cpid: 0x6020,
        bdid: 0x04,
        board: "J414sAP",
        identifier: "Mac14,9",
        name: "MacBook Pro (M2 Pro, 14-inch, 2023)",
    },
    MacModel {
        cpid: 0x6020,
        bdid: 0x06,
        board: "J416sAP",
        identifier: "Mac14,10",
        name: "MacBook Pro (M2 Pro, 16-inch, 2023)",
    },
    MacModel {
        cpid: 0x6021,
        bdid: 0x04,
        board: "J414cAP",
        identifier: "Mac14,5",
        name: "MacBook Pro (M2 Max, 14-inch, 2023)",
    },
    MacModel {
        cpid: 0x6021,
        bdid: 0x06,
        board: "J416cAP",
        identifier: "Mac14,6",
        name: "MacBook Pro (M2 Max, 16-inch, 2023)",
    },
    MacModel {
        cpid: 0x6021,
        bdid: 0x0a,
        board: "J475cAP",
        identifier: "Mac14,13",
        name: "Mac Studio (M2 Max, 2023)",
    },
    MacModel {
        cpid: 0x6022,
        bdid: 0x08,
        board: "J180dAP",
        identifier: "Mac14,8",
        name: "Mac Pro (2023)",
    },
    MacModel {
        cpid: 0x6022,
        bdid: 0x0a,
        board: "J475dAP",
        identifier: "Mac14,14",
        name: "Mac Studio (M2 Ultra, 2023)",
    },
    MacModel {
        cpid: 0x6030,
        bdid: 0x04,
        board: "J514sAP",
        identifier: "Mac15,6",
        name: "MacBook Pro (M3 Pro, 14-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6030,
        bdid: 0x06,
        board: "J516sAP",
        identifier: "Mac15,7",
        name: "MacBook Pro (M3 Pro, 16-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6031,
        bdid: 0x44,
        board: "J514cAP",
        identifier: "Mac15,8",
        name: "MacBook Pro (M3 Max, 14-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6031,
        bdid: 0x46,
        board: "J516cAP",
        identifier: "Mac15,9",
        name: "MacBook Pro (M3 Max, 16-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6032,
        bdid: 0x44,
        board: "J575dAP",
        identifier: "Mac15,14",
        name: "Mac Studio (2025)",
    },
    MacModel {
        cpid: 0x6034,
        bdid: 0x44,
        board: "J514mAP",
        identifier: "Mac15,10",
        name: "MacBook Pro (M3 Max, 14-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6034,
        bdid: 0x46,
        board: "J516mAP",
        identifier: "Mac15,11",
        name: "MacBook Pro (M3 Max, 16-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x6040,
        bdid: 0x02,
        board: "J773sAP",
        identifier: "Mac16,11",
        name: "Mac mini (M4 Pro, 2024)",
    },
    MacModel {
        cpid: 0x6040,
        bdid: 0x04,
        board: "J614sAP",
        identifier: "Mac16,8",
        name: "MacBook Pro (M4 Pro, 14-inch, Nov 2024)",
    },
    MacModel {
        cpid: 0x6040,
        bdid: 0x06,
        board: "J616sAP",
        identifier: "Mac16,7",
        name: "MacBook Pro (M4 Pro, 16-inch, Nov 2024)",
    },
    MacModel {
        cpid: 0x6041,
        bdid: 0x02,
        board: "J575cAP",
        identifier: "Mac16,9",
        name: "Mac Studio (2025)",
    },
    MacModel {
        cpid: 0x6041,
        bdid: 0x04,
        board: "J614cAP",
        identifier: "Mac16,6",
        name: "MacBook Pro (M4 Max, 14-inch, Nov 2024)",
    },
    MacModel {
        cpid: 0x6041,
        bdid: 0x06,
        board: "J616cAP",
        identifier: "Mac16,5",
        name: "MacBook Pro (M4 Max, 16-inch, Nov 2024)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x08,
        board: "J714sAP",
        identifier: "Mac17,9",
        name: "MacBook Pro (14-inch, M5 Pro)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x0a,
        board: "J714cAP",
        identifier: "Mac17,7",
        name: "MacBook Pro (14-inch, M5 Max)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x0c,
        board: "J716sAP",
        identifier: "Mac17,8",
        name: "MacBook Pro (16-inch, M5 Pro)",
    },
    MacModel {
        cpid: 0x6050,
        bdid: 0x0e,
        board: "J716cAP",
        identifier: "Mac17,6",
        name: "MacBook Pro (16-inch, M5 Max)",
    },
    MacModel {
        cpid: 0x8103,
        bdid: 0x22,
        board: "J274AP",
        identifier: "Macmini9,1",
        name: "Mac mini (M1, Late 2020)",
    },
    MacModel {
        cpid: 0x8103,
        bdid: 0x24,
        board: "J293AP",
        identifier: "MacBookPro17,1",
        name: "MacBook Pro (M1, Late 2020)",
    },
    MacModel {
        cpid: 0x8103,
        bdid: 0x26,
        board: "J313AP",
        identifier: "MacBookAir10,1",
        name: "MacBook Air (M1, Late 2020)",
    },
    MacModel {
        cpid: 0x8103,
        bdid: 0x28,
        board: "J456AP",
        identifier: "iMac21,1",
        name: "iMac 24-inch (M1, Two Ports, 2021)",
    },
    MacModel {
        cpid: 0x8103,
        bdid: 0x2a,
        board: "J457AP",
        identifier: "iMac21,2",
        name: "iMac 24-inch (M1, Four Ports, 2021)",
    },
    MacModel {
        cpid: 0x8112,
        bdid: 0x24,
        board: "J473AP",
        identifier: "Mac14,3",
        name: "Mac mini (M2, 2023)",
    },
    MacModel {
        cpid: 0x8112,
        bdid: 0x28,
        board: "J413AP",
        identifier: "Mac14,2",
        name: "MacBook Air (M2, 2022)",
    },
    MacModel {
        cpid: 0x8112,
        bdid: 0x2a,
        board: "J493AP",
        identifier: "Mac14,7",
        name: "MacBook Pro (13-inch, M2, 2022)",
    },
    MacModel {
        cpid: 0x8112,
        bdid: 0x2e,
        board: "J415AP",
        identifier: "Mac14,15",
        name: "MacBook Air (15-inch, M2, 2023)",
    },
    MacModel {
        cpid: 0x8122,
        bdid: 0x22,
        board: "J504AP",
        identifier: "Mac15,3",
        name: "MacBook Pro (M3, 14-inch, Nov 2023)",
    },
    MacModel {
        cpid: 0x8122,
        bdid: 0x28,
        board: "J433AP",
        identifier: "Mac15,4",
        name: "iMac (Two Ports, 24-inch, 2023)",
    },
    MacModel {
        cpid: 0x8122,
        bdid: 0x2a,
        board: "J434AP",
        identifier: "Mac15,5",
        name: "iMac (Four Ports, 24-inch, 2023)",
    },
    MacModel {
        cpid: 0x8122,
        bdid: 0x30,
        board: "J613AP",
        identifier: "Mac15,12",
        name: "MacBook Air (13-inch, M3, 2024)",
    },
    MacModel {
        cpid: 0x8122,
        bdid: 0x32,
        board: "J615AP",
        identifier: "Mac15,13",
        name: "MacBook Air (15-inch, M3, 2024)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x22,
        board: "J604AP",
        identifier: "Mac16,1",
        name: "MacBook Pro (M4, 14-inch, Nov 2024)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x24,
        board: "J623AP",
        identifier: "Mac16,2",
        name: "iMac (Two Ports, 24-inch, 2024)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x26,
        board: "J624AP",
        identifier: "Mac16,3",
        name: "iMac (Four Ports, 24-inch, 2024)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x2a,
        board: "J773gAP",
        identifier: "Mac16,10",
        name: "Mac mini (M4, 2024)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x2c,
        board: "J713AP",
        identifier: "Mac16,12",
        name: "MacBook Air (13-inch, M4, 2025)",
    },
    MacModel {
        cpid: 0x8132,
        bdid: 0x2e,
        board: "J715AP",
        identifier: "Mac16,13",
        name: "MacBook Air (15-inch, M4, 2025)",
    },
    MacModel {
        cpid: 0x8140,
        bdid: 0x64,
        board: "J700AP",
        identifier: "Mac17,5",
        name: "MacBook Neo",
    },
    MacModel {
        cpid: 0x8142,
        bdid: 0x22,
        board: "J704AP",
        identifier: "Mac17,2",
        name: "MacBook Pro (14-inch, M5)",
    },
    MacModel {
        cpid: 0x8142,
        bdid: 0x24,
        board: "J813AP",
        identifier: "Mac17,3",
        name: "MacBook Air (13-inch, M5)",
    },
    MacModel {
        cpid: 0x8142,
        bdid: 0x26,
        board: "J815AP",
        identifier: "Mac17,4",
        name: "MacBook Air (15-inch, M5)",
    },
];

pub fn lookup(cpid: u16, bdid: u8) -> Option<&'static MacModel> {
    MAC_MODELS.iter().find(|m| m.cpid == cpid && m.bdid == bdid)
}

pub fn lookup_identifier(identifier: &str) -> Option<&'static MacModel> {
    MAC_MODELS
        .iter()
        .find(|m| m.identifier.eq_ignore_ascii_case(identifier))
}

/// Resolve a Mac model from the USB `bcdDevice` release number. A booted Mac
/// encodes its model identifier's numeric part there in BCD — e.g. `0x1701` →
/// "17,1" → `MacBookPro17,1`, `0x1606` → "16,6" → `Mac16,6` — which is how
/// Apple Configurator identifies a booted Mac's exact model. The numeric part
/// is unique across identifiers, so it resolves the model regardless of the
/// (varying) alphabetic prefix.
pub fn model_from_device_version(bcd: u16) -> Option<&'static MacModel> {
    let bcd_byte = |b: u16| (b >> 4) * 10 + (b & 0xf);
    let number = format!("{},{}", bcd_byte(bcd >> 8), bcd_byte(bcd & 0xff));
    MAC_MODELS.iter().find(|m| {
        // The identifier's numeric suffix (after the alphabetic prefix).
        match m.identifier.rfind(|c: char| c.is_ascii_alphabetic()) {
            Some(i) => m.identifier[i + 1..] == number,
            None => false,
        }
    })
}

/// Apple's USB vendor ID.
pub const APPLE_VID: u16 = 0x05ac;
/// Product ID presented by an Apple SoC in DFU mode.
pub const DFU_PID: u16 = 0x1227;

/// The USB mode a connected Apple device is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UsbMode {
    /// DFU mode — the only mode restorekit can restore from.
    Dfu,
    /// Recovery mode (iBoot).
    Recovery,
    /// WTF ("what's the firmware") mode.
    Wtf,
    /// Mid-restore ramdisk.
    Restore,
    /// A Mac booted into macOS. It exposes a USB gadget to a cabled host (the
    /// RemoteXPC/NCM sidecar) advertising its Apple serial number; its exact
    /// model and ECID are filled in separately by [`identify`].
    Booted,
    /// Any other Apple USB device (e.g. an iPhone in normal mode).
    Other,
}

impl UsbMode {
    fn from_pid(pid: u16) -> UsbMode {
        match pid {
            0x1227 => UsbMode::Dfu,
            0x1280 | 0x1281 => UsbMode::Recovery,
            0x1222 => UsbMode::Wtf,
            0x1338 | 0x1339 => UsbMode::Restore,
            // The RemoteXPC/NCM gadget a booted macOS exposes to a USB host.
            0x1902 => UsbMode::Booted,
            _ => UsbMode::Other,
        }
    }
}

impl fmt::Display for UsbMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            UsbMode::Dfu => "dfu",
            UsbMode::Recovery => "recovery",
            UsbMode::Wtf => "wtf",
            UsbMode::Restore => "restore",
            UsbMode::Booted => "booted",
            UsbMode::Other => "other",
        })
    }
}

/// Restore-family chip identity parsed from the `CPID:… BDID:…` USB serial
/// string Apple SoCs expose in DFU/recovery/restore modes. The ECID lives on
/// [`Device`] itself, since it is also obtainable for a booted Mac.
#[derive(Debug, Clone, Serialize)]
pub struct Identity {
    pub cpid: u16,
    pub bdid: u8,
    /// iBoot version tag (`SRTG`), when present.
    pub srtg: Option<String>,
    /// Resolved model, if the (cpid, bdid) pair is known.
    pub model: Option<MacModel>,
}

/// A connected Apple USB device, in whatever mode it is in.
///
/// Macs in a restore-family mode carry a parsed [`Identity`] (chip/board/model)
/// plus the ECID, all from the USB serial string. A Mac booted into macOS
/// enumerates as [`UsbMode::Booted`] with its name and Apple serial but no
/// [`Identity`]; its ECID can still be filled in by [`identify`]. Older targets
/// may not enumerate at all while booted.
#[derive(Debug, Clone, Serialize)]
pub struct Device {
    pub mode: UsbMode,
    /// USB product string, e.g. "Apple Mobile Device (DFU Mode)".
    pub product: String,
    /// Raw USB serial string (empty if the device exposes none).
    pub serial: String,
    /// The device's ECID, if known: parsed from the serial in restore modes, or
    /// filled in for a booted Mac by [`identify`].
    pub ecid: Option<u64>,
    /// Restore-family chip identity; present in DFU/recovery/restore, else None.
    pub identity: Option<Identity>,
    /// Exact Apple marketing name (e.g. "MacBook Pro (13-inch, M1, 2020)"),
    /// looked up from the serial by [`identify`]; falls back to the model's
    /// generic name in [`Device::display_name`] when absent.
    pub marketing_name: Option<String>,
    /// The host port this device is cabled to, filled in by [`identify`].
    /// `None` when undeterminable (non-macOS host, or the topology couldn't be
    /// read).
    pub port: Option<Port>,
    /// Whether the OS driver lets restorekit open this device. Always true on
    /// macOS/Linux; on Windows, false until WinUSB is bound.
    pub driver_ready: bool,
}

/// The host USB-C port a [`Device`] is cabled to.
#[derive(Debug, Clone, Serialize)]
pub struct Port {
    /// Whether this is the host's DFU-capable port — the one restorekit
    /// triggers DFU on. If `false`, move the cable to a DFU port to restore.
    pub dfu: bool,
    /// The port's physical location label from the firmware, e.g. "left-back",
    /// when the hardware provides one.
    pub location: Option<String>,
    /// The AppleHPM `RID` addressing this port — the value `dfu`/`reboot` take
    /// via `--port`.
    pub rid: i32,
}

impl Device {
    pub fn in_dfu(&self) -> bool {
        self.mode == UsbMode::Dfu
    }

    /// Whether restorekit can restore this device right now (DFU mode with a
    /// readable identity).
    pub fn restorable(&self) -> bool {
        self.in_dfu() && self.identity.is_some()
    }

    /// ECID formatted as idevicerestore expects it (hex with 0x prefix).
    pub fn ecid_hex(&self) -> Option<String> {
        self.ecid.map(|e| format!("0x{e:x}"))
    }

    /// Model identifier (e.g. "MacBookAir10,1"), if the model is known.
    pub fn identifier(&self) -> Option<&'static str> {
        self.identity.as_ref()?.model.map(|m| m.identifier)
    }

    /// iBoot version (`SRTG`), if the serial carried one.
    pub fn srtg(&self) -> Option<&str> {
        self.identity.as_ref()?.srtg.as_deref()
    }

    /// Human-readable name for display: the exact Apple marketing name when
    /// known, else the resolved model's generic name, else the USB product name.
    pub fn display_name(&self) -> String {
        if let Some(name) = &self.marketing_name {
            return name.clone();
        }
        match &self.identity {
            Some(Identity { model: Some(m), .. }) => m.name.to_string(),
            Some(i) => format!("Unknown Mac (CPID:{:04x} BDID:{:02x})", i.cpid, i.bdid),
            None => self.product.clone(),
        }
    }
}

/// Parse the restore-family USB serial string.
///
/// Example:
/// `CPID:8103 CPFM:03 SCEP:01 BDID:26 ECID:000C60A812345678 IBFL:3C SRTG:[iBoot-7429.61.2]`
///
/// Fields are space-separated `KEY:VALUE`; numeric values are hex, `SRTG` is
/// bracketed. Missing optional fields are tolerated; CPID/BDID/ECID are required.
pub fn parse_serial(serial: &str) -> Option<(u16, u8, u64, Option<String>)> {
    let mut cpid = None;
    let mut bdid = None;
    let mut ecid = None;
    let mut srtg = None;

    for field in serial.split_whitespace() {
        let (key, value) = field.split_once(':')?;
        match key {
            "CPID" => cpid = u16::from_str_radix(value, 16).ok(),
            "BDID" => bdid = u8::from_str_radix(value, 16).ok(),
            "ECID" => ecid = u64::from_str_radix(value, 16).ok(),
            "SRTG" => {
                srtg = Some(
                    value
                        .trim_start_matches('[')
                        .trim_end_matches(']')
                        .to_string(),
                )
            }
            _ => {}
        }
    }

    Some((cpid?, bdid?, ecid?, srtg))
}

pub(crate) fn from_usb(info: &nusb::DeviceInfo) -> Device {
    let serial = info.serial_number().unwrap_or("").to_string();
    let mode = UsbMode::from_pid(info.product_id());
    let parsed = parse_serial(&serial);
    let ecid = parsed.as_ref().map(|(_, _, ecid, _)| *ecid);
    let identity = parsed.map(|(cpid, bdid, _, srtg)| Identity {
        cpid,
        bdid,
        srtg,
        model: lookup(cpid, bdid).copied(),
    });
    Device {
        driver_ready: driver_ready(info, mode),
        mode,
        product: info.product_string().unwrap_or("Apple device").to_string(),
        serial,
        ecid,
        identity,
        marketing_name: None,
        port: None,
    }
}

/// Whether the OS driver lets restorekit open this device. Only the
/// restore-family modes need WinUSB on Windows; never poke a normal device.
#[cfg(target_os = "windows")]
fn driver_ready(info: &nusb::DeviceInfo, mode: UsbMode) -> bool {
    match mode {
        UsbMode::Other => true,
        _ => info.open().wait().is_ok(),
    }
}
#[cfg(not(target_os = "windows"))]
fn driver_ready(_info: &nusb::DeviceInfo, _mode: UsbMode) -> bool {
    true
}

/// List every connected Apple USB device, in any mode. Cheap enumeration only;
/// call [`identify`] to fill in the richer fields (model, ECID, marketing name,
/// DFU-port).
pub fn list() -> Result<Vec<Device>> {
    let infos = nusb::list_devices()
        .wait()
        .map_err(|e| Error::Usb(e.to_string()))?;
    Ok(infos
        .filter(|i| i.vendor_id() == APPLE_VID)
        .map(|i| from_usb(&i))
        .collect())
}

/// Which device to pick.
#[derive(Debug, Clone, Copy)]
pub enum Target {
    /// The only Mac in DFU mode — the implied restore target; it is an error
    /// if several are connected.
    One,
    /// The device with this ECID, in any mode that exposes one.
    Ecid(u64),
}

/// Pick the target from `devices`. `Ok(None)` means "not there (yet)";
/// `Err` means the target can never match (ambiguous).
fn select(devices: Vec<Device>, target: Target) -> Result<Option<Device>> {
    match target {
        Target::One => {
            let mut dfu: Vec<Device> = devices.into_iter().filter(|d| d.restorable()).collect();
            match dfu.len() {
                0 | 1 => Ok(dfu.pop()),
                n => Err(Error::MultipleDevices(n)),
            }
        }
        Target::Ecid(e) => Ok(devices.into_iter().find(|d| d.ecid == Some(e))),
    }
}

/// Find the target among the currently connected devices.
pub fn find(target: Target) -> Result<Device> {
    select(list()?, target)?.ok_or(match target {
        Target::Ecid(e) => Error::EcidNotFound(e),
        Target::One => Error::NoDeviceFound,
    })
}

/// Poll until the target is connected, or the timeout elapses.
pub fn wait(target: Target, timeout: Duration) -> Result<Device> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(dev) = select(list()?, target)? {
            return Ok(dev);
        }
        if Instant::now() >= deadline {
            return Err(Error::WaitTimeout);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Enrich the devices [`list`] returned with everything else we can determine —
/// the same facts Apple Configurator surfaces:
///
/// - **Port** (macOS): which host port each device is on and whether it's the
///   DFU-capable one (`port`), from read-only IORegistry topology.
/// - **Model** (booted Macs): the `bcdDevice` release number encodes the model
///   identifier (see [`model_from_device_version`]); no device open needed.
/// - **Marketing name** (booted Macs): the exact Apple name from the serial's
///   config code (network, cached per serial).
/// - **ECID** (booted Macs): advertised in a platform-capability descriptor in
///   the USB BOS descriptor (the value macOS also exposes as
///   `UsbAppleDeviceECID`), read with a standard `GET_DESCRIPTOR` request.
///
/// Best-effort: a device we can't open keeps `ecid == None` (the ECID is then
/// free from the USB serial once the Mac enters DFU or recovery); the model
/// still resolves without opening. Never errors; leaves known fields untouched.
pub fn identify(devices: &mut [Device]) {
    // DFU-port detection applies to every device (macOS only).
    #[cfg(target_os = "macos")]
    crate::dfu::port::mark_ports(devices);

    // The rest enriches booted Macs, which alone need it.
    let needs = |d: &Device| {
        d.mode == UsbMode::Booted
            && (d.identity.is_none() || d.ecid.is_none())
            && !d.serial.is_empty()
    };
    if !devices.iter().any(needs) {
        return;
    }
    let Ok(infos) = nusb::list_devices().wait() else {
        return;
    };
    let apple: Vec<nusb::DeviceInfo> = infos.filter(|i| i.vendor_id() == APPLE_VID).collect();
    for d in devices.iter_mut() {
        if !needs(d) {
            continue;
        }
        let Some(info) = apple
            .iter()
            .find(|i| i.serial_number() == Some(d.serial.as_str()))
        else {
            continue;
        };
        // Model comes from the bcdDevice release number — no open needed.
        if d.identity.is_none() {
            if let Some(m) = model_from_device_version(info.device_version()) {
                d.identity = Some(Identity {
                    cpid: m.cpid,
                    bdid: m.bdid,
                    srtg: None,
                    model: Some(*m),
                });
            }
        }
        // Exact Apple marketing name from the serial's config code (network,
        // cached per serial); display falls back to the model name without it.
        d.marketing_name = apple_marketing_name(&d.serial);
        // ECID needs to open the device and read its BOS descriptor.
        if d.ecid.is_none() {
            d.ecid = info.open().wait().ok().and_then(|dev| read_bos_ecid(&dev));
        }
    }
}

/// The exact Apple marketing name for a Mac serial, from Apple's config-code
/// service — the same source Apple Configurator uses. Best-effort and cached
/// per serial (a machine's name never changes): returns `None` on any network
/// or parse failure, or for serial formats without a positional config code, so
/// callers fall back to the local model name.
fn apple_marketing_name(serial: &str) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(serial) {
        return hit.clone();
    }
    let name = fetch_marketing_name(serial);
    cache
        .lock()
        .unwrap()
        .insert(serial.to_string(), name.clone());
    name
}

fn fetch_marketing_name(serial: &str) -> Option<String> {
    // Config code: the last 4 chars of a 12-char serial, last 3 of an 11-char
    // one (both start at index 8). Randomized/other formats have no positional
    // code — skip them.
    let cc = match serial.len() {
        11 | 12 => serial.get(8..)?,
        _ => return None,
    };
    let body = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .user_agent(concat!("restorekit/", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()?
        .get(format!("https://support-sp.apple.com/sp/product?cc={cc}"))
        .send()
        .ok()?
        .text()
        .ok()?;
    parse_config_code_name(&body)
}

/// Extract the marketing name from the config-code service's XML response.
/// Returns `None` for the "CPU Name" placeholder the service echoes for an
/// unknown code (a valid name always contains "Mac").
fn parse_config_code_name(body: &str) -> Option<String> {
    let start = body.find("<configCode>")? + "<configCode>".len();
    let end = start + body[start..].find("</configCode>")?;
    let name = body[start..end].trim();
    name.contains("Mac").then(|| name.to_string())
}

/// Apple's platform-capability UUID whose payload is the device's ECID.
// Only reached via the BOS reader (skipped on Windows) and the parser test.
#[cfg_attr(target_os = "windows", allow(dead_code))]
const APPLE_ECID_UUID: [u8; 16] = [
    0x0a, 0x37, 0x4c, 0xe4, 0x76, 0x23, 0x47, 0xc9, 0x88, 0x0e, 0x1a, 0xc3, 0x15, 0x13, 0x47, 0x6f,
];

/// Read the device's BOS descriptor and extract the ECID from Apple's platform
/// capability, via a standard `GET_DESCRIPTOR(BOS)` control request.
///
/// `nusb` exposes device-level control transfers only on Linux and macOS; the
/// Windows WinUSB backend routes them through a claimed interface. The
/// booted-Mac ECID is a best-effort nicety — it's resolved for free the moment
/// the Mac enters DFU — so on Windows we skip it rather than claim an interface
/// just to read a descriptor.
#[cfg(not(target_os = "windows"))]
fn read_bos_ecid(dev: &nusb::Device) -> Option<u64> {
    let bos = dev
        .control_in(
            nusb::transfer::ControlIn {
                control_type: nusb::transfer::ControlType::Standard,
                recipient: nusb::transfer::Recipient::Device,
                request: 0x06, // GET_DESCRIPTOR
                value: 0x0f00, // descriptor type 0x0F (BOS), index 0
                index: 0,
                length: 256,
            },
            Duration::from_millis(500),
        )
        .wait()
        .ok()?;
    ecid_from_bos(&bos)
}

#[cfg(target_os = "windows")]
fn read_bos_ecid(_dev: &nusb::Device) -> Option<u64> {
    None
}

/// Parse a BOS descriptor and return the ECID from Apple's platform-capability
/// descriptor (`bDevCapabilityType` = 5 PLATFORM, matching [`APPLE_ECID_UUID`],
/// with an 8-byte little-endian ECID payload).
// Only reached via the BOS reader (skipped on Windows) and the parser test.
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn ecid_from_bos(bos: &[u8]) -> Option<u64> {
    // 5-byte BOS header, then a sequence of capability descriptors.
    let mut i = 5;
    while i + 3 <= bos.len() {
        let len = bos[i] as usize;
        if len < 3 || i + len > bos.len() {
            break;
        }
        let cap = &bos[i..i + len];
        // DEVICE_CAPABILITY (0x10), PLATFORM (0x05), 4-byte header + 16-byte
        // UUID + 8-byte payload.
        if cap[1] == 0x10 && cap[2] == 0x05 && len >= 28 && cap[4..20] == APPLE_ECID_UUID {
            return Some(u64::from_le_bytes(cap[20..28].try_into().unwrap()));
        }
        i += len;
    }
    None
}

/// Poll until `pred` matches a connected device, or the timeout elapses.
/// E.g. "this ECID, in DFU mode": `wait_where(t, |d| d.ecid == Some(e) && d.in_dfu())`.
pub fn wait_where(timeout: Duration, mut pred: impl FnMut(&Device) -> bool) -> Result<Device> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(dev) = list()?.into_iter().find(&mut pred) {
            return Ok(dev);
        }
        if Instant::now() >= deadline {
            return Err(Error::WaitTimeout);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_m1_air() {
        let m = lookup(0x8103, 0x26).unwrap();
        assert_eq!(m.board, "J313AP");
        assert_eq!(m.identifier, "MacBookAir10,1");
    }

    #[test]
    fn lookup_unknown() {
        assert!(lookup(0xffff, 0xff).is_none());
    }

    #[test]
    fn lookup_by_identifier_case_insensitive() {
        assert_eq!(lookup_identifier("macmini9,1").unwrap().board, "J274AP");
    }

    #[test]
    fn no_duplicate_keys() {
        let mut keys: Vec<_> = MAC_MODELS.iter().map(|m| (m.cpid, m.bdid)).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(n, keys.len());
    }

    #[test]
    fn model_from_device_version_decodes_bcd() {
        // 0x1701 → "17,1" → MacBookPro17,1 (a booted M1 MacBook Pro).
        assert_eq!(
            model_from_device_version(0x1701).map(|m| m.identifier),
            Some("MacBookPro17,1")
        );
        // Newer Macs use the "Mac16,6"-style identifier.
        assert_eq!(
            model_from_device_version(0x1606).map(|m| m.identifier),
            Some("Mac16,6")
        );
        // Two-digit minor: 0x1415 → "14,15" → Mac14,15 (15" MacBook Air M2).
        assert_eq!(
            model_from_device_version(0x1415).map(|m| m.identifier),
            Some("Mac14,15")
        );
        assert_eq!(model_from_device_version(0x9999), None);
    }

    #[test]
    fn parses_config_code_name() {
        // Real response for the M1 MacBook Pro's config code (cc=Q05P).
        let ok = "<?xml version=\"1.0\"?><root><name>CPU Name</name>\
            <configCode>MacBook Pro (13-inch, M1, 2020)</configCode><locale>en_US</locale></root>";
        assert_eq!(
            parse_config_code_name(ok).as_deref(),
            Some("MacBook Pro (13-inch, M1, 2020)")
        );
        // Unknown code: the service echoes "CPU Name" — treated as no result.
        let placeholder = "<root><configCode>CPU Name</configCode></root>";
        assert_eq!(parse_config_code_name(placeholder), None);
        assert_eq!(parse_config_code_name("<root></root>"), None);
    }

    #[test]
    fn parses_ecid_from_bos_descriptor() {
        // Real BOS descriptor from a booted MacBook Pro (ECID 0x445E11462001E).
        let bos = [
            0x05u8, 0x0f, 0x46, 0x00, 0x04, // BOS header, 4 capabilities
            0x07, 0x10, 0x02, 0x02, 0x00, 0x00, 0x00, // USB 2.0 extension
            0x1c, 0x10, 0x05, 0x00, // platform capability, len 28
            0x0a, 0x37, 0x4c, 0xe4, 0x76, 0x23, 0x47, 0xc9, 0x88, 0x0e, 0x1a, 0xc3, 0x15, 0x13,
            0x47, 0x6f, // Apple ECID UUID
            0x1e, 0x00, 0x62, 0x14, 0xe1, 0x45, 0x04, 0x00, // ECID (little-endian)
            0x0a, 0x10, 0x03, 0x00, 0x0e, 0x00, 0x01, 0x0a, 0xff, 0x07, // another capability
        ];
        assert_eq!(ecid_from_bos(&bos), Some(0x445E11462001E));
        assert_eq!(ecid_from_bos(&[0x05, 0x0f, 0x05, 0x00, 0x00]), None);
    }

    #[test]
    fn parses_full_serial() {
        let s = "CPID:8103 CPFM:03 SCEP:01 BDID:26 ECID:000C60A812345678 IBFL:3C SRTG:[iBoot-7429.61.2]";
        let (cpid, bdid, ecid, srtg) = parse_serial(s).unwrap();
        assert_eq!(cpid, 0x8103);
        assert_eq!(bdid, 0x26);
        assert_eq!(ecid, 0x000C60A812345678);
        assert_eq!(srtg.as_deref(), Some("iBoot-7429.61.2"));
    }

    #[test]
    fn missing_required_field_fails() {
        assert!(parse_serial("CPID:8103 BDID:26").is_none());
    }

    fn dev(mode: UsbMode, ecid: u64) -> Device {
        Device {
            mode,
            product: "Apple Mobile Device".into(),
            serial: String::new(),
            ecid: Some(ecid),
            identity: Some(Identity {
                cpid: 0x8103,
                bdid: 0x26,
                srtg: None,
                model: lookup(0x8103, 0x26).copied(),
            }),
            marketing_name: None,
            port: None,
            driver_ready: true,
        }
    }

    #[test]
    fn device_accessors() {
        let d = dev(UsbMode::Dfu, 0x1234);
        assert!(d.restorable());
        assert_eq!(d.identifier(), Some("MacBookAir10,1"));
        assert_eq!(d.ecid_hex().as_deref(), Some("0x1234"));
        assert!(!dev(UsbMode::Recovery, 1).restorable());
    }

    #[test]
    fn select_one_only_matches_dfu() {
        assert!(select(vec![], Target::One).unwrap().is_none());
        assert!(select(vec![dev(UsbMode::Recovery, 1)], Target::One)
            .unwrap()
            .is_none());
        let picked = select(
            vec![dev(UsbMode::Recovery, 1), dev(UsbMode::Dfu, 2)],
            Target::One,
        )
        .unwrap()
        .unwrap();
        assert_eq!(picked.ecid, Some(2));
        assert!(matches!(
            select(
                vec![dev(UsbMode::Dfu, 1), dev(UsbMode::Dfu, 2)],
                Target::One
            ),
            Err(Error::MultipleDevices(2))
        ));
    }

    #[test]
    fn select_ecid_matches_any_mode() {
        let devices = vec![dev(UsbMode::Recovery, 1), dev(UsbMode::Dfu, 2)];
        let found = select(devices.clone(), Target::Ecid(1)).unwrap().unwrap();
        assert_eq!(found.mode, UsbMode::Recovery);
        assert!(select(devices, Target::Ecid(3)).unwrap().is_none());
    }
}
