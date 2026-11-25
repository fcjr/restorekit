//! Product firmware for the Dongle-Pro v2: what restorekit talks to.
//!
//! It enumerates as RecoverKit's 16D0:14F0 with the iProduct `Dongle-Pro-v2`,
//! answers the vendor interface in `restorekit-dongle-proto` (dfu, reboot,
//! debugusb, serial, nop, bootsel, status, version), logs to its first CDC
//! port and bridges the target's SBU UART to its second. The PD engine is the
//! bench firmware's, without the hand controls: offer the Lite's 5 V PDO, take
//! the target's Request, and send the Apple VDMs on SOP'' Debug when asked.
//!
//! Firmware update: `restorekit dongle update` sends `bootsel`, which drops
//! the board into the CH32's ROM USB loader, and flashes it from the host.

#![no_std]
#![no_main]

#[allow(dead_code)]
#[path = "../i2c.rs"]
mod i2c;
#[allow(dead_code)]
#[path = "../pd.rs"]
mod pd;
#[path = "../rom.rs"]
mod rom;
#[allow(dead_code)]
#[path = "../tcpc.rs"]
mod tcpc;
#[path = "../uart.rs"]
mod uart;
#[path = "../usb.rs"]
mod usb;
#[path = "../usbfs.rs"]
mod usbfs;

use core::fmt::Write as _;
use core::sync::atomic::{AtomicU8, Ordering};

use ch32_hal::gpio::{Flex, Input, Level, Output, Pull};
use embassy_futures::join::join;
use embassy_time::{Duration, Instant, Timer};
use embassy_usb::Handler;
use embassy_usb::control::{InResponse, OutResponse, Recipient, Request, RequestType};
use restorekit_dongle_proto as proto;
use usb::Port;

use i2c::I2c;
use tcpc::{Tcpc, Tx};

// A Mac that has just booted can answer `dfu` with a hard reset, dropping the
// VDM. If one lands this soon after a `dfu`, send it again after a pause, as
// dongle-lite-fw does.
const DFU_HARD_RESET_WINDOW: Duration = Duration::from_millis(5000);
const DFU_RETRY_DELAY: Duration = Duration::from_secs(5);
const DFU_MAX_RETRIES: u8 = 3;

/// The target's debug UART on SBU.
const SBU_BAUD: u32 = 115_200;

// ---------------------------------------------------------------- vendor

/// No command waiting.
const CMD_NONE: u8 = 0xFF;

static STATE: AtomicU8 = AtomicU8::new(proto::PD_DISCONNECTED);
static FLAGS: AtomicU8 = AtomicU8::new(0);
static RESULT: AtomicU8 = AtomicU8::new(proto::RES_NONE);
static SEQ: AtomicU8 = AtomicU8::new(0);
static CMD: AtomicU8 = AtomicU8::new(CMD_NONE);

struct Vendor;

impl Vendor {
    fn ours(req: &Request) -> bool {
        req.request_type == RequestType::Vendor
            && req.recipient == Recipient::Interface
            && req.index & 0xff == u16::from(usb::vendor_interface())
    }
}

impl Handler for Vendor {
    fn control_out(&mut self, req: Request, _data: &[u8]) -> Option<OutResponse> {
        if !Self::ours(&req) {
            return None;
        }
        if req.request != proto::VREQ_CMD {
            return Some(OutResponse::Rejected);
        }
        let cmd = match req.value {
            proto::VCMD_NOP
            | proto::VCMD_DFU
            | proto::VCMD_REBOOT
            | proto::VCMD_SERIAL
            | proto::VCMD_DEBUGUSB
            | proto::VCMD_REPLUG
            | proto::VCMD_DFU_SEEN
            | proto::VCMD_BOOTSEL => req.value as u8,
            _ => return Some(OutResponse::Rejected),
        };
        // One command at a time, as the Lite's queue effectively is.
        if CMD.load(Ordering::Relaxed) != CMD_NONE {
            return Some(OutResponse::Rejected);
        }
        // Pending before the status stage, so the host never reads a stale result.
        RESULT.store(proto::RES_PENDING, Ordering::Relaxed);
        // No atomic read-modify-write on rv32imc; only this handler writes SEQ.
        SEQ.store(
            SEQ.load(Ordering::Relaxed).wrapping_add(1),
            Ordering::Relaxed,
        );
        CMD.store(cmd, Ordering::Relaxed);
        Some(OutResponse::Accepted)
    }

    fn control_in<'a>(&'a mut self, req: Request, buf: &'a mut [u8]) -> Option<InResponse<'a>> {
        if !Self::ours(&req) {
            return None;
        }
        match req.request {
            proto::VREQ_STATUS if buf.len() >= proto::STATUS_LEN => {
                buf[0] = proto::STATUS_VERSION;
                buf[1] = STATE.load(Ordering::Relaxed);
                buf[2] = FLAGS.load(Ordering::Relaxed);
                buf[3] = RESULT.load(Ordering::Relaxed);
                buf[4] = SEQ.load(Ordering::Relaxed);
                Some(InResponse::Accepted(&buf[..proto::STATUS_LEN]))
            }
            proto::VREQ_VERSION => {
                let v = env!("CARGO_PKG_VERSION").as_bytes();
                let n = v.len().min(buf.len());
                buf[..n].copy_from_slice(&v[..n]);
                Some(InResponse::Accepted(&buf[..n]))
            }
            _ => Some(InResponse::Rejected),
        }
    }
}

// ---------------------------------------------------------------- log

struct Line {
    bytes: [u8; 120],
    len: usize,
}
impl Line {
    const fn new() -> Self {
        Self {
            bytes: [0; 120],
            len: 0,
        }
    }
}
impl core::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.bytes.len() - self.len);
        self.bytes[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// The CDC console, log only. Output is dropped while no terminal is open,
/// and never waits long: the PD engine has deadlines.
struct Log {
    console: Port,
}
impl Log {
    fn raw(&mut self, mut bytes: &[u8]) {
        let mut spins = 0u32;
        while !bytes.is_empty() && self.console.connected() && spins < 20_000 {
            match self.console.try_write(bytes) {
                Ok(n) if n != 0 => {
                    bytes = &bytes[n..];
                    spins = 0;
                }
                _ => spins += 1,
            }
        }
    }
    fn line(&mut self, line: &Line) {
        self.raw(&line.bytes[..line.len]);
        self.raw(b"\r\n");
    }
}

macro_rules! log {
    ($log:expr, $($arg:tt)*) => {{
        let mut line = Line::new();
        let _ = write!(line, $($arg)*);
        $log.line(&line);
    }};
}

// ---------------------------------------------------------------- engine

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Detached,
    /// VBUS on, no Source_Capabilities acknowledged yet.
    Attached,
    /// Source_Capabilities acknowledged, no contract yet.
    Connected,
    Contract,
}

struct Engine<'d> {
    log: Log,
    tcpc: Tcpc<'d>,
    pd_int: Input<'d>,
    led: Output<'d>,
    vbus_en: Output<'d>,
    sbu_pu: Output<'d>,
    /// PA2, PA3 (USART2 TX, RX) and PB3, PB4 (USART3 TX, RX).
    sbu_pins: [Flex<'d>; 4],
    uart2: Option<uart::Controller>,
    uart3: Option<uart::Controller>,
    bridge: Option<uart::Uart>,
    /// After `serial`, before the target has sent anything: both USARTs
    /// receiving, neither driving its TX pad.
    listening: Option<(uart::Uart, uart::Uart)>,
    port: Port,
    /// `serial` was asked for: keep it up across target reboots.
    serial: bool,
    /// When to ask for the UART again: a rebooted target comes back with it
    /// off SBU, and renegotiates its contract on the way.
    serial_rearm_at: Option<Instant>,
    /// Further re-arms after this one, a second apart.
    serial_rearms: u8,
    /// Cycle CC before the next re-arm: a booting Mac ignores SOP'' Debug
    /// on the session it had before the reboot.
    serial_reprime: bool,
    /// GoodCRCs the last serial VDM pair got.
    serial_acked: u8,
    tcpc_ok: bool,
    state: State,
    cc2: bool,
    cap_timer: u8,
    detach_count: u8,
    dfu_sent_at: Option<Instant>,
    dfu_retry_at: Option<Instant>,
    dfu_retries: u8,
    ticks: u32,
}

impl Engine<'_> {
    fn publish(&self) {
        let state = match self.state {
            State::Detached => proto::PD_DISCONNECTED,
            State::Attached => proto::PD_VBUS_ON,
            State::Connected => proto::PD_CONNECTED,
            State::Contract => proto::PD_ACCEPT,
        };
        STATE.store(state, Ordering::Relaxed);
        let mut flags = 0;
        if self.state != State::Detached {
            flags |= proto::FLAG_TARGET_ATTACHED;
        }
        if self.cc2 {
            flags |= proto::FLAG_POLARITY_CC2;
        }
        FLAGS.store(flags, Ordering::Relaxed);
    }

    async fn tcpc_init(&mut self) {
        match self.tcpc.init().await {
            Some((vid, pid, _)) => {
                log!(self.log, "HUSB311 {:04x}:{:04x} up", vid, pid);
                self.tcpc_ok = true;
                self.tcpc.set_rp(0);
            }
            None => {
                if self.tcpc_ok {
                    log!(self.log, "HUSB311 stopped answering");
                }
                self.tcpc_ok = false;
            }
        }
    }

    async fn detach(&mut self, why: &str) {
        self.close_bridge();
        self.vbus_en.set_low();
        self.tcpc.end_session();
        self.tcpc.set_rp(0);
        self.state = State::Detached;
        self.detach_count = 0;
        log!(self.log, "target detached ({})", why);
    }

    async fn attach(&mut self, cc1: u8, cc2: u8) {
        self.cc2 = cc2 > cc1;
        self.tcpc.start_session(self.cc2);
        self.vbus_en.set_high();
        self.state = State::Attached;
        self.detach_count = 0;
        log!(
            self.log,
            "target attached on CC{}; VBUS on",
            if self.cc2 { 2 } else { 1 }
        );
        // tTypeCSendSourceCap: 100-200 ms after VBUS before the first message.
        Timer::after_millis(150).await;
        self.send_caps().await;
    }

    /// Ask the target for its debug UART on SBU and bridge it to the second
    /// CDC port. Which SBU line the target transmits on depends on the cable
    /// and the orientation, so listen on both (USART3 RX on SBU1, USART2 RX
    /// on SBU2) and only drive the other line once the target has spoken.
    async fn enter_serial(&mut self) -> u8 {
        if self.state == State::Detached {
            log!(self.log, "serial: no target");
            return proto::RES_NOTARGET;
        }
        let mut acked = 0;
        for _ in 0..2 {
            let header = pd::header(pd::DATA_VENDOR_DEF, pd::VDM_SERIAL_SBU.len() as u16);
            if self
                .tcpc
                .transmit(tcpc::SOP_DBG2, header, &pd::VDM_SERIAL_SBU)
                .await
                == Tx::Ok
            {
                acked += 1;
            }
        }
        self.close_bridge();
        self.sbu_pu.set_high();
        let (Some(u2), Some(u3)) = (self.uart2.take(), self.uart3.take()) else {
            return proto::RES_NONE;
        };
        self.listening = Some((u2.open(SBU_BAUD, false), u3.open(SBU_BAUD, false)));
        self.serial = true;
        self.serial_acked = acked;
        log!(
            self.log,
            "serial: {} of 2 got a GoodCRC; listening on both SBU lines",
            acked
        );
        proto::RES_OK
    }

    /// `serial` from the host. A Mac that has booted or rebooted since the
    /// PD session started ignores SOP'' Debug, as with `dfu`: if nothing
    /// acknowledged it, start a fresh session and ask again.
    async fn serial_command(&mut self) -> u8 {
        let res = self.enter_serial().await;
        if res != proto::RES_OK || self.serial_acked != 0 {
            return res;
        }
        if !self.fresh_session().await {
            return proto::RES_NOTARGET;
        }
        let res = self.enter_serial().await;
        if res == proto::RES_OK && self.serial_acked == 0 {
            // Still listening, in case it comes round; the host is told.
            return proto::RES_UNHEARD;
        }
        res
    }

    /// Cycle CC and wait for the target to take a contract again.
    async fn fresh_session(&mut self) -> bool {
        if !self.reprime().await {
            return false;
        }
        let end = Instant::now() + Duration::from_secs(2);
        while self.state != State::Contract && Instant::now() < end {
            Timer::after_millis(2).await;
            self.alerts().await;
        }
        true
    }

    fn close_bridge(&mut self) {
        if let Some((u2, u3)) = self.listening.take() {
            self.park(u2);
            self.park(u3);
        }
        if let Some(uart) = self.bridge.take() {
            self.park(uart);
        }
        self.sbu_pu.set_low();
    }

    /// Stop a USART and hand its pins back as inputs: each SBU net also lands
    /// on the other USART's pin, and a parked TX output would fight it.
    fn park(&mut self, uart: uart::Uart) {
        let controller = uart.close();
        let pins = if controller.index() == 2 { 0..2 } else { 2..4 };
        for pin in &mut self.sbu_pins[pins] {
            pin.set_as_input(Pull::None);
        }
        if controller.index() == 2 {
            self.uart2 = Some(controller);
        } else {
            self.uart3 = Some(controller);
        }
    }

    /// While listening: the first byte picks the line the target transmits
    /// on, and the USART receiving it takes the bridge.
    fn pick_line(&mut self) -> Option<u8> {
        let (u2, u3) = self.listening.as_mut()?;
        let (first, keep_u3) = if let Some(b) = u3.read() {
            (b, true)
        } else {
            (u2.read()?, false)
        };
        let (u2, u3) = self.listening.take()?;
        let (mut keep, drop) = if keep_u3 { (u3, u2) } else { (u2, u3) };
        self.park(drop);
        keep.drive_tx();
        let (rx, tx) = if keep_u3 { (1, 2) } else { (2, 1) };
        log!(
            self.log,
            "serial: target transmits on SBU{}; bridged, sending on SBU{}",
            rx,
            tx
        );
        self.bridge = Some(keep);
        Some(first)
    }

    /// Move bytes between the target UART and the second CDC port.
    fn bridge_poll(&mut self) {
        let mut buf = [0u8; 64];
        let mut n = 0;
        if self.listening.is_some() {
            match self.pick_line() {
                Some(b) => {
                    buf[0] = b;
                    n = 1;
                }
                None => {
                    // Nowhere to send host input until the target has spoken.
                    let _ = self.port.try_read(&mut buf);
                    return;
                }
            }
        }
        let Some(uart) = self.bridge.as_mut() else {
            return;
        };
        let port = &mut self.port;
        let mut inbound = [0u8; 64];
        if let Ok(k) = port.try_read(&mut inbound) {
            for &b in &inbound[..k] {
                for _ in 0..20_000 {
                    if uart.write(b) {
                        break;
                    }
                }
            }
        }
        while n < buf.len() {
            match uart.read() {
                Some(b) => {
                    buf[n] = b;
                    n += 1;
                }
                None => break,
            }
        }
        // Dropped while no terminal has the port open, as on a real UART.
        let mut out = &buf[..n];
        let mut spins = 0u32;
        while !out.is_empty() && port.connected() && spins < 20_000 {
            match port.try_write(out) {
                Ok(k) if k != 0 => {
                    out = &out[k..];
                    spins = 0;
                }
                _ => spins += 1,
            }
        }
    }

    /// The Lite's PDO: fixed 5 V, 450 mA, USB communications capable.
    async fn send_caps(&mut self) {
        let pdo = 1 << 26 | pd::fixed_pdo(5000, 450, false);
        let res = self
            .tcpc
            .transmit(tcpc::SOP, pd::header(pd::DATA_SOURCE_CAP, 1), &[pdo])
            .await;
        if res == Tx::Ok && self.state == State::Attached {
            self.state = State::Connected;
        }
        self.cap_timer = 0;
    }

    /// An empty VDM on SOP'' Debug, as dongle-lite-fw sends on its
    /// handshake timer.
    async fn debug_poke(&mut self) -> Tx {
        let header = pd::header(pd::DATA_VENDOR_DEF, 1);
        self.tcpc.transmit(tcpc::SOP_DBG2, header, &[0]).await
    }

    async fn ctrl(&mut self, kind: u16) -> Tx {
        self.tcpc
            .transmit(tcpc::SOP, pd::header(kind, 0), &[])
            .await
    }

    async fn on_message(&mut self, rx: tcpc::Rx) {
        if rx.frame != tcpc::SOP {
            return;
        }
        let n = pd::cnt(rx.header);
        let kind = pd::kind(rx.header);
        if n != 0 {
            match kind {
                pd::DATA_REQUEST => {
                    if self.ctrl(pd::CTRL_ACCEPT).await == Tx::Ok
                        && self.ctrl(pd::CTRL_PS_RDY).await == Tx::Ok
                    {
                        self.state = State::Contract;
                        log!(self.log, "PD contract: 5 V");
                        if self.serial {
                            self.serial_rearm_at =
                                Some(Instant::now() + Duration::from_millis(300));
                        }
                    }
                }
                pd::DATA_VENDOR_DEF if rx.data[0] == 0xff00_8001 => {
                    // Discover Identity, answered the way dongle-lite-fw does.
                    let vdm = [
                        0xff00_8001 | 1 << 6,
                        1 << 30 | 0x5ac,
                        0,
                        0x0001 << 16 | 0x100,
                    ];
                    let header = pd::DATA_VENDOR_DEF | pd::REV20 << 6 | 4 << 12;
                    self.tcpc.transmit(tcpc::SOP, header, &vdm).await;
                }
                _ => {}
            }
        } else {
            match kind {
                pd::CTRL_GET_SOURCE_CAP => self.send_caps().await,
                pd::CTRL_GET_SINK_CAP => {
                    self.tcpc
                        .transmit(tcpc::SOP, pd::header(pd::DATA_SINK_CAP, 1), &[1 << 26])
                        .await;
                }
                pd::CTRL_PR_SWAP | pd::CTRL_DR_SWAP | pd::CTRL_VCONN_SWAP => {
                    self.ctrl(pd::CTRL_REJECT).await;
                }
                pd::CTRL_SOFT_RESET => {
                    self.tcpc.reset_msg_ids();
                    self.ctrl(pd::CTRL_ACCEPT).await;
                    self.state = State::Attached;
                    self.send_caps().await;
                }
                _ => {}
            }
        }
    }

    async fn alerts(&mut self) {
        let Some(alert) = self.tcpc.r16(tcpc::ALERT) else {
            self.tcpc_ok = false;
            log!(self.log, "HUSB311 stopped answering");
            return;
        };
        if alert == 0 {
            return;
        }
        if alert & tcpc::ALERT_RX != 0 {
            if let Some(rx) = self.tcpc.receive() {
                self.on_message(rx).await;
            }
        }
        if alert & tcpc::ALERT_RX_HARD_RESET != 0 {
            self.tcpc.reset_msg_ids();
            // Tear down and attach afresh, VBUS included, as the Lite does.
            // The next tick sees Rd and starts a new session.
            if self.state != State::Detached {
                self.detach("PD hard reset from the target").await;
            } else {
                log!(self.log, "PD hard reset from the target");
            }
            if let Some(t) = self.dfu_sent_at.take() {
                if t.elapsed() < DFU_HARD_RESET_WINDOW && self.dfu_retries < DFU_MAX_RETRIES {
                    self.dfu_retries += 1;
                    self.dfu_retry_at = Some(Instant::now() + DFU_RETRY_DELAY);
                    log!(
                        self.log,
                        "dfu dropped by a hard reset; retrying in {} s",
                        DFU_RETRY_DELAY.as_secs()
                    );
                }
            }
        }
        if alert & tcpc::ALERT_FAULT != 0 {
            let fault = self.tcpc.r8(tcpc::FAULT_STATUS).unwrap_or(0);
            self.tcpc.w8(tcpc::FAULT_STATUS, fault);
        }
        // RX was released by receive(); TX bits belong to transmit().
        self.tcpc.w16(
            tcpc::ALERT,
            alert
                & !(tcpc::ALERT_RX
                    | tcpc::ALERT_TX_OK
                    | tcpc::ALERT_TX_FAIL
                    | tcpc::ALERT_TX_DISCARD),
        );
    }

    /// Wait while still answering the target, which wants its Accept within
    /// tSenderResponse.
    async fn serve_for(&mut self, ms: u32) {
        let end = Instant::now() + Duration::from_millis(ms.into());
        while Instant::now() < end {
            Timer::after_millis(2).await;
            self.alerts().await;
        }
    }

    /// Drop Rp for a second so the target starts a fresh PD session, as the
    /// Lite does before dfu and debugusb. False if the target never came back.
    async fn reprime(&mut self) -> bool {
        self.tcpc.set_open();
        Timer::after_millis(1000).await;
        self.tcpc.set_rp(0);
        for _ in 0..15 {
            Timer::after_millis(100).await;
            if let Some((cc1, cc2)) = self.tcpc.cc() {
                if cc1 == tcpc::CC_RD || cc2 == tcpc::CC_RD {
                    self.attach(cc1, cc2).await;
                    self.serve_for(200).await;
                    return true;
                }
            }
        }
        self.detach("did not come back after the CC cycle").await;
        false
    }

    /// Spray an Apple VDM over about 1.5 s on SOP'' Debug. A Mac in the DFU
    /// bootrom takes these unreliably, so repeat; each variant goes out per
    /// round and the target ignores the ones it doesn't implement. The real
    /// confirmation is the Mac re-enumerating, which the SDK watches for.
    async fn action(&mut self, name: &str, variants: &[&[u32]], reprime: bool) -> u8 {
        if self.state == State::Detached {
            log!(self.log, "{}: no target", name);
            return proto::RES_NOTARGET;
        }
        // dfu and debugusb can hit a stale session (the Mac reboots without
        // dropping CC); reboot works bare and any pre-traffic stops it.
        if reprime && !self.reprime().await {
            return proto::RES_NOTARGET;
        }
        let mut acked = 0;
        for _ in 0..12 {
            for words in variants {
                let header = pd::header(pd::DATA_VENDOR_DEF, words.len() as u16);
                if self.tcpc.transmit(tcpc::SOP_DBG2, header, words).await == Tx::Ok {
                    acked += 1;
                }
            }
            self.serve_for(120).await;
        }
        log!(
            self.log,
            "{}: sent, {} of {} got a GoodCRC",
            name,
            acked,
            12 * variants.len()
        );
        if acked == 0 {
            proto::RES_UNHEARD
        } else {
            proto::RES_OK
        }
    }

    /// Target power off and CC open for a second, then attach again: what
    /// unplugging and replugging the cable does, to recover a target whose
    /// USB stopped enumerating.
    async fn replug(&mut self) -> u8 {
        if self.state == State::Detached {
            log!(self.log, "replug: no target");
            return proto::RES_NOTARGET;
        }
        self.close_bridge();
        self.vbus_en.set_low();
        self.state = State::Detached;
        log!(self.log, "replug: target power and CC off for a second");
        if self.reprime().await {
            proto::RES_OK
        } else {
            proto::RES_NOTARGET
        }
    }

    async fn dfu(&mut self) -> u8 {
        let res = self
            .action("dfu", &[&pd::VDM_DFU_HOLD, &pd::VDM_DFU_HOLD_T2], true)
            .await;
        if res == proto::RES_OK {
            self.dfu_sent_at = Some(Instant::now());
        }
        res
    }

    async fn command(&mut self, cmd: u16) {
        // Anything else asked of the target ends a pending `dfu` resend: the
        // hard resets that follow are the Mac doing what it was told.
        if cmd != proto::VCMD_DFU && cmd != proto::VCMD_NOP {
            self.dfu_sent_at = None;
            self.dfu_retry_at = None;
        }
        let res = match cmd {
            proto::VCMD_NOP | proto::VCMD_DFU_SEEN => proto::RES_OK,
            proto::VCMD_DFU => {
                self.dfu_retries = 0;
                self.dfu().await
            }
            proto::VCMD_REBOOT => {
                let res = self.action("reboot", &[&pd::VDM_REBOOT], false).await;
                if res == proto::RES_OK && self.serial {
                    // A rebooting Mac drops its UART off SBU and stops
                    // listening on the old PD session, so start a fresh one
                    // and ask again while it comes up, as macvdmtool's
                    // `reboot serial` does.
                    self.serial_rearm_at = Some(Instant::now() + Duration::from_millis(1500));
                    self.serial_rearms = 6;
                    self.serial_reprime = true;
                }
                res
            }
            proto::VCMD_DEBUGUSB => self.action("debugusb", &[&pd::VDM_DEBUGUSB], true).await,
            proto::VCMD_SERIAL => self.serial_command().await,
            proto::VCMD_REPLUG => self.replug().await,
            proto::VCMD_BOOTSEL => {
                log!(self.log, "entering the ROM USB loader");
                self.vbus_en.set_low();
                // Let the status stage and the log line reach the host.
                Timer::after_millis(120).await;
                rom::enter_rom();
            }
            _ => proto::RES_NONE,
        };
        RESULT.store(res, Ordering::Relaxed);
    }

    /// Every 50 ms.
    async fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        match self.state {
            State::Detached if self.ticks % 10 == 0 => self.led.toggle(),
            State::Detached => {}
            _ => self.led.set_high(),
        }
        if !self.tcpc_ok {
            if self.ticks % 20 == 1 {
                self.tcpc_init().await;
            }
            return;
        }
        if self.pd_int.is_low() || self.ticks % 4 == 0 {
            self.alerts().await;
        }
        let Some((cc1, cc2)) = self.tcpc.cc() else {
            return;
        };
        let rd = cc1 == tcpc::CC_RD || cc2 == tcpc::CC_RD;
        match self.state {
            State::Detached => {
                if rd && self.ticks % 2 == 0 {
                    self.attach(cc1, cc2).await;
                }
            }
            _ => {
                if self.serial_rearm_at.is_some_and(|t| Instant::now() >= t) {
                    self.serial_rearm_at = None;
                    if core::mem::take(&mut self.serial_reprime) && !self.fresh_session().await {
                        return;
                    }
                    self.enter_serial().await;
                    // Unheard: the Mac is still resetting its port, so start
                    // over with a fresh session in a moment.
                    if self.serial_acked == 0 && self.serial_rearms > 0 {
                        self.serial_rearms -= 1;
                        self.serial_reprime = true;
                        self.serial_rearm_at = Some(Instant::now() + Duration::from_secs(2));
                    }
                }
                if self.dfu_retry_at.is_some_and(|t| Instant::now() >= t) {
                    self.dfu_retry_at = None;
                    log!(self.log, "retrying dfu");
                    self.dfu().await;
                }
                // Keep offering until the target takes a contract, and keep
                // the SOP'' Debug session warm the way the Lite does, so a
                // Mac that rebooted without dropping CC still hears us.
                self.cap_timer += 1;
                if self.cap_timer > 10 {
                    if self.state != State::Contract {
                        self.send_caps().await;
                    }
                    self.cap_timer = 0;
                    self.debug_poke().await;
                }
                if rd {
                    self.detach_count = 0;
                } else {
                    self.detach_count += 1;
                    if self.detach_count > 40 {
                        self.detach("no Rd for 2 s").await;
                    }
                }
            }
        }
    }
}

#[embassy_executor::main(entry = "ch32_hal::entry")]
async fn main(_spawner: embassy_executor::Spawner) -> ! {
    let p = ch32_hal::init(ch32_hal::Config {
        rcc: ch32_hal::rcc::Config::SYSCLK_FREQ_48MHZ_HSI,
        ..Default::default()
    });

    static mut VENDOR: Vendor = Vendor;
    let usb = usb::run(
        p.USBFS,
        p.PC17,
        p.PC16,
        usb::Options {
            vid: proto::VID,
            pid: proto::PID,
            product: proto::PRODUCT_PRO_V2,
            serial: usb::serial_number(),
            second_port: true,
            // SAFETY: the only reference to it.
            vendor: Some(unsafe { &mut *core::ptr::addr_of_mut!(VENDOR) }),
        },
    );

    // The design has 10k pull-ups, but the first board's bus floats low
    // without the MCU's own.
    let bus = I2c::new(Flex::new(p.PB9), Flex::new(p.PB10), Pull::Up);
    // SBU shifters off and the SBU pins idle until `serial`:
    // PA2, PA3 (USART2 TX, RX) and PB3, PB4 (USART3 TX, RX).
    let mut sbu_pins = [
        Flex::new(p.PA2),
        Flex::new(p.PA3),
        Flex::new(p.PB3),
        Flex::new(p.PB4),
    ];
    for pin in &mut sbu_pins {
        pin.set_as_input(Pull::None);
    }

    let mut engine = Engine {
        log: Log {
            console: Port::console(),
        },
        tcpc: Tcpc::new(bus),
        pd_int: Input::new(p.PB8, Pull::Up),
        led: Output::new(p.PA1, Level::Low, Default::default()),
        vbus_en: Output::new(p.PB7, Level::Low, Default::default()),
        sbu_pu: Output::new(p.PB6, Level::Low, Default::default()),
        sbu_pins,
        uart2: Some(uart::Controller::from(p.USART2)),
        uart3: Some(uart::Controller::from(p.USART3)),
        bridge: None,
        listening: None,
        port: Port::second(),
        serial: false,
        serial_rearm_at: None,
        serial_rearms: 0,
        serial_reprime: false,
        serial_acked: 0,
        tcpc_ok: false,
        state: State::Detached,
        cc2: false,
        cap_timer: 0,
        detach_count: 0,
        dfu_sent_at: None,
        dfu_retry_at: None,
        dfu_retries: 0,
        ticks: 0,
    };

    let run = async {
        engine.tcpc_init().await;
        let mut greeted = false;
        let mut next_tick = Instant::now();
        loop {
            let bridging = engine.bridge.is_some() || engine.listening.is_some();
            Timer::after_millis(if bridging { 1 } else { 2 }).await;
            engine.bridge_poll();
            if engine.log.console.connected() != greeted {
                greeted = !greeted;
                if greeted {
                    Timer::after_millis(200).await;
                    log!(
                        engine.log,
                        "Dongle-Pro-v2 firmware {}",
                        env!("CARGO_PKG_VERSION")
                    );
                }
            }
            let cmd = CMD.load(Ordering::Relaxed);
            if cmd != CMD_NONE {
                engine.command(u16::from(cmd)).await;
                CMD.store(CMD_NONE, Ordering::Relaxed);
            }
            // A sink wants its Accept within tSenderResponse (24-30 ms), so
            // don't leave a received message for the 50 ms tick.
            if engine.tcpc_ok && (engine.pd_int.is_low() || engine.state != State::Detached) {
                engine.alerts().await;
            }
            if Instant::now() >= next_tick {
                next_tick = Instant::now() + Duration::from_millis(50);
                engine.tick().await;
            }
            engine.publish();
        }
    };
    join(usb, run).await;
    unreachable!()
}
