//! Interrupt-buffered USART2 and USART3, 8N1, chosen at run time: the SBU
//! bridge picks one by cable orientation. USART2 runs on PA2 (TX) and PA3
//! (RX), USART3 on PB3 (TX) and PB4 (RX), both at their default AFIO route.
//! Received bytes land in a 64-byte ring from the interrupt, so input survives
//! a busy PD engine.

use core::cell::RefCell;

use ch32_hal::interrupt::typelevel::{Interrupt, USART2, USART3};
use ch32_hal::pac::{self, usart::Usart};
use ch32_hal::{Peri, peripherals};
use critical_section::Mutex;

const RX_BUFFER: usize = 64;

struct Ring {
    bytes: [u8; RX_BUFFER],
    head: u8,
    len: u8,
}
static RINGS: [Mutex<RefCell<Ring>>; 2] = [const {
    Mutex::new(RefCell::new(Ring {
        bytes: [0; RX_BUFFER],
        head: 0,
        len: 0,
    }))
}; 2];

fn regs(index: u8) -> Usart {
    if index == 2 { pac::USART2 } else { pac::USART3 }
}
fn ring(index: u8) -> &'static Mutex<RefCell<Ring>> {
    &RINGS[usize::from(index - 2)]
}
fn interrupt(index: u8, enable: bool) {
    // SAFETY: the handlers below only touch this module's ring.
    unsafe {
        match (index, enable) {
            (2, true) => {
                USART2::unpend();
                USART2::enable()
            }
            (2, false) => USART2::disable(),
            (_, true) => {
                USART3::unpend();
                USART3::enable()
            }
            (_, false) => USART3::disable(),
        }
    }
}
/// USART2 and USART3 are APB1 bits 17 and 18, in the enable and reset registers.
fn clock(index: u8, on: bool) {
    let bit = 1u32 << (15 + u32::from(index));
    let set = |bits: u32, value: bool| if value { bits | bit } else { bits & !bit };
    let rcc = pac::RCC;
    rcc.apb1prstr().modify(|w| w.0 = set(w.0, true));
    rcc.apb1prstr().modify(|w| w.0 = set(w.0, false));
    rcc.apb1pcenr().modify(|w| w.0 = set(w.0, on));
}

/// Rewrite one pad's 4-bit CFG field: `0b1011` alternate-function push-pull,
/// `0b1000` input with the pull OUTDR selects.
fn pad(port: usize, pin: u32, cfg: u32, pull_up: bool) {
    let gpio = pac::GPIO(port);
    let shift = (pin % 8) * 4;
    critical_section::with(|_| {
        let set = |bits: u32| bits & !(0xf << shift) | cfg << shift;
        match pin / 8 {
            0 => gpio.cfglr().modify(|w| w.0 = set(w.0)),
            1 => gpio.cfghr().modify(|w| w.0 = set(w.0)),
            _ => gpio.cfgxr().modify(|w| w.0 = set(w.0)),
        }
        if pull_up {
            gpio.outdr().modify(|w| w.0 |= 1 << pin);
        }
    });
}

/// A stopped USART.
pub struct Controller(u8);
impl From<Peri<'static, peripherals::USART2>> for Controller {
    fn from(_: Peri<'static, peripherals::USART2>) -> Self {
        Self(2)
    }
}
impl From<Peri<'static, peripherals::USART3>> for Controller {
    fn from(_: Peri<'static, peripherals::USART3>) -> Self {
        Self(3)
    }
}
impl Controller {
    pub fn index(&self) -> u8 {
        self.0
    }
    /// Start 8N1 at `baud` on the controller's pads. The caller owns them.
    /// With `drive_tx` false only RX is connected and the TX pad is left as
    /// it is, so nothing drives the line until [`Uart::drive_tx`].
    pub fn open(self, baud: u32, drive_tx: bool) -> Uart {
        let index = self.0;
        let divider = (ch32_hal::rcc::clocks().hclk.0 + baud / 2) / baud;
        critical_section::with(|cs| {
            let mut ring = ring(index).borrow_ref_mut(cs);
            (ring.head, ring.len) = (0, 0);
            clock(index, true);
        });
        let (port, _, rx) = pads(index);
        pad(port, rx, 0b1000, true);
        let r = regs(index);
        r.brr().write(|w| w.0 = divider);
        r.ctlr1().write(|w| {
            w.set_ue(true);
            w.set_te(true);
            w.set_re(true);
            w.set_rxneie(true);
        });
        interrupt(index, true);
        let mut uart = Uart(index);
        if drive_tx {
            uart.drive_tx();
        }
        uart
    }
}

/// (port, TX pin, RX pin): PA2/PA3 for USART2, PB3/PB4 for USART3.
fn pads(index: u8) -> (usize, u32, u32) {
    if index == 2 { (0, 2, 3) } else { (1, 3, 4) }
}

/// A running USART.
pub struct Uart(u8);
impl Uart {
    /// Connect the TX pad.
    pub fn drive_tx(&mut self) {
        let (port, tx, _) = pads(self.0);
        pad(port, tx, 0b1011, false);
    }
    /// The next received byte, if any.
    pub fn read(&mut self) -> Option<u8> {
        critical_section::with(|cs| {
            let mut ring = ring(self.0).borrow_ref_mut(cs);
            if ring.len == 0 {
                return None;
            }
            let byte = ring.bytes[usize::from(ring.head)];
            ring.head = (ring.head + 1) % RX_BUFFER as u8;
            ring.len -= 1;
            Some(byte)
        })
    }
    /// Send one byte if the transmit register is free.
    pub fn write(&mut self, byte: u8) -> bool {
        let r = regs(self.0);
        if !r.statr().read().txe() {
            return false;
        }
        r.datar().write(|w| w.set_dr(byte.into()));
        true
    }
    /// Stop it, discarding buffered input. The pads stay as they are.
    pub fn close(self) -> Controller {
        interrupt(self.0, false);
        regs(self.0).ctlr1().write(|w| w.0 = 0);
        clock(self.0, false);
        Controller(self.0)
    }
}

fn on_irq(index: u8) {
    let r = regs(index);
    critical_section::with(|cs| {
        let mut ring = ring(index).borrow_ref_mut(cs);
        loop {
            let status = r.statr().read();
            if !status.rxne() && !status.ore() {
                break;
            }
            // Reading DATAR after STATR clears RXNE and the error flags.
            let byte = r.datar().read().dr() as u8;
            if !status.rxne() || status.fe() || status.ne() || status.pe() {
                continue;
            }
            if usize::from(ring.len) < RX_BUFFER {
                let at = (usize::from(ring.head) + usize::from(ring.len)) % RX_BUFFER;
                ring.bytes[at] = byte;
                ring.len += 1;
            }
        }
    });
}

#[ch32_hal::interrupt]
fn USART2() {
    on_irq(2)
}
#[ch32_hal::interrupt]
fn USART3() {
    on_irq(3)
}
