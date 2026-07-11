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
//! the iProduct string descriptor. To add a new model (Dongle Lite, Dongle
//! Pro, RecoverKit Pro, ...):
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
/// iProduct string of the Dongle Proto Lite.
pub const PRODUCT_PROTO_LITE: &str = "Dongle-Proto-Lite";
/// iSerial prefix of the Dongle Proto Lite (e.g. `DPL-1A2B3C4D`).
pub const SERIAL_PREFIX_PROTO_LITE: &str = "DPL-";

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
