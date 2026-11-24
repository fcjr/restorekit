//! USB PD message helpers and the Apple vendor messages, as in dongle-lite-fw.

pub const REV20: u16 = 1;

pub const DATA_SOURCE_CAP: u16 = 1;
pub const DATA_REQUEST: u16 = 2;
pub const DATA_SINK_CAP: u16 = 4;
pub const DATA_VENDOR_DEF: u16 = 15;

pub const CTRL_ACCEPT: u16 = 3;
pub const CTRL_REJECT: u16 = 4;
pub const CTRL_PS_RDY: u16 = 6;
pub const CTRL_GET_SOURCE_CAP: u16 = 7;
pub const CTRL_GET_SINK_CAP: u16 = 8;
pub const CTRL_DR_SWAP: u16 = 9;
pub const CTRL_PR_SWAP: u16 = 10;
pub const CTRL_VCONN_SWAP: u16 = 11;
pub const CTRL_SOFT_RESET: u16 = 13;

/// Header for a source/DFP message. The message id is filled in at transmit.
pub const fn header(msg_type: u16, cnt: u16) -> u16 {
    msg_type | REV20 << 6 | 1 << 5 | 1 << 8 | cnt << 12
}
pub const fn cnt(header: u16) -> usize {
    (header >> 12 & 7) as usize
}
pub const fn kind(header: u16) -> u16 {
    header & 0x1F
}

/// Fixed-supply source PDO. `first` adds the flags that only go in PDO 1.
pub const fn fixed_pdo(mv: u32, ma: u32, first: bool) -> u32 {
    let flags = if first { 1 << 27 | 1 << 26 } else { 0 };
    flags | (mv / 50) << 10 | ma / 10
}
/// (millivolts, milliamps) of a fixed PDO, or None for the other kinds.
#[cfg_attr(not(feature = "pd"), allow(dead_code))]
pub const fn fixed(pdo: u32) -> Option<(u32, u32)> {
    if pdo >> 30 != 0 {
        return None;
    }
    Some(((pdo >> 10 & 0x3FF) * 50, (pdo & 0x3FF) * 10))
}

// Apple vendor-defined messages (SVID 0x05AC), matching macvdmtool. All go out
// on SOP'' Debug.
pub const VDM_DFU_HOLD: [u32; 3] = [0x5ac_8012, 0x0106, 0x8001_0000];
// T2 Macs NAK 0x0106; they take the reboot action with PMU reset + DFU hold.
pub const VDM_DFU_HOLD_T2: [u32; 3] = [0x5ac_8012, 0x0105, 0x8002_0000];
pub const VDM_REBOOT: [u32; 3] = [0x5ac_8012, 0x0105, 0x8000_0000];
pub const VDM_SERIAL_SBU: [u32; 2] = [0x5ac_8012, 0x0184_0306];
pub const VDM_DEBUGUSB: [u32; 2] = [0x5ac_8012, 0x0182_4606];
