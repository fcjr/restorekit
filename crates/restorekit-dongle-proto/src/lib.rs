//! The USB contract between RecoverKit dongles and the host SDK.
//!
//! Single source of truth for everything both sides must agree on: the
//! VID/PID, the USB string descriptors, and the vendor-interface control
//! protocol. The dongle firmware (`crates/dongle-lite-fw`) and the host
//! client (`restorekit::dongle`) both link this crate, so the two can't
//! drift apart.
//!
//! # Identification
//!
//! Every RecoverKit device enumerates as [`VID`]:[`PID`]. The PID is unique
//! to RecoverKit but shared across models — the specific model is carried in
//! the iProduct string descriptor. To add a new model (Dongle Pro,
//! RecoverKit Pro, ...):
//!
//! 1. Add a `PRODUCT_*` constant here with a unique model name, and set it as
//!    `config.product` in that model's firmware (keeping [`VID`]:[`PID`]).
//! 2. Add a variant to `DongleModel` in `restorekit::dongle` and a match arm
//!    in its `from_product`.
//!
//! Discovery, udev rules, and the vendor protocol all key off the shared
//! VID/PID and need no changes.

#![no_std]

/// USB vendor ID (MCS Electronics).
pub const VID: u16 = 0x16D0;
/// USB product ID assigned to RecoverKit. Shared by every RecoverKit model;
/// the model is identified by the iProduct string, not the PID.
pub const PID: u16 = 0x14F0;

/// iManufacturer string on every RecoverKit device.
pub const MANUFACTURER: &str = "RecoverKit";
/// iProduct string of the Dongle Lite.
pub const PRODUCT_LITE: &str = "Dongle-Lite";
/// iSerial prefix of the Dongle Lite (e.g. `DL-1A2B3C4D`).
pub const SERIAL_PREFIX_LITE: &str = "DL-";

/// Class of the vendor-specific interface the SDK drives.
pub const VENDOR_CLASS: u8 = 0xFF;

// bRequest values on the vendor interface (vendor type, interface recipient).
/// Control OUT: execute a command; wValue = `VCMD_*`.
pub const VREQ_CMD: u8 = 0x01;
/// Control IN: read the status struct.
pub const VREQ_STATUS: u8 = 0x02;

// Command codes carried in wValue on `VREQ_CMD`.
pub const VCMD_NOP: u16 = 0;
pub const VCMD_DFU: u16 = 1;
pub const VCMD_REBOOT: u16 = 2;
pub const VCMD_SERIAL: u16 = 3;
pub const VCMD_DEBUGUSB: u16 = 4;
/// Reboot the dongle itself into its USB bootloader for a firmware update.
/// Fire-and-forget: the device drops off the bus instead of reporting a
/// result, so hosts must not poll the status after sending it.
pub const VCMD_BOOTSEL: u16 = 5;

// `VREQ_STATUS` response: [version, pd_state, flags, last_result, seq].
/// Current status struct version (byte 0).
pub const STATUS_VERSION: u8 = 1;
/// Length of the status struct.
pub const STATUS_LEN: usize = 5;

// PD state codes (status byte 1).
pub const PD_DISCONNECTED: u8 = 0;
pub const PD_VBUS_ON: u8 = 1;
pub const PD_CONNECTED: u8 = 2;
pub const PD_ACCEPT: u8 = 3;
pub const PD_IDLE: u8 = 4;

// Status flag bits (status byte 2).
pub const FLAG_TARGET_ATTACHED: u8 = 1 << 0;
pub const FLAG_POLARITY_CC2: u8 = 1 << 1;

// Result codes of the last command (status byte 3).
pub const RES_NONE: u8 = 0;
pub const RES_PENDING: u8 = 1;
pub const RES_OK: u8 = 2;
pub const RES_NOTARGET: u8 = 3;
/// Reserved; emitted only by old firmware. Apple action VDMs don't return a
/// GoodCRC, so absence of one is no longer treated as a failure.
pub const RES_NOACK: u8 = 4;

// Firmware update over the vendor interface (no bootrom, no RPI-RP2 drive).
// The dongle runs behind an embassy-boot bootloader with A/B slots: the host
// streams the new image into the inactive slot, the dongle verifies its CRC,
// marks it for swap, and reboots; the bootloader swaps power-fail-safely and
// reverts unless the new firmware boots far enough to mark itself healthy.
//
// Flow: FW_BEGIN (image size) -> FW_DATA x N (sequential [`FW_CHUNK`]-sized
// chunks, the last one padded with 0xFF) -> FW_DONE (CRC-32 of the unpadded
// image). Each request completes only once its bytes are in flash, so the
// transfer is self-flow-controlled. A rejected request aborts the update.
/// Control OUT: start an update; data = image size in bytes (u32 LE).
pub const VREQ_FW_BEGIN: u8 = 0x10;
/// Control OUT: one image chunk; wValue = chunk index (sequential from 0),
/// data = exactly [`FW_CHUNK`] bytes.
pub const VREQ_FW_DATA: u8 = 0x11;
/// Control OUT: finish; data = CRC-32 (u32 LE, [`crc32`]) of the image. On
/// success the dongle marks the update and reboots into the new firmware.
pub const VREQ_FW_DONE: u8 = 0x12;
/// Update chunk size: one RP2040 flash erase sector.
pub const FW_CHUNK: usize = 4096;

/// Pack a `MAJOR.MINOR.PATCH` firmware version into the USB `bcdDevice`
/// field, so hosts can read the version without opening the device. Layout:
/// major in the high byte, minor and patch in one nibble each — so minor and
/// patch must stay ≤ 15 (bump the major instead of running past it).
pub const fn encode_bcd_version(v: &str) -> u16 {
    let b = v.as_bytes();
    let mut parts = [0u16; 3];
    let mut part = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'.' {
            part += 1;
        } else {
            parts[part] = parts[part] * 10 + (b[i] - b'0') as u16;
        }
        i += 1;
    }
    assert!(parts[0] <= 0xFF && parts[1] <= 0xF && parts[2] <= 0xF);
    (parts[0] << 8) | (parts[1] << 4) | parts[2]
}

/// The inverse of [`encode_bcd_version`]: `(major, minor, patch)`.
pub const fn decode_bcd_version(bcd: u16) -> (u8, u8, u8) {
    (
        (bcd >> 8) as u8,
        ((bcd >> 4) & 0xF) as u8,
        (bcd & 0xF) as u8,
    )
}

/// CRC-32 (IEEE 802.3, the zlib/`crc32fast` polynomial), used to verify a
/// streamed firmware image. Bitwise and dependency-free so the dongle and the
/// host compute it from the same code.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcd_version_roundtrip() {
        assert_eq!(decode_bcd_version(encode_bcd_version("0.1.0")), (0, 1, 0));
        assert_eq!(decode_bcd_version(encode_bcd_version("2.15.9")), (2, 15, 9));
        assert_eq!(decode_bcd_version(encode_bcd_version("99.0.3")), (99, 0, 3));
    }

    #[test]
    fn crc32_matches_ieee() {
        // Well-known vector: crc32(b"123456789") = 0xCBF43926.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
