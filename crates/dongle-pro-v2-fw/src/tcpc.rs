//! HUSB311 over TCPCI. Standard TCPCI registers only, plus the RT1711H-style
//! soft reset the HUSB311 copies. Anything vendor-specific is left at its
//! reset value: poke it from the console (`tr`, `tw`) while bringing a board up.

use crate::i2c::I2c;
use embassy_time::Timer;

pub const ADDR: u8 = 0x4E;

pub const VENDOR_ID: u8 = 0x00;
pub const ALERT: u8 = 0x10;
pub const ALERT_MASK: u8 = 0x12;
pub const POWER_STATUS_MASK: u8 = 0x14;
pub const TCPC_CONTROL: u8 = 0x19;
pub const ROLE_CONTROL: u8 = 0x1A;
pub const CC_STATUS: u8 = 0x1D;
pub const POWER_STATUS: u8 = 0x1E;
pub const FAULT_STATUS: u8 = 0x1F;
pub const MESSAGE_HEADER_INFO: u8 = 0x2E;
pub const RECEIVE_DETECT: u8 = 0x2F;
pub const RX_BYTE_COUNT: u8 = 0x30;
pub const TRANSMIT: u8 = 0x50;
pub const TX_BYTE_COUNT: u8 = 0x51;
pub const SOFT_RESET: u8 = 0xA0;

pub const ALERT_CC: u16 = 1 << 0;
pub const ALERT_POWER: u16 = 1 << 1;
pub const ALERT_RX: u16 = 1 << 2;
pub const ALERT_RX_HARD_RESET: u16 = 1 << 3;
pub const ALERT_TX_FAIL: u16 = 1 << 4;
pub const ALERT_TX_DISCARD: u16 = 1 << 5;
pub const ALERT_TX_OK: u16 = 1 << 6;
pub const ALERT_FAULT: u16 = 1 << 9;
pub const ALERT_RX_OVERFLOW: u16 = 1 << 10;

/// Frame types, the same numbers in TRANSMIT and RX_BUF_FRAME_TYPE.
pub const SOP: u8 = 0;
pub const SOP_DBG2: u8 = 4;
pub const HARD_RESET: u8 = 5;

/// CC states while presenting Rp.
pub const CC_RD: u8 = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tx {
    Ok,
    Failed,
    Discarded,
    /// No TX alert within 100 ms.
    Timeout,
    /// The chip did not acknowledge on I2C.
    Bus,
}
impl Tx {
    pub fn name(self) -> &'static str {
        match self {
            Tx::Ok => "ok",
            Tx::Failed => "no GoodCRC",
            Tx::Discarded => "discarded",
            Tx::Timeout => "timeout",
            Tx::Bus => "i2c error",
        }
    }
}

pub struct Rx {
    pub frame: u8,
    pub header: u16,
    pub data: [u32; 7],
}

pub struct Tcpc<'d> {
    pub bus: I2c<'d>,
    msg_id: [u8; 5],
}

impl<'d> Tcpc<'d> {
    pub fn new(bus: I2c<'d>) -> Self {
        Self {
            bus,
            msg_id: [0; 5],
        }
    }

    pub fn r8(&mut self, reg: u8) -> Option<u8> {
        let mut b = [0];
        self.bus.read(ADDR, reg, &mut b).then_some(b[0])
    }
    pub fn r16(&mut self, reg: u8) -> Option<u16> {
        let mut b = [0; 2];
        self.bus
            .read(ADDR, reg, &mut b)
            .then(|| u16::from_le_bytes(b))
    }
    pub fn w8(&mut self, reg: u8, value: u8) -> bool {
        self.bus.write(ADDR, &[reg, value])
    }
    pub fn w16(&mut self, reg: u8, value: u16) -> bool {
        self.bus
            .write(ADDR, &[reg, value as u8, (value >> 8) as u8])
    }

    /// Reset the chip and leave it presenting open CC with alerts unmasked.
    /// Returns (vendor, product, device) ids.
    pub async fn init(&mut self) -> Option<(u16, u16, u16)> {
        let ids = (
            self.r16(VENDOR_ID)?,
            self.r16(VENDOR_ID + 2)?,
            self.r16(VENDOR_ID + 4)?,
        );
        self.w8(SOFT_RESET, 1);
        Timer::after_millis(5).await;
        self.w8(ROLE_CONTROL, 0x0F);
        self.w8(RECEIVE_DETECT, 0);
        self.w16(ALERT, 0xFFFF);
        self.w8(FAULT_STATUS, 0xFF);
        self.w8(POWER_STATUS_MASK, 1 << 2);
        self.w16(
            ALERT_MASK,
            ALERT_CC
                | ALERT_POWER
                | ALERT_RX
                | ALERT_RX_HARD_RESET
                | ALERT_TX_FAIL
                | ALERT_TX_DISCARD
                | ALERT_TX_OK
                | ALERT_FAULT
                | ALERT_RX_OVERFLOW,
        );
        Some(ids)
    }

    /// Rp on both CC lines; `level` 0 = default USB, 1 = 1.5 A, 2 = 3 A.
    pub fn set_rp(&mut self, level: u8) {
        self.w8(ROLE_CONTROL, 0x05 | (level & 3) << 4);
    }
    pub fn set_open(&mut self) {
        self.w8(ROLE_CONTROL, 0x0F);
    }
    /// (cc1, cc2) states.
    pub fn cc(&mut self) -> Option<(u8, u8)> {
        let s = self.r8(CC_STATUS)?;
        Some((s & 3, s >> 2 & 3))
    }
    pub fn vbus_present(&mut self) -> Option<bool> {
        Some(self.r8(POWER_STATUS)? & 1 << 2 != 0)
    }

    /// Source/DFP session on the given CC line: message header, plug
    /// orientation, receive SOP, SOP'' Debug and hard reset.
    pub fn start_session(&mut self, cc2: bool) {
        self.msg_id = [0; 5];
        self.w8(TCPC_CONTROL, cc2 as u8);
        self.w8(MESSAGE_HEADER_INFO, 0x0B);
        self.w8(RECEIVE_DETECT, 1 << 0 | 1 << 4 | 1 << 5);
    }
    pub fn end_session(&mut self) {
        self.w8(RECEIVE_DETECT, 0);
        self.w16(ALERT, 0xFFFF);
    }
    pub fn reset_msg_ids(&mut self) {
        self.msg_id = [0; 5];
    }

    /// Send one message and wait for the chip to report how it went. The
    /// HUSB311 wants byte count, header and payload in one write.
    pub async fn transmit(&mut self, frame: u8, header: u16, data: &[u32]) -> Tx {
        let id = self.msg_id[usize::from(frame.min(4))];
        let header = header | u16::from(id) << 9;
        let mut buf = [0u8; 32];
        buf[0] = TX_BYTE_COUNT;
        buf[1] = 2 + 4 * data.len() as u8;
        buf[2..4].copy_from_slice(&header.to_le_bytes());
        for (i, w) in data.iter().enumerate() {
            buf[4 + 4 * i..8 + 4 * i].copy_from_slice(&w.to_le_bytes());
        }
        self.w16(ALERT, ALERT_TX_OK | ALERT_TX_FAIL | ALERT_TX_DISCARD);
        if !(self.bus.write(ADDR, &buf[..4 + 4 * data.len()]) && self.w8(TRANSMIT, 3 << 4 | frame))
        {
            return Tx::Bus;
        }
        for _ in 0..50 {
            Timer::after_millis(2).await;
            let Some(alert) = self.r16(ALERT) else {
                return Tx::Bus;
            };
            let done = alert & (ALERT_TX_OK | ALERT_TX_FAIL | ALERT_TX_DISCARD);
            if done != 0 {
                self.w16(ALERT, done);
                return if done & ALERT_TX_OK != 0 && done & ALERT_TX_FAIL == 0 {
                    self.msg_id[usize::from(frame.min(4))] = (id + 1) & 7;
                    Tx::Ok
                } else if done & ALERT_TX_DISCARD != 0 {
                    Tx::Discarded
                } else {
                    Tx::Failed
                };
            }
        }
        Tx::Timeout
    }

    /// Hard reset signalling.
    pub fn hard_reset(&mut self) {
        self.w8(TRANSMIT, HARD_RESET);
    }

    /// Take the message the RX alert announced, and release the buffer.
    pub fn receive(&mut self) -> Option<Rx> {
        // raw: count, frame type, header (2), payload. Read only what the
        // count says: on a bit-banged bus the full buffer costs too long.
        let mut raw = [0u8; 32];
        let mut ok = self.bus.read(ADDR, RX_BYTE_COUNT, &mut raw[..1]);
        let n = usize::from(raw[0]).min(31);
        if ok && n > 0 {
            ok = self.bus.read(ADDR, RX_BYTE_COUNT + 1, &mut raw[1..1 + n]);
        }
        self.w16(ALERT, ALERT_RX | ALERT_RX_OVERFLOW);
        // raw: count, frame type, header (2), payload
        if !ok || raw[0] < 3 {
            return None;
        }
        let header = u16::from_le_bytes([raw[2], raw[3]]);
        let mut data = [0u32; 7];
        for (i, w) in data.iter_mut().enumerate().take(crate::pd::cnt(header)) {
            *w = u32::from_le_bytes([
                raw[4 + 4 * i],
                raw[5 + 4 * i],
                raw[6 + 4 * i],
                raw[7 + 4 * i],
            ]);
        }
        Some(Rx {
            frame: raw[1],
            header,
            data,
        })
    }
}
