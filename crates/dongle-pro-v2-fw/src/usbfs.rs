//! USB device driver for the CH32X035's full-speed controller, for
//! embassy-usb.
//!
//! Adapted from ch32-hal's `otg_fs` driver (MIT OR Apache-2.0, ch32-rs
//! contributors), X035 only, with the host-side failure cases handled:
//!
//! - The controller has one "transfer done" flag for every endpoint. The
//!   upstream driver leaves it set for the endpoint's future to clear, so an
//!   event for an endpoint nobody is waiting on (a transfer the host abandoned,
//!   a future that was dropped) left it set forever and stalled every
//!   endpoint, EP0 included. Here each endpoint direction records whether a
//!   transfer is armed, and the interrupt handler answers any other event
//!   itself (NAK, clear the flag).
//! - A SETUP that arrives while a control transfer is still in its data or
//!   status stage (the host gave up on it) ends that transfer and is kept for
//!   the next `setup()`, instead of being dropped or panicking.
//! - Endpoint stall and unstall work (CLEAR_FEATURE / SET_FEATURE HALT,
//!   GET_STATUS), and nothing panics on what a host sends.

use core::cell::Cell;
use core::future::poll_fn;
use core::marker::PhantomData;
use core::task::Poll;

use ch32_hal::gpio::{Flex, Pull};
use ch32_hal::interrupt::typelevel::{Interrupt, USBFS as Irq};
use ch32_hal::pac;
use ch32_hal::pac::usb::Usbd;
use ch32_hal::pac::usb::regs::UepCtrl;
use ch32_hal::pac::usb::vals::{EpRxResponse, EpTxResponse, UsbToken};
use ch32_hal::{Peri, peripherals};
use critical_section::Mutex;
use embassy_sync::waitqueue::AtomicWaker;
use embassy_usb_driver::{
    Direction, EndpointAddress, EndpointAllocError, EndpointConfig, EndpointError, EndpointInfo,
    Event, Unsupported,
};

/// EP0 and EP1-7. EP4 shares EP0's DMA buffer on the X035 and is never
/// handed out.
const NR_EP: usize = 8;
const MAX_PACKET: u16 = 64;

static BUS_WAKER: AtomicWaker = AtomicWaker::new();
static EP_WAKERS: [AtomicWaker; NR_EP] = [const { AtomicWaker::new() }; NR_EP];

/// Endpoint directions with a transfer armed and a future waiting on it:
/// bit n of `.0` for IN n, of `.1` for OUT n. EP0 counts as both while the
/// control pipe waits on a stage.
static ARMED: Mutex<Cell<(u8, u8)>> = Mutex::new(Cell::new((0, 0)));

fn arm(ep: usize, dir: Direction, on: bool) {
    critical_section::with(|cs| {
        let cell = ARMED.borrow(cs);
        let (mut i, mut o) = cell.get();
        let bits = if dir == Direction::In { &mut i } else { &mut o };
        if on {
            *bits |= 1 << ep;
        } else {
            *bits &= !(1 << ep);
        }
        cell.set((i, o));
    });
}

fn armed(ep: usize, dir: Direction) -> bool {
    critical_section::with(|cs| {
        let (i, o) = ARMED.borrow(cs).get();
        (if dir == Direction::In { i } else { o }) & 1 << ep != 0
    })
}

fn regs() -> Usbd {
    // SAFETY: the X035's USBFS block, in its device-mode register view.
    unsafe { Usbd::from_ptr(pac::USBFS.as_ptr()) }
}

// ------------------------------------------------------------- registers

struct IntSt {
    token: UsbToken,
    ep: usize,
    tog_ok: bool,
}

fn transfer_pending() -> bool {
    regs().int_fg().read().transfer()
}
fn int_st() -> IntSt {
    let v = regs().int_st().read();
    IntSt {
        token: v.mask_token(),
        ep: usize::from(v.mask_uis_endp()),
        tog_ok: v.tog_ok(),
    }
}
fn clear_transfer() {
    regs().int_fg().write(|w| w.set_transfer(true));
}
fn rx_len() -> usize {
    usize::from(regs().rx_len().read().rx_len())
}
fn set_ep_dma(ep: usize, addr: u32) {
    match ep {
        0..=3 => regs()
            .uep0123_dma(ep)
            .write(|v| v.set_buf_addr(addr as u16)),
        5..=7 => regs()
            .uep567_dma(ep - 5)
            .write(|v| v.set_buf_addr(addr as u16)),
        _ => {}
    }
}
fn set_t_len(ep: usize, len: usize) {
    match ep {
        0..=4 => regs().uep01234_t_len(ep).write(|v| v.set_t_len(len as u8)),
        5..=7 => regs()
            .uep567_t_len(ep - 5)
            .write(|v| v.set_t_len(len as u8)),
        _ => {}
    }
}
fn ctrl(ep: usize, f: impl FnOnce(&mut UepCtrl)) {
    match ep {
        0..=4 => regs().uep01234_ctrl(ep).modify(f),
        5..=7 => regs().uep567_ctrl(ep - 5).modify(f),
        _ => {}
    }
}
fn read_ctrl(ep: usize) -> UepCtrl {
    match ep {
        0..=4 => regs().uep01234_ctrl(ep).read(),
        _ => regs().uep567_ctrl(ep - 5).read(),
    }
}
fn tx(ep: usize, res: EpTxResponse) {
    ctrl(ep, |v| v.set_t_res(res));
}
fn rx(ep: usize, res: EpRxResponse) {
    ctrl(ep, |v| v.set_r_res(res));
}
fn ep_enabled(ep: usize, dir: Direction) -> bool {
    let r = regs();
    let i = dir == Direction::In;
    match ep {
        1 => r.uep4_1_mod().read().tx_en(1) && i || r.uep4_1_mod().read().rx_en(1) && !i,
        2 => r.uep2_3_mod().read().tx_en(0) && i || r.uep2_3_mod().read().rx_en(0) && !i,
        3 => r.uep2_3_mod().read().tx_en(1) && i || r.uep2_3_mod().read().rx_en(1) && !i,
        5..=7 => {
            let m = r.uep567_mod().read();
            if i { m.tx_en(ep - 5) } else { m.rx_en(ep - 5) }
        }
        _ => false,
    }
}
fn set_ep_enabled(ep: usize, dir: Direction, on: bool) {
    let r = regs();
    let i = dir == Direction::In;
    match ep {
        4 | 1 => r.uep4_1_mod().modify(|v| {
            let n = if ep == 4 { 0 } else { 1 };
            if i {
                v.set_tx_en(n, on)
            } else {
                v.set_rx_en(n, on)
            }
        }),
        2 | 3 => r.uep2_3_mod().modify(|v| {
            if i {
                v.set_tx_en(ep - 2, on)
            } else {
                v.set_rx_en(ep - 2, on)
            }
        }),
        5..=7 => r.uep567_mod().modify(|v| {
            if i {
                v.set_tx_en(ep - 5, on)
            } else {
                v.set_rx_en(ep - 5, on)
            }
        }),
        _ => {}
    }
}

// ------------------------------------------------------------- interrupt

#[ch32_hal::interrupt]
fn USBFS() {
    let r = regs();
    let fg = r.int_fg().read();
    if fg.fifo_ov() {
        // Data was lost; the transfer it belonged to fails on its own.
        r.int_fg().write(|w| w.set_fifo_ov(true));
    }
    if fg.hst_sof() {
        r.int_fg().write(|w| w.set_hst_sof(true));
    }
    if fg.suspend() || fg.bus_rst() {
        // Left set for `Bus::poll`, which re-enables the interrupt.
        Irq::disable();
        BUS_WAKER.wake();
    }
    if !fg.transfer() {
        return;
    }
    let st = int_st();
    match st.token {
        UsbToken::SETUP => {
            Irq::disable();
            EP_WAKERS[0].wake();
        }
        UsbToken::IN | UsbToken::OUT if st.ep < NR_EP => {
            let dir = if st.token == UsbToken::IN {
                Direction::In
            } else {
                Direction::Out
            };
            if armed(st.ep, dir) {
                // Left set for the waiting future, which re-enables the interrupt.
                Irq::disable();
                EP_WAKERS[st.ep].wake();
            } else {
                // Nobody is waiting for this one: answer it here so the flag
                // doesn't block every other endpoint.
                if dir == Direction::In {
                    tx(st.ep, EpTxResponse::NAK);
                } else {
                    rx(st.ep, EpRxResponse::NAK);
                }
                clear_transfer();
            }
        }
        _ => clear_transfer(),
    }
}

/// Wait for a transfer event on `ep`. `step` sees each event for `ep` (the
/// flag still set) and returns `Some` to finish, `None` to clear it and keep
/// waiting. A SETUP ends the wait with `Err` and is left for `setup()`.
async fn wait_ep<R>(
    ep: usize,
    mut step: impl FnMut(&IntSt) -> Option<R>,
) -> Result<R, EndpointError> {
    poll_fn(|cx| {
        EP_WAKERS[ep].register(cx.waker());
        let res = if transfer_pending() {
            let st = int_st();
            if st.token == UsbToken::SETUP && ep == 0 {
                Poll::Ready(Err(EndpointError::Disabled))
            } else if st.ep == ep && (st.token == UsbToken::IN || st.token == UsbToken::OUT) {
                let done = step(&st);
                clear_transfer();
                match done {
                    Some(v) => Poll::Ready(Ok(v)),
                    None => Poll::Pending,
                }
            } else {
                Poll::Pending
            }
        } else {
            Poll::Pending
        };
        // SAFETY: the handler only touches this module's state.
        unsafe { Irq::enable() };
        res
    })
    .await
}

/// Disarms an endpoint direction and NAKs it if its future is dropped before
/// the transfer finished.
struct Armed {
    ep: usize,
    dir: Direction,
}
impl Armed {
    fn new(ep: usize, dir: Direction) -> Self {
        arm(ep, dir, true);
        Self { ep, dir }
    }
}
impl Drop for Armed {
    fn drop(&mut self) {
        if self.dir == Direction::In {
            tx(self.ep, EpTxResponse::NAK);
        } else {
            rx(self.ep, EpRxResponse::NAK);
        }
        arm(self.ep, self.dir, false);
    }
}

// ------------------------------------------------------------- buffers

/// A 64-byte endpoint buffer the controller DMAs into; must live in RAM.
#[repr(C, align(4))]
pub struct EpBuffer([u8; 64]);
impl EpBuffer {
    pub const fn new() -> Self {
        Self([0; 64])
    }
}

struct Buf(*mut u8);
impl Buf {
    fn addr(&self) -> u32 {
        self.0 as u32
    }
    fn read(&self, out: &mut [u8]) {
        for (i, b) in out.iter_mut().enumerate() {
            // SAFETY: within the 64-byte buffer, which outlives the driver.
            *b = unsafe { core::ptr::read_volatile(self.0.add(i)) };
        }
    }
    fn write(&mut self, data: &[u8]) {
        for (i, &b) in data.iter().enumerate() {
            // SAFETY: as above; `data` is at most 64 bytes.
            unsafe { core::ptr::write_volatile(self.0.add(i), b) };
        }
    }
}

// ------------------------------------------------------------- driver

pub struct Driver<'d> {
    buffers: &'d mut [EpBuffer],
    next_buffer: usize,
    /// Endpoint numbers handed out; EP0 and EP4 are never free.
    taken: u8,
}

impl<'d> Driver<'d> {
    /// `buffers` needs one entry for EP0 plus one per class endpoint.
    pub fn new(
        _usb: Peri<'d, peripherals::USBFS>,
        dp: Peri<'d, peripherals::PC17>,
        dm: Peri<'d, peripherals::PC16>,
        buffers: &'d mut [EpBuffer],
    ) -> Self {
        // The X035 keeps D+/D- as GPIO inputs and switches its own 1.5k
        // pull-up on through AFIO for a 3.3 V supply.
        Flex::new(dp).set_as_input(Pull::Up);
        Flex::new(dm).set_as_input(Pull::None);
        critical_section::with(|_| {
            pac::AFIO.ctlr().modify(|w| {
                w.set_usb_phy_v33(true);
                w.set_udp_pue(0b11);
                w.set_udm_pue(0);
                w.set_usb_ioen(true);
            });
            pac::RCC.ahbpcenr().modify(|w| w.set_usbfsen(true));
            pac::RCC.ahbrstr().modify(|w| w.set_usbfsrst(true));
            pac::RCC.ahbrstr().modify(|w| w.set_usbfsrst(false));
        });
        Self {
            buffers,
            next_buffer: 0,
            taken: 1 | 1 << 4,
        }
    }

    fn buffer(&mut self) -> Result<Buf, EndpointAllocError> {
        let b = self
            .buffers
            .get_mut(self.next_buffer)
            .ok_or(EndpointAllocError)?;
        self.next_buffer += 1;
        Ok(Buf(b.0.as_mut_ptr()))
    }

    fn alloc<D>(
        &mut self,
        config: &EndpointConfig,
        dir: Direction,
    ) -> Result<Endpoint<D>, EndpointAllocError> {
        if config.max_packet_size > MAX_PACKET {
            return Err(EndpointAllocError);
        }
        let ep = match config.addr {
            Some(a)
                if a.direction() == dir
                    && a.index() < NR_EP
                    && self.taken & 1 << a.index() == 0 =>
            {
                a.index()
            }
            Some(_) => return Err(EndpointAllocError),
            None => (1..NR_EP)
                .find(|&n| self.taken & 1 << n == 0)
                .ok_or(EndpointAllocError)?,
        };
        self.taken |= 1 << ep;
        let buf = self.buffer()?;
        set_ep_dma(ep, buf.addr());
        Ok(Endpoint {
            info: EndpointInfo {
                addr: EndpointAddress::from_parts(ep, dir),
                ep_type: config.ep_type,
                max_packet_size: config.max_packet_size,
                interval_ms: config.interval_ms,
                max_burst: 0,
            },
            buf,
            _dir: PhantomData,
        })
    }
}

pub struct In;
pub struct Out;

impl<'d> embassy_usb_driver::Driver<'d> for Driver<'d> {
    type EndpointOut = Endpoint<Out>;
    type EndpointIn = Endpoint<In>;
    type ControlPipe = ControlPipe;
    type Bus = Bus;

    fn alloc_endpoint_out(
        &mut self,
        config: &EndpointConfig,
    ) -> Result<Self::EndpointOut, EndpointAllocError> {
        self.alloc(config, Direction::Out)
    }

    fn alloc_endpoint_in(
        &mut self,
        config: &EndpointConfig,
    ) -> Result<Self::EndpointIn, EndpointAllocError> {
        self.alloc(config, Direction::In)
    }

    fn start(mut self, control_max_packet_size: u16) -> (Bus, ControlPipe) {
        assert!(control_max_packet_size <= MAX_PACKET);
        let r = regs();
        rx(0, EpRxResponse::NAK);
        r.ctrl().write(|w| {
            w.set_clr_all(true);
            w.set_reset_sie(true);
        });
        qingke::riscv::asm::delay(500); // about 10 us
        r.ctrl().write(|_| {});
        r.int_en().write(|w| {
            w.set_suspend(true);
            w.set_transfer(true);
            w.set_bus_rst(true);
        });
        r.ctrl().write(|w| {
            w.set_int_busy(true);
            w.set_dma_en(true);
            // SYS_CTRL 0b10: device controller and its pull-up on.
            w.set_sys_ctrl(0b10);
        });
        let ep0 = self.buffer().expect("an EP0 buffer");
        set_ep_dma(0, ep0.addr());
        ctrl(0, |v| {
            v.set_r_tog(false);
            v.set_r_res(EpRxResponse::ACK);
            v.set_t_tog(false);
            v.set_t_res(EpTxResponse::NAK);
        });
        critical_section::with(|_| {
            Irq::unpend();
            // SAFETY: the handler above is this driver's.
            unsafe { Irq::enable() };
        });
        (Bus { powered: false }, ControlPipe { buf: ep0 })
    }
}

// ------------------------------------------------------------- bus

pub struct Bus {
    powered: bool,
}

impl Bus {
    fn bus_reset(&mut self) {
        regs().dev_ad().write(|v| v.set_mask_usb_addr(0));
        ctrl(0, |v| {
            v.set_r_tog(false);
            v.set_r_res(EpRxResponse::ACK);
            v.set_t_tog(false);
            v.set_t_res(EpTxResponse::NAK);
        });
        for ep in 1..NR_EP {
            ctrl(ep, |v| {
                v.set_r_res(EpRxResponse::NAK);
                v.set_t_res(EpTxResponse::NAK);
            });
            // The controller enables every endpoint after a reset.
            set_ep_enabled(ep, Direction::In, false);
            set_ep_enabled(ep, Direction::Out, false);
            EP_WAKERS[ep].wake();
        }
    }
}

impl embassy_usb_driver::Bus for Bus {
    async fn enable(&mut self) {
        critical_section::with(|_| pac::AFIO.ctlr().modify(|w| w.set_udp_pue(0b11)));
        regs().ctrl().modify(|w| w.set_sys_ctrl(0b10));
        regs().udev_ctrl().write(|w| {
            w.set_pd_dis(true);
            w.set_port_en(true);
        });
        self.bus_reset();
    }

    async fn disable(&mut self) {
        critical_section::with(|_| pac::AFIO.ctlr().modify(|w| w.set_udp_pue(0)));
        regs().ctrl().modify(|w| w.set_sys_ctrl(0b00));
        self.powered = false;
    }

    async fn poll(&mut self) -> Event {
        // No VBUS sensing: the board is bus-powered by the host it talks to.
        if !self.powered {
            self.powered = true;
            return Event::PowerDetected;
        }
        poll_fn(|cx| {
            BUS_WAKER.register(cx.waker());
            let r = regs();
            let fg = r.int_fg().read();
            let res = if fg.suspend() {
                r.int_fg().write(|w| w.set_suspend(true));
                Poll::Ready(if r.mis_st().read().suspend() {
                    Event::Suspend
                } else {
                    Event::Resume
                })
            } else if fg.bus_rst() {
                self.bus_reset();
                r.int_fg().write(|w| w.set_bus_rst(true));
                Poll::Ready(Event::Reset)
            } else {
                Poll::Pending
            };
            // SAFETY: as in `wait_ep`.
            unsafe { Irq::enable() };
            res
        })
        .await
    }

    fn endpoint_set_enabled(&mut self, addr: EndpointAddress, enabled: bool) {
        let ep = addr.index();
        // A (re)configured endpoint starts at DATA0, NAKing until armed.
        ctrl(ep, |v| match addr.direction() {
            Direction::In => {
                v.set_t_tog(false);
                v.set_t_res(EpTxResponse::NAK);
            }
            Direction::Out => {
                v.set_r_tog(false);
                v.set_r_res(EpRxResponse::NAK);
            }
        });
        set_ep_enabled(ep, addr.direction(), enabled);
        EP_WAKERS[ep].wake();
    }

    fn endpoint_set_stalled(&mut self, addr: EndpointAddress, stalled: bool) {
        let ep = addr.index();
        let armed = armed(ep, addr.direction());
        ctrl(ep, |v| match addr.direction() {
            Direction::In => {
                if !stalled {
                    v.set_t_tog(false);
                }
                v.set_t_res(match (stalled, armed) {
                    (true, _) => EpTxResponse::STALL,
                    (false, true) => EpTxResponse::ACK,
                    (false, false) => EpTxResponse::NAK,
                });
            }
            Direction::Out => {
                if !stalled {
                    v.set_r_tog(false);
                }
                v.set_r_res(match (stalled, armed) {
                    (true, _) => EpRxResponse::STALL,
                    (false, true) => EpRxResponse::ACK,
                    (false, false) => EpRxResponse::NAK,
                });
            }
        });
    }

    fn endpoint_is_stalled(&mut self, addr: EndpointAddress) -> bool {
        let v = read_ctrl(addr.index());
        match addr.direction() {
            Direction::In => v.t_res() == EpTxResponse::STALL,
            Direction::Out => v.r_res() == EpRxResponse::STALL,
        }
    }

    async fn remote_wakeup(&mut self) -> Result<(), Unsupported> {
        Err(Unsupported)
    }
}

// ------------------------------------------------------------- endpoints

pub struct Endpoint<D> {
    info: EndpointInfo,
    buf: Buf,
    _dir: PhantomData<D>,
}

impl<D> Endpoint<D> {
    fn ep(&self) -> usize {
        self.info.addr.index()
    }
    fn enabled(&self) -> bool {
        ep_enabled(self.ep(), self.info.addr.direction())
    }
}

impl<D> embassy_usb_driver::Endpoint for Endpoint<D> {
    fn info(&self) -> &EndpointInfo {
        &self.info
    }

    async fn wait_enabled(&mut self) {
        poll_fn(|cx| {
            EP_WAKERS[self.ep()].register(cx.waker());
            if self.enabled() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await
    }

    fn set_stall(&mut self, _: bool) -> Result<(), Unsupported> {
        Err(Unsupported)
    }

    fn is_stalled(&self) -> Result<bool, Unsupported> {
        Err(Unsupported)
    }
}

impl embassy_usb_driver::EndpointIn for Endpoint<In> {
    async fn write(&mut self, data: &[u8]) -> Result<(), EndpointError> {
        if data.len() > usize::from(self.info.max_packet_size) {
            return Err(EndpointError::BufferOverflow);
        }
        if !self.enabled() {
            return Err(EndpointError::Disabled);
        }
        let ep = self.ep();
        self.buf.write(data);
        set_t_len(ep, data.len());
        let _armed = Armed::new(ep, Direction::In);
        tx(ep, EpTxResponse::ACK);
        wait_ep(ep, |st| {
            (st.token == UsbToken::IN).then(|| {
                ctrl(ep, |v| {
                    v.set_t_tog(!v.t_tog());
                    v.set_t_res(EpTxResponse::NAK);
                });
            })
        })
        .await
    }
}

impl embassy_usb_driver::EndpointOut for Endpoint<Out> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, EndpointError> {
        if !self.enabled() {
            return Err(EndpointError::Disabled);
        }
        let ep = self.ep();
        let _armed = Armed::new(ep, Direction::Out);
        rx(ep, EpRxResponse::ACK);
        let mut overflow = false;
        let len = wait_ep(ep, |st| {
            // A retransmission (wrong toggle) leaves the endpoint armed.
            if st.token != UsbToken::OUT || !st.tog_ok {
                return None;
            }
            let len = rx_len();
            overflow = len > buf.len();
            let n = len.min(buf.len());
            self.buf.read(&mut buf[..n]);
            ctrl(ep, |v| {
                v.set_r_tog(!v.r_tog());
                v.set_r_res(EpRxResponse::NAK);
            });
            Some(len)
        })
        .await?;
        if overflow {
            Err(EndpointError::BufferOverflow)
        } else {
            Ok(len)
        }
    }
}

// ------------------------------------------------------------- control pipe

pub struct ControlPipe {
    buf: Buf,
}

impl ControlPipe {
    /// Wait for the status-stage IN (zero-length packet) the device sends.
    async fn status_in(&mut self) -> Result<(), EndpointError> {
        set_t_len(0, 0);
        let _armed = Armed::new(0, Direction::In);
        ctrl(0, |v| {
            v.set_t_tog(true);
            v.set_t_res(EpTxResponse::ACK);
        });
        wait_ep(0, |st| {
            (st.token == UsbToken::IN).then(|| tx(0, EpTxResponse::NAK))
        })
        .await
    }
}

impl embassy_usb_driver::ControlPipe for ControlPipe {
    fn max_packet_size(&self) -> usize {
        usize::from(MAX_PACKET)
    }

    async fn setup(&mut self) -> [u8; 8] {
        poll_fn(|cx| {
            EP_WAKERS[0].register(cx.waker());
            let res = if transfer_pending() {
                let st = int_st();
                if st.token == UsbToken::SETUP {
                    // Whatever was in flight on EP0 is over.
                    ctrl(0, |v| {
                        v.set_t_res(EpTxResponse::NAK);
                        v.set_r_res(EpRxResponse::NAK);
                    });
                    let mut req = [0u8; 8];
                    self.buf.read(&mut req);
                    clear_transfer();
                    Poll::Ready(req)
                } else if st.ep == 0 {
                    // A late IN/OUT for a stage nobody waits on any more.
                    clear_transfer();
                    Poll::Pending
                } else {
                    Poll::Pending
                }
            } else {
                Poll::Pending
            };
            // SAFETY: as in `wait_ep`.
            unsafe { Irq::enable() };
            res
        })
        .await
    }

    async fn data_out(
        &mut self,
        buf: &mut [u8],
        first: bool,
        _last: bool,
    ) -> Result<usize, EndpointError> {
        if buf.len() > usize::from(MAX_PACKET) {
            return Err(EndpointError::BufferOverflow);
        }
        let _armed = Armed::new(0, Direction::Out);
        ctrl(0, |v| {
            if first {
                v.set_r_tog(true);
            } else {
                v.set_r_tog(!v.r_tog());
            }
            v.set_r_res(EpRxResponse::ACK);
        });
        let pipe = &self.buf;
        wait_ep(0, |st| {
            (st.token == UsbToken::OUT).then(|| {
                let len = rx_len().min(buf.len());
                pipe.read(&mut buf[..len]);
                rx(0, EpRxResponse::NAK);
                len
            })
        })
        .await
    }

    async fn data_in(&mut self, data: &[u8], first: bool, last: bool) -> Result<(), EndpointError> {
        if data.len() > usize::from(MAX_PACKET) {
            return Err(EndpointError::BufferOverflow);
        }
        self.buf.write(data);
        set_t_len(0, data.len());
        {
            let _armed = Armed::new(0, Direction::In);
            ctrl(0, |v| {
                if first {
                    v.set_t_tog(true);
                } else {
                    v.set_t_tog(!v.t_tog());
                }
                v.set_t_res(EpTxResponse::ACK);
            });
            wait_ep(0, |st| {
                (st.token == UsbToken::IN).then(|| tx(0, EpTxResponse::NAK))
            })
            .await?;
        }
        if last {
            // Status stage: the host's zero-length OUT.
            let _armed = Armed::new(0, Direction::Out);
            ctrl(0, |v| {
                v.set_r_tog(true);
                v.set_r_res(EpRxResponse::ACK);
            });
            wait_ep(0, |st| {
                (st.token == UsbToken::OUT).then(|| rx(0, EpRxResponse::NAK))
            })
            .await?;
        }
        Ok(())
    }

    async fn accept(&mut self) {
        // An abandoned status stage just ends here; `setup()` takes the next request.
        let _ = self.status_in().await;
    }

    async fn reject(&mut self) {
        ctrl(0, |v| {
            v.set_t_res(EpTxResponse::STALL);
            v.set_r_res(EpRxResponse::STALL);
        });
    }

    async fn accept_set_address(&mut self, addr: u8) {
        if self.status_in().await.is_ok() {
            regs().dev_ad().write(|v| v.set_mask_usb_addr(addr));
        }
    }
}
