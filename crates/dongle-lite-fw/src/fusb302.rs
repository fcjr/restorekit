//! Async FUSB302B driver, ported from the AsahiLinux vdmtool / Central
//! Scrutinizer C driver (originally Chromium OS / Reclaimer Labs, MIT).
//!
//! This talks to a single FUSB302B over I2C in source (DFP) mode, which is all
//! the Dongle Lite needs: it owns the target's CC line, sources vSafe5V, and
//! sends Apple vendor-defined messages.

use embassy_time::Timer;
use embedded_hal_async::i2c::I2c;

pub const FUSB302_ADDR: u8 = 0x22;

// Registers.
pub const REG_DEVICE_ID: u8 = 0x01;
pub const REG_SWITCHES0: u8 = 0x02;
pub const REG_SWITCHES1: u8 = 0x03;
pub const REG_MEASURE: u8 = 0x04;
pub const REG_CONTROL0: u8 = 0x06;
pub const REG_CONTROL1: u8 = 0x07;
pub const REG_CONTROL2: u8 = 0x08;
pub const REG_CONTROL3: u8 = 0x09;
pub const REG_MASK: u8 = 0x0A;
pub const REG_POWER: u8 = 0x0B;
pub const REG_RESET: u8 = 0x0C;
pub const REG_MASKA: u8 = 0x0E;
pub const REG_MASKB: u8 = 0x0F;
pub const REG_STATUS0: u8 = 0x40;
pub const REG_STATUS1: u8 = 0x41;
pub const REG_INTERRUPT: u8 = 0x42;
pub const REG_INTERRUPTA: u8 = 0x3E;
pub const REG_INTERRUPTB: u8 = 0x3F;
pub const REG_FIFOS: u8 = 0x43;

// SWITCHES0 bits.
pub const SW0_CC2_PU_EN: u8 = 1 << 7;
pub const SW0_CC1_PU_EN: u8 = 1 << 6;
pub const SW0_VCONN_CC2: u8 = 1 << 5;
pub const SW0_VCONN_CC1: u8 = 1 << 4;
pub const SW0_MEAS_CC2: u8 = 1 << 3;
pub const SW0_MEAS_CC1: u8 = 1 << 2;
pub const SW0_CC2_PD_EN: u8 = 1 << 1;
pub const SW0_CC1_PD_EN: u8 = 1 << 0;

// SWITCHES1 bits.
pub const SW1_POWERROLE: u8 = 1 << 7;
pub const SW1_SPECREV1: u8 = 1 << 6;
pub const SW1_SPECREV0: u8 = 1 << 5;
pub const SW1_DATAROLE: u8 = 1 << 4;
pub const SW1_AUTO_GCRC: u8 = 1 << 2;
pub const SW1_TXCC2_EN: u8 = 1 << 1;
pub const SW1_TXCC1_EN: u8 = 1 << 0;

// MEASURE.
pub const fn mdac_mv(mv: u16) -> u8 {
    ((mv / 42) & 0x3f) as u8
}

// CONTROL0 bits.
pub const C0_TX_FLUSH: u8 = 1 << 6;
pub const C0_INT_MASK: u8 = 1 << 5;
pub const C0_HOST_CUR_MASK: u8 = 3 << 2;
pub const C0_HOST_CUR_USB: u8 = 1 << 2;

// CONTROL1 bits.
pub const C1_ENSOP2DB: u8 = 1 << 6;
pub const C1_ENSOP1DB: u8 = 1 << 5;
pub const C1_RX_FLUSH: u8 = 1 << 2;

// CONTROL2 bits.
pub const C2_TOGGLE: u8 = 1 << 0;

// CONTROL3 bits.
pub const C3_SEND_HARDRESET: u8 = 1 << 6;
pub const C3_N_RETRIES_POS: u8 = 1;
pub const C3_AUTO_RETRY: u8 = 1 << 0;

// MASK bits.
pub const MASK_VBUSOK: u8 = 1 << 7;
pub const MASK_CRC_CHK: u8 = 1 << 4;
pub const MASK_ALERT: u8 = 1 << 3;
pub const MASK_COLLISION: u8 = 1 << 1;
pub const MASK_BC_LVL: u8 = 1 << 0;

// MASKA bits.
pub const MASKA_RETRYFAIL: u8 = 1 << 4;
pub const MASKA_HARDSENT: u8 = 1 << 3;
pub const MASKA_TX_SUCCESS: u8 = 1 << 2;
pub const MASKA_HARDRESET: u8 = 1 << 0;

// MASKB bits.
pub const MASKB_GCRCSENT: u8 = 1 << 0;

// POWER.
pub const POWER_PWR_ALL: u8 = 0xF;

// RESET.
pub const RESET_PD_RESET: u8 = 1 << 1;
pub const RESET_SW_RESET: u8 = 1 << 0;

// STATUS0 bits.
pub const STATUS0_VBUSOK: u8 = 1 << 7;
pub const STATUS0_COMP: u8 = 1 << 5;
pub const STATUS0_BC_LVL1: u8 = 1 << 1;
pub const STATUS0_BC_LVL0: u8 = 1 << 0;

// STATUS1 bits.
pub const STATUS1_RX_EMPTY: u8 = 1 << 5;

// INTERRUPT bits.
pub const INT_VBUSOK: u8 = 1 << 7;

// INTERRUPTA bits.
pub const INTA_RETRYFAIL: u8 = 1 << 4;
pub const INTA_HARDSENT: u8 = 1 << 3;
pub const INTA_TX_SUCCESS: u8 = 1 << 2;
pub const INTA_HARDRESET: u8 = 1 << 0;

// INTERRUPTB bits.
pub const INTB_GCRCSENT: u8 = 1 << 0;

// TX FIFO tokens.
pub const TKN_TXON: u8 = 0xA1;
pub const TKN_SYNC1: u8 = 0x12;
pub const TKN_SYNC2: u8 = 0x13;
pub const TKN_SYNC3: u8 = 0x1B;
pub const TKN_RST2: u8 = 0x16;
pub const TKN_PACKSYM: u8 = 0x80;
pub const TKN_JAMCRC: u8 = 0xFF;
pub const TKN_EOP: u8 = 0x14;
pub const TKN_TXOFF: u8 = 0xFE;

// RX FIFO SOP token mask.
pub const TKN_SOP_MASK: u8 = 0xE0;

// Type-C CC voltage status.
pub const CC_VOLT_OPEN: i8 = 0;
pub const CC_VOLT_RA: i8 = 1;
pub const CC_VOLT_RD: i8 = 2;
pub const CC_VOLT_SNK_DEF: i8 = 5;
pub const CC_VOLT_SNK_1_5: i8 = 6;
pub const CC_VOLT_SNK_3_0: i8 = 7;

// Rp source-default measurement thresholds (mV).
const PD_SRC_DEF_VNC_MV: u16 = 1600;
const PD_SRC_DEF_RD_THRESH_MV: u16 = 200;
const PD_RETRY_COUNT: u8 = 3;

// PD header helpers.
pub const PD_REV20: u16 = 1;
pub const PD_DATA_SOURCE_CAP: u16 = 1;
pub const PD_DATA_REQUEST: u16 = 2;
pub const PD_DATA_SINK_CAP: u16 = 4;
pub const PD_DATA_VENDOR_DEF: u16 = 15;
pub const PD_CTRL_GOOD_CRC: u16 = 1;
pub const PD_CTRL_ACCEPT: u16 = 3;
pub const PD_CTRL_REJECT: u16 = 4;
pub const PD_CTRL_PS_RDY: u16 = 6;
pub const PD_CTRL_GET_SINK_CAP: u16 = 8;
pub const PD_CTRL_DR_SWAP: u16 = 9;
pub const PD_CTRL_PR_SWAP: u16 = 10;

pub const fn pd_header(msg_type: u16, prole: u16, drole: u16, id: u16, cnt: u16, rev: u16) -> u16 {
    msg_type | (rev << 6) | (drole << 5) | (prole << 8) | (id << 9) | (cnt << 12)
}

pub const fn pd_header_cnt(header: u16) -> u16 {
    (header >> 12) & 7
}

pub const fn pd_header_type(header: u16) -> u16 {
    header & 0x1F
}

pub fn packet_is_good_crc(head: u16) -> bool {
    pd_header_type(head) == PD_CTRL_GOOD_CRC && pd_header_cnt(head) == 0
}

/// SOP* target for a transmitted message. Apple VDMs go out as SOP'' DEBUG.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TxSop {
    Sop,
    DebugPrimePrime,
}

/// Outcome of a transmit: did the port partner acknowledge with a GoodCRC?
#[derive(Copy, Clone, PartialEq, Eq, defmt::Format)]
pub enum TxResult {
    /// TX_SUCCESS: message sent and GoodCRC received (after auto-retry).
    Ok,
    /// RETRYFAIL: no GoodCRC after all retries — nothing is listening.
    NoAck,
    /// Neither bit set within the poll window.
    Timeout,
}

impl TxResult {
    pub fn is_ok(self) -> bool {
        matches!(self, TxResult::Ok)
    }
}

pub struct Fusb302<I2C> {
    i2c: I2C,
    cc_polarity: i8,
    vconn_enabled: bool,
    pulling_up: bool,
    msgid: u8,
    mdac_vnc: u8,
    mdac_rd: u8,
    control1: u8,
    /// Consecutive I2C errors; reset on any success. The engine watches this to
    /// recover (reset) when the FUSB302 goes unreachable, since a failed read
    /// otherwise returns 0 and masquerades as a real register value.
    i2c_fail_streak: u32,
}

impl<I2C: I2c> Fusb302<I2C> {
    pub fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            cc_polarity: -1,
            vconn_enabled: false,
            pulling_up: false,
            msgid: 0,
            mdac_vnc: mdac_mv(PD_SRC_DEF_VNC_MV),
            mdac_rd: mdac_mv(PD_SRC_DEF_RD_THRESH_MV),
            control1: 0,
            i2c_fail_streak: 0,
        }
    }

    pub fn polarity(&self) -> i8 {
        self.cc_polarity
    }

    /// Consecutive I2C failures since the last success.
    pub fn i2c_fail_streak(&self) -> u32 {
        self.i2c_fail_streak
    }

    fn note_i2c(&mut self, ok: bool) {
        if ok {
            self.i2c_fail_streak = 0;
        } else {
            self.i2c_fail_streak = self.i2c_fail_streak.saturating_add(1);
        }
    }

    async fn write(&mut self, reg: u8, val: u8) {
        let ok = self.i2c.write(FUSB302_ADDR, &[reg, val]).await.is_ok();
        self.note_i2c(ok);
    }

    async fn read(&mut self, reg: u8) -> u8 {
        let mut buf = [0u8; 1];
        let ok = self
            .i2c
            .write_read(FUSB302_ADDR, &[reg], &mut buf)
            .await
            .is_ok();
        self.note_i2c(ok);
        buf[0]
    }

    /// Burst write starting at FIFOS: `buf` already contains the register
    /// address in position 0.
    async fn write_raw(&mut self, buf: &[u8]) {
        let ok = self.i2c.write(FUSB302_ADDR, buf).await.is_ok();
        self.note_i2c(ok);
    }

    pub async fn device_id(&mut self) -> u8 {
        self.read(REG_DEVICE_ID).await
    }

    pub async fn pd_reset(&mut self) {
        self.write(REG_RESET, RESET_PD_RESET).await;
        self.msgid = 0;
    }

    pub async fn flush_rx_fifo(&mut self) {
        let v = self.control1 | C1_RX_FLUSH;
        self.write(REG_CONTROL1, v).await;
    }

    pub async fn flush_tx_fifo(&mut self) {
        let mut reg = self.read(REG_CONTROL0).await;
        reg |= C0_TX_FLUSH;
        self.write(REG_CONTROL0, reg).await;
    }

    pub async fn auto_goodcrc_enable(&mut self, enable: bool) {
        let mut reg = self.read(REG_SWITCHES1).await;
        if enable {
            reg |= SW1_AUTO_GCRC;
        } else {
            reg &= !SW1_AUTO_GCRC;
        }
        reg &= !(SW1_SPECREV0 | SW1_SPECREV1);
        self.write(REG_SWITCHES1, reg).await;
    }

    /// Select the default-USB Rp source current and matching detection
    /// thresholds. The dongle only ever advertises USB default Rp.
    pub async fn select_rp_usb(&mut self) {
        let mut reg = self.read(REG_CONTROL0).await;
        reg &= !C0_HOST_CUR_MASK;
        reg |= C0_HOST_CUR_USB;
        self.mdac_vnc = mdac_mv(PD_SRC_DEF_VNC_MV);
        self.mdac_rd = mdac_mv(PD_SRC_DEF_RD_THRESH_MV);
        self.write(REG_CONTROL0, reg).await;
    }

    pub async fn init(&mut self) {
        self.cc_polarity = -1;
        self.mdac_vnc = mdac_mv(PD_SRC_DEF_VNC_MV);
        self.mdac_rd = mdac_mv(PD_SRC_DEF_RD_THRESH_MV);

        self.write(REG_RESET, RESET_SW_RESET).await;
        let _ = self.read(REG_DEVICE_ID).await;

        let mut reg = self.read(REG_CONTROL3).await;
        reg |= C3_AUTO_RETRY;
        reg |= (PD_RETRY_COUNT & 0x3) << C3_N_RETRIES_POS;
        self.write(REG_CONTROL3, reg).await;

        // Interrupt masks: unmask the events we act on.
        let mut reg = 0xFFu8;
        reg &= !MASK_VBUSOK;
        reg &= !MASK_BC_LVL;
        reg &= !MASK_COLLISION;
        reg &= !MASK_ALERT;
        reg &= !MASK_CRC_CHK;
        self.write(REG_MASK, reg).await;

        let mut reg = 0xFFu8;
        reg &= !MASKA_RETRYFAIL;
        reg &= !MASKA_HARDSENT;
        reg &= !MASKA_TX_SUCCESS;
        reg &= !MASKA_HARDRESET;
        self.write(REG_MASKA, reg).await;

        let mut reg = 0xFFu8;
        reg &= !MASKB_GCRCSENT;
        self.write(REG_MASKB, reg).await;

        let mut reg = self.read(REG_CONTROL0).await;
        reg &= !C0_INT_MASK;
        self.write(REG_CONTROL0, reg).await;

        self.control1 = C1_RX_FLUSH | C1_ENSOP1DB | C1_ENSOP2DB;
        self.write(REG_CONTROL1, self.control1).await;

        self.auto_goodcrc_enable(false).await;

        self.write(REG_POWER, POWER_PWR_ALL).await;
    }

    fn convert_bc_lvl(&self, bc_lvl: u8) -> i8 {
        if self.pulling_up {
            if bc_lvl == 0x00 {
                CC_VOLT_RA
            } else if bc_lvl < 0x3 {
                CC_VOLT_RD
            } else {
                CC_VOLT_OPEN
            }
        } else {
            match bc_lvl {
                0x1 => CC_VOLT_SNK_DEF,
                0x2 => CC_VOLT_SNK_1_5,
                0x3 => CC_VOLT_SNK_3_0,
                _ => CC_VOLT_OPEN,
            }
        }
    }

    async fn measure_cc_pin_source(&mut self, cc_measure: u8) -> i8 {
        let switches0_reg = self.read(REG_SWITCHES0).await;
        let mut reg = switches0_reg;
        reg &= !(SW0_MEAS_CC1 | SW0_MEAS_CC2 | SW0_CC1_PU_EN | SW0_CC2_PU_EN);
        if cc_measure == SW0_MEAS_CC1 {
            reg |= SW0_CC1_PU_EN;
        } else {
            reg |= SW0_CC2_PU_EN;
        }
        reg |= cc_measure;
        self.write(REG_SWITCHES0, reg).await;

        self.write(REG_MEASURE, self.mdac_vnc).await;
        Timer::after_micros(250).await;

        let reg = self.read(REG_STATUS0).await;
        let mut cc_lvl = CC_VOLT_OPEN;

        if (reg & STATUS0_COMP) == 0 {
            self.write(REG_MEASURE, self.mdac_rd).await;
            Timer::after_micros(250).await;
            let reg = self.read(REG_STATUS0).await;
            cc_lvl = if reg & STATUS0_COMP != 0 {
                CC_VOLT_RD
            } else {
                CC_VOLT_RA
            };
        }

        self.write(REG_SWITCHES0, switches0_reg).await;
        cc_lvl
    }

    /// Source-mode CC detection. Returns (cc1, cc2) voltage status.
    pub async fn get_cc(&mut self) -> (i8, i8) {
        let mut cc1 = -1i8;
        let mut cc2 = -1i8;
        if self.vconn_enabled {
            if self.cc_polarity != 0 {
                cc2 = self.measure_cc_pin_source(SW0_MEAS_CC2).await;
            } else {
                cc1 = self.measure_cc_pin_source(SW0_MEAS_CC1).await;
            }
        } else {
            cc1 = self.measure_cc_pin_source(SW0_MEAS_CC1).await;
            cc2 = self.measure_cc_pin_source(SW0_MEAS_CC2).await;
        }
        (cc1, cc2)
    }

    /// Set the CC pull. Only Rp (source) and Open are used by the dongle.
    pub async fn set_cc_rp(&mut self) {
        let mut reg = self.read(REG_SWITCHES0).await;
        reg &= !(SW0_CC2_PU_EN
            | SW0_CC1_PU_EN
            | SW0_CC1_PD_EN
            | SW0_CC2_PD_EN
            | SW0_VCONN_CC1
            | SW0_VCONN_CC2);
        reg |= SW0_CC1_PU_EN | SW0_CC2_PU_EN;
        if self.vconn_enabled {
            reg |= if self.cc_polarity != 0 {
                SW0_VCONN_CC1
            } else {
                SW0_VCONN_CC2
            };
        }
        self.write(REG_SWITCHES0, reg).await;
        self.pulling_up = true;
    }

    pub async fn set_cc_open(&mut self) {
        let mut reg = self.read(REG_CONTROL2).await;
        reg &= !C2_TOGGLE;
        self.write(REG_CONTROL2, reg).await;

        let mut reg = self.read(REG_SWITCHES0).await;
        reg &= !SW0_CC1_PU_EN;
        reg &= !SW0_CC2_PU_EN;
        reg &= !SW0_CC1_PD_EN;
        reg &= !SW0_CC2_PD_EN;
        self.write(REG_SWITCHES0, reg).await;
        self.pulling_up = false;
    }

    pub async fn set_polarity(&mut self, polarity: i8) {
        let mut reg = self.read(REG_SWITCHES0).await;
        reg &= !SW0_VCONN_CC1;
        reg &= !SW0_VCONN_CC2;
        if self.vconn_enabled {
            if polarity != 0 {
                reg |= SW0_VCONN_CC1;
                reg &= !SW0_CC1_PU_EN;
            } else {
                reg |= SW0_VCONN_CC2;
                reg &= !SW0_CC2_PU_EN;
            }
        }
        reg &= !SW0_MEAS_CC1;
        reg &= !SW0_MEAS_CC2;
        if polarity != 0 {
            reg |= SW0_MEAS_CC2;
        } else {
            reg |= SW0_MEAS_CC1;
        }
        self.write(REG_SWITCHES0, reg).await;

        let mut reg = self.read(REG_SWITCHES1).await;
        reg &= !SW1_TXCC1_EN;
        reg &= !SW1_TXCC2_EN;
        if polarity != 0 {
            reg |= SW1_TXCC2_EN;
        } else {
            reg |= SW1_TXCC1_EN;
        }
        self.write(REG_SWITCHES1, reg).await;

        self.cc_polarity = polarity;
    }

    pub async fn set_msg_header(&mut self, power_role: bool, data_role: bool) {
        let mut reg = self.read(REG_SWITCHES1).await;
        reg &= !SW1_POWERROLE;
        reg &= !SW1_DATAROLE;
        if power_role {
            reg |= SW1_POWERROLE;
        }
        if data_role {
            reg |= SW1_DATAROLE;
        }
        self.write(REG_SWITCHES1, reg).await;
    }

    pub async fn set_vconn(&mut self, enable: bool) {
        self.vconn_enabled = enable;
        if enable {
            let pol = self.cc_polarity;
            self.set_polarity(pol).await;
        } else {
            let mut reg = self.read(REG_SWITCHES0).await;
            reg &= !SW0_VCONN_CC1;
            reg &= !SW0_VCONN_CC2;
            self.write(REG_SWITCHES0, reg).await;
        }
    }

    /// Returns false if polarity is undetermined (cannot enable Rx).
    pub async fn set_rx_enable(&mut self, enable: bool) -> bool {
        let mut reg = self.read(REG_SWITCHES0).await;
        reg &= !SW0_MEAS_CC1;
        reg &= !SW0_MEAS_CC2;

        if enable {
            match self.cc_polarity {
                0 => reg |= SW0_MEAS_CC1,
                1 => reg |= SW0_MEAS_CC2,
                _ => return false,
            }
            self.write(REG_SWITCHES0, reg).await;
            let m = self.read(REG_MASK).await;
            self.write(REG_MASK, m | MASK_BC_LVL).await;
            self.flush_rx_fifo().await;
        } else {
            self.write(REG_SWITCHES0, reg).await;
            let m = self.read(REG_MASK).await;
            self.write(REG_MASK, m & !MASK_BC_LVL).await;
        }
        self.auto_goodcrc_enable(enable).await;
        true
    }

    pub async fn rx_fifo_is_empty(&mut self) -> bool {
        let reg = self.read(REG_STATUS1).await;
        reg & STATUS1_RX_EMPTY != 0
    }

    pub async fn vbus_ok(&mut self) -> bool {
        let reg = self.read(REG_STATUS0).await;
        reg & STATUS0_VBUSOK != 0
    }

    /// Read interrupt registers (reading clears them).
    pub async fn get_irq(&mut self) -> (u8, u8, u8) {
        let i = self.read(REG_INTERRUPT).await;
        let ia = self.read(REG_INTERRUPTA).await;
        let ib = self.read(REG_INTERRUPTB).await;
        (i, ia, ib)
    }

    fn num_bytes(header: u16) -> usize {
        (pd_header_cnt(header) as usize) * 4 + 2
    }

    /// Pull one message from the Rx FIFO. Returns (header, payload_words, len)
    /// or None if the FIFO is empty / only GoodCRC.
    pub async fn get_message(&mut self, payload: &mut [u32; 16]) -> Option<(u16, usize)> {
        if self.rx_fifo_is_empty().await {
            return None;
        }

        let mut head: u16;
        let mut buf = [0u8; 32];
        let mut len: usize;

        loop {
            // Read the SOP token + 2 header bytes.
            let mut hdr3 = [0u8; 3];
            let ok = self
                .i2c
                .write_read(FUSB302_ADDR, &[REG_FIFOS], &mut hdr3)
                .await
                .is_ok();
            self.note_i2c(ok);
            head = (hdr3[1] as u16) | ((hdr3[2] as u16) << 8);

            len = Self::num_bytes(head) - 2;
            // Read payload + 4 CRC bytes.
            let total = len + 4;
            let ok = self.i2c.read(FUSB302_ADDR, &mut buf[..total]).await.is_ok();
            self.note_i2c(ok);

            if !(packet_is_good_crc(head) && !self.rx_fifo_is_empty().await) {
                break;
            }
        }

        if packet_is_good_crc(head) {
            return None;
        }

        // Unpack little-endian words.
        let words = len / 4;
        for i in 0..words {
            let b = i * 4;
            payload[i] = (buf[b] as u32)
                | ((buf[b + 1] as u32) << 8)
                | ((buf[b + 2] as u32) << 16)
                | ((buf[b + 3] as u32) << 24);
        }
        Some((head, words))
    }

    /// Transmit a PD message and wait for the port partner's GoodCRC.
    ///
    /// `data` is the payload words (may be empty). Returns whether the send was
    /// acknowledged. Only `REG_INTERRUPTA` (the TX-ack register) is polled here;
    /// `REG_INTERRUPT` (VBUSOK) and `REG_INTERRUPTB` (GCRCSENT / inbound
    /// messages) are left for the main loop's `handle_irq`, so attach and RX
    /// events are never consumed by a transmit.
    pub async fn transmit(&mut self, sop: TxSop, header: u16, data: &[u32]) -> TxResult {
        // The 40-byte FIFO staging buffer holds reg + SOP(4) + PACKSYM(1) +
        // header(2) + payload + CRC(4), so at most 7 payload words fit. PD data
        // messages are always <= 7 words; reject longer rather than overflow.
        if data.len() > 7 {
            return TxResult::Timeout;
        }
        self.flush_tx_fifo().await;

        let header = header | ((self.msgid as u16) << 9);
        self.msgid = (self.msgid + 1) & 0x7;

        let mut buf = [0u8; 40];
        let mut p = 0usize;
        buf[p] = REG_FIFOS;
        p += 1;

        match sop {
            TxSop::Sop => {
                buf[p] = TKN_SYNC1;
                buf[p + 1] = TKN_SYNC1;
                buf[p + 2] = TKN_SYNC1;
                buf[p + 3] = TKN_SYNC2;
                p += 4;
            }
            TxSop::DebugPrimePrime => {
                buf[p] = TKN_SYNC1;
                buf[p + 1] = TKN_RST2;
                buf[p + 2] = TKN_SYNC3;
                buf[p + 3] = TKN_SYNC2;
                p += 4;
            }
        }

        // Payload framing: PACKSYM | byte count (2 header + 4 per payload word),
        // matching exactly what we stage into the FIFO regardless of the
        // header's count field.
        let len = 2 + data.len() * 4;
        buf[p] = TKN_PACKSYM | ((len as u8) & 0x1F);
        p += 1;
        buf[p] = (header & 0xFF) as u8;
        buf[p + 1] = ((header >> 8) & 0xFF) as u8;
        p += 2;
        for w in data {
            buf[p] = (w & 0xFF) as u8;
            buf[p + 1] = ((w >> 8) & 0xFF) as u8;
            buf[p + 2] = ((w >> 16) & 0xFF) as u8;
            buf[p + 3] = ((w >> 24) & 0xFF) as u8;
            p += 4;
        }

        buf[p] = TKN_JAMCRC;
        buf[p + 1] = TKN_EOP;
        buf[p + 2] = TKN_TXOFF;
        buf[p + 3] = TKN_TXON;
        p += 4;

        self.write_raw(&buf[..p]).await;

        // Poll INTERRUPTA for the TX outcome. Auto-retry runs a few ~1 ms
        // attempts, so give it up to ~30 ms before declaring a timeout.
        for _ in 0..60 {
            Timer::after_micros(500).await;
            let ia = self.read(REG_INTERRUPTA).await;
            if ia & INTA_TX_SUCCESS != 0 {
                return TxResult::Ok;
            }
            if ia & INTA_RETRYFAIL != 0 {
                return TxResult::NoAck;
            }
        }
        TxResult::Timeout
    }
}
