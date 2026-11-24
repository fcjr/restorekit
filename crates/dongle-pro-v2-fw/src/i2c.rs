//! Bit-banged I2C master. The QFN-28 CH32X035 only bonds hardware I2C on the
//! USB and SDI pins, and its GPIO has no open-drain mode, so a line is "high"
//! when the pin is an input and "low" when it is an output driving 0.

use ch32_hal::gpio::{Flex, Pull, Speed};

pub struct I2c<'d> {
    scl: Flex<'d>,
    sda: Flex<'d>,
    pull: Pull,
}

// About 3 us between edges, plus what each pin direction change costs. Don't
// slow the bus down: a received message has to be read and answered inside
// tSenderResponse (24-30 ms).
fn pause() {
    for i in 0..8u32 {
        core::hint::black_box(i);
    }
}

impl<'d> I2c<'d> {
    /// `pull` is what holds a released line high: `Pull::Up` for a bus with no
    /// resistors, `Pull::None` when the bus has its own.
    pub fn new(scl: Flex<'d>, sda: Flex<'d>, pull: Pull) -> Self {
        let mut bus = Self { scl, sda, pull };
        bus.scl.set_as_input(pull);
        bus.sda.set_as_input(pull);
        bus
    }

    fn release(pin: &mut Flex<'d>, pull: Pull) {
        pin.set_as_input(pull);
    }
    fn low(pin: &mut Flex<'d>) {
        pin.set_low();
        pin.set_as_output(Speed::default());
    }
    fn scl_high(&mut self) {
        Self::release(&mut self.scl, self.pull);
        // clock stretching, and slow rise on weak pull-ups
        for _ in 0..2000 {
            if self.scl.is_high() {
                break;
            }
        }
        pause();
    }
    fn scl_low(&mut self) {
        Self::low(&mut self.scl);
        pause();
    }

    /// Both lines read high with nothing driving them.
    #[cfg_attr(not(feature = "pd"), allow(dead_code))]
    pub fn idle(&mut self) -> bool {
        Self::release(&mut self.scl, self.pull);
        Self::release(&mut self.sda, self.pull);
        pause();
        self.scl.is_high() && self.sda.is_high()
    }

    fn start(&mut self) {
        Self::release(&mut self.sda, self.pull);
        self.scl_high();
        Self::low(&mut self.sda);
        pause();
        self.scl_low();
    }
    fn stop(&mut self) {
        Self::low(&mut self.sda);
        pause();
        self.scl_high();
        Self::release(&mut self.sda, self.pull);
        pause();
    }
    fn put(&mut self, byte: u8) -> bool {
        for bit in (0..8).rev() {
            if byte >> bit & 1 != 0 {
                Self::release(&mut self.sda, self.pull);
            } else {
                Self::low(&mut self.sda);
            }
            pause();
            self.scl_high();
            self.scl_low();
        }
        Self::release(&mut self.sda, self.pull);
        pause();
        self.scl_high();
        let ack = self.sda.is_low();
        self.scl_low();
        ack
    }
    fn get(&mut self, ack: bool) -> u8 {
        Self::release(&mut self.sda, self.pull);
        let mut byte = 0;
        for _ in 0..8 {
            pause();
            self.scl_high();
            byte = byte << 1 | self.sda.is_high() as u8;
            self.scl_low();
        }
        if ack {
            Self::low(&mut self.sda);
        }
        pause();
        self.scl_high();
        self.scl_low();
        Self::release(&mut self.sda, self.pull);
        byte
    }

    /// Address-only write: true if something acknowledged.
    pub fn probe(&mut self, addr: u8) -> bool {
        self.start();
        let ack = self.put(addr << 1);
        self.stop();
        ack
    }

    pub fn write(&mut self, addr: u8, bytes: &[u8]) -> bool {
        self.start();
        let mut ok = self.put(addr << 1);
        for &b in bytes {
            if !ok {
                break;
            }
            ok = self.put(b);
        }
        self.stop();
        ok
    }

    /// Write `reg`, repeated start, read `out.len()` bytes.
    pub fn read(&mut self, addr: u8, reg: u8, out: &mut [u8]) -> bool {
        self.start();
        if !(self.put(addr << 1) && self.put(reg)) {
            self.stop();
            return false;
        }
        self.start();
        if !self.put(addr << 1 | 1) {
            self.stop();
            return false;
        }
        let last = out.len().saturating_sub(1);
        for (i, b) in out.iter_mut().enumerate() {
            *b = self.get(i != last);
        }
        self.stop();
        true
    }
}
