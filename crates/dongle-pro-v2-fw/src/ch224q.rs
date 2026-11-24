//! CH224Q PD sink on the charger port (PD board only). It negotiates with the
//! charger by itself; the MCU reads what the charger offers and picks a voltage.

use crate::i2c::I2c;

pub const STATUS: u8 = 0x09;
pub const VOLTAGE: u8 = 0x0A;
pub const CURRENT: u8 = 0x50;
pub const SOURCE_CAPS: u8 = 0x60;

pub struct Ch224q<'d> {
    pub bus: I2c<'d>,
    pub addr: Option<u8>,
}

impl<'d> Ch224q<'d> {
    pub fn new(bus: I2c<'d>) -> Self {
        Self { bus, addr: None }
    }

    /// The pull-ups are inside the CH224Q, so the bus is dead until a charger
    /// powers it. Returns the address that answered.
    pub fn detect(&mut self) -> Option<u8> {
        self.addr = None;
        if !self.bus.idle() {
            return None;
        }
        self.addr = [0x22, 0x23].into_iter().find(|&a| self.bus.probe(a));
        self.addr
    }

    pub fn r8(&mut self, reg: u8) -> Option<u8> {
        let mut b = [0];
        self.bus.read(self.addr?, reg, &mut b).then_some(b[0])
    }
    pub fn w8(&mut self, reg: u8, value: u8) -> bool {
        self.addr.is_some_and(|a| self.bus.write(a, &[reg, value]))
    }

    fn code(volts: u32) -> Option<u8> {
        Some(match volts {
            5 => 0,
            9 => 1,
            12 => 2,
            15 => 3,
            20 => 4,
            _ => return None,
        })
    }
    /// Whether `volts` is one the CH224Q can ask for.
    pub fn request_code(&self, volts: u32) -> bool {
        Self::code(volts).is_some()
    }
    /// Ask the charger for 5, 9, 12, 15 or 20 V.
    pub fn request(&mut self, volts: u32) -> bool {
        Self::code(volts).is_some_and(|code| self.w8(VOLTAGE, code))
    }

    /// The charger's Source_Capabilities as the CH224Q stored it: message
    /// header, then up to seven PDOs.
    pub fn source_caps(&mut self) -> Option<(u16, [u32; 7])> {
        let mut raw = [0u8; 30];
        if !self.bus.read(self.addr?, SOURCE_CAPS, &mut raw) {
            return None;
        }
        let header = u16::from_le_bytes([raw[0], raw[1]]);
        let mut pdos = [0u32; 7];
        for (i, p) in pdos.iter_mut().enumerate().take(crate::pd::cnt(header)) {
            *p = u32::from_le_bytes([
                raw[2 + 4 * i],
                raw[3 + 4 * i],
                raw[4 + 4 * i],
                raw[5 + 4 * i],
            ]);
        }
        Some((header, pdos))
    }
}
