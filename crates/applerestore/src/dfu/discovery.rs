use std::time::{Duration, Instant};

use nusb::MaybeFuture;
use serde::Serialize;

use crate::device::{self, MacModel};
use crate::error::{Error, Result};

/// Apple's USB vendor ID.
pub const APPLE_VID: u16 = 0x05ac;
/// Product ID presented by an Apple SoC in DFU mode.
pub const DFU_PID: u16 = 0x1227;

/// A Mac detected in DFU mode.
#[derive(Debug, Clone, Serialize)]
pub struct DfuDevice {
    pub cpid: u16,
    pub bdid: u8,
    pub ecid: u64,
    pub serial: String,
    pub srtg: Option<String>,
    /// Resolved model, if the (cpid, bdid) pair is known.
    pub model: Option<MacModel>,
}

impl DfuDevice {
    /// Model identifier (e.g. "MacBookAir10,1"), if the model is known.
    pub fn identifier(&self) -> Option<&'static str> {
        self.model.map(|m| m.identifier)
    }

    /// Human-readable name for display.
    pub fn display_name(&self) -> String {
        match self.model {
            Some(m) => m.name.to_string(),
            None => format!(
                "Unknown Mac (CPID:{:04x} BDID:{:02x})",
                self.cpid, self.bdid
            ),
        }
    }

    /// ECID formatted as idevicerestore expects it (hex with 0x prefix).
    pub fn ecid_hex(&self) -> String {
        format!("0x{:x}", self.ecid)
    }
}

/// Parse the DFU-mode USB serial string.
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

fn to_dfu_device(serial: &str) -> Option<DfuDevice> {
    let (cpid, bdid, ecid, srtg) = parse_serial(serial)?;
    Some(DfuDevice {
        cpid,
        bdid,
        ecid,
        serial: serial.to_string(),
        srtg,
        model: device::lookup(cpid, bdid).copied(),
    })
}

/// List every Mac currently in DFU mode.
pub fn list() -> Result<Vec<DfuDevice>> {
    let devices = nusb::list_devices()
        .wait()
        .map_err(|e| Error::Usb(e.to_string()))?;

    let mut out = Vec::new();
    for info in devices {
        if info.vendor_id() != APPLE_VID || info.product_id() != DFU_PID {
            continue;
        }
        if let Some(serial) = info.serial_number() {
            if let Some(dev) = to_dfu_device(serial) {
                out.push(dev);
            }
        }
    }
    Ok(out)
}

/// Return the single Mac in DFU mode, erroring if there are zero or many.
pub fn find_one() -> Result<DfuDevice> {
    let mut devices = list()?;
    match devices.len() {
        0 => Err(Error::NoDeviceFound),
        1 => Ok(devices.pop().unwrap()),
        n => Err(Error::MultipleDevices(n)),
    }
}

/// Poll until exactly one Mac is in DFU mode, or the timeout elapses.
pub fn wait_for_dfu(timeout: Duration) -> Result<DfuDevice> {
    let deadline = Instant::now() + timeout;
    loop {
        match list()?.into_iter().next() {
            Some(dev) => return Ok(dev),
            None if Instant::now() >= deadline => return Err(Error::WaitTimeout),
            None => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn resolves_model() {
        let s = "CPID:8103 BDID:26 ECID:1234";
        let dev = to_dfu_device(s).unwrap();
        assert_eq!(dev.identifier(), Some("MacBookAir10,1"));
        assert_eq!(dev.ecid_hex(), "0x1234");
    }

    #[test]
    fn unknown_model_still_parses() {
        let s = "CPID:ffff BDID:ff ECID:1";
        let dev = to_dfu_device(s).unwrap();
        assert!(dev.model.is_none());
        assert!(dev.display_name().contains("Unknown"));
    }

    #[test]
    fn missing_required_field_fails() {
        assert!(parse_serial("CPID:8103 BDID:26").is_none());
    }
}
