//! Bench firmware for the Dongle-Pro v2 and, with the `pd` feature, the
//! Dongle-Pro v2 PD. Not product firmware: it enumerates as a USB serial
//! console (through the CH334P, which is the first thing to check) and lets
//! you drive every part of the board by hand. See the README for the commands
//! and which check-list item each one answers.

#![no_std]
#![no_main]

#[cfg(feature = "pd")]
mod ch224q;
mod i2c;
mod pd;
mod rom;
mod tcpc;
mod uart;
mod usb;

use core::fmt::Write as _;

use ch32_hal::gpio::{Flex, Input, Level, Output, Pull};
use embassy_futures::join::join;
use embassy_time::{Duration, Instant, Timer};
use usb::Port;

use i2c::I2c;
use tcpc::{Tcpc, Tx};

#[cfg(not(feature = "pd"))]
const BOARD: &str = "Dongle-Pro v2";
#[cfg(feature = "pd")]
const BOARD: &str = "Dongle-Pro v2 PD";

/// What a 5 V fixed PDO advertises on host power: under the MT9700's 0.68 A
/// on the v2, under the TPS2121's 1.5 A host limit on the PD board.
#[cfg(not(feature = "pd"))]
const HOST_MA: u32 = 500;
#[cfg(feature = "pd")]
const HOST_MA: u32 = 900;

/// Leaves the SBU serial bridge.
const CTRL_X: u8 = 0x18;

// A Mac that has just booted can answer `dfu` with a hard reset, dropping the
// VDM. If one lands this soon after a `dfu`, send it again after a pause, as
// dongle-lite-fw does.
const DFU_HARD_RESET_WINDOW: Duration = Duration::from_millis(5000);
const DFU_RETRY_DELAY: Duration = Duration::from_secs(5);
const DFU_MAX_RETRIES: u8 = 3;

struct Line {
    bytes: [u8; 160],
    len: usize,
}
impl Line {
    const fn new() -> Self {
        Self {
            bytes: [0; 160],
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

struct Out {
    console: Port,
}
impl Out {
    /// The USB stack runs in its interrupt, so waiting here is a plain spin.
    /// Output is dropped when no terminal is open or the host stops reading.
    fn raw(&mut self, mut bytes: &[u8]) {
        let mut spins = 0u32;
        while !bytes.is_empty() && self.console.connected() && spins < 400_000 {
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

macro_rules! say {
    ($out:expr, $($arg:tt)*) => {{
        let mut line = Line::new();
        let _ = write!(line, $($arg)*);
        $out.line(&line);
    }};
}

#[cfg(not(feature = "pd"))]
struct Power<'d> {
    vbus_en: Output<'d>,
}
#[cfg(not(feature = "pd"))]
impl Power<'_> {
    fn on(&mut self) {
        self.vbus_en.set_high();
    }
    fn off(&mut self) {
        self.vbus_en.set_low();
    }
}

#[cfg(feature = "pd")]
struct Power<'d> {
    host_en_n: Flex<'d>,
    host: bool,
    chg_en: Output<'d>,
    chg_hv: Output<'d>,
}
#[cfg(feature = "pd")]
impl Power<'_> {
    /// HOST_EN_N is driven low to pass host 5 V and released to block it.
    fn host(&mut self, on: bool) {
        if on {
            self.host_en_n.set_low();
            self.host_en_n.set_as_output(Default::default());
        } else {
            self.host_en_n.set_as_input(Pull::None);
        }
        self.host = on;
    }
    fn on(&mut self) {
        self.host(true);
    }
    fn off(&mut self) {
        self.chg_hv.set_low();
        self.chg_en.set_low();
        self.host(false);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Detached,
    Attached,
    Contract,
}
impl State {
    fn name(self) -> &'static str {
        match self {
            State::Detached => "detached",
            State::Attached => "attached",
            State::Contract => "contract",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Caps {
    /// The PDO dongle-lite-fw sends: fixed 5 V at 450 mA, USB communications
    /// capable. 40 restores in a row through the Lite with it.
    Lite,
    /// A real 5 V fixed PDO at the host limit.
    FiveVolt,
    /// The charger's fixed PDOs, capped at 3 A.
    #[cfg(feature = "pd")]
    Charger,
}

struct Bench<'d> {
    out: Out,
    tcpc: Tcpc<'d>,
    pd_int: Input<'d>,
    led: Output<'d>,
    sbu_pu: Output<'d>,
    power: Power<'d>,
    #[cfg(feature = "pd")]
    chg: ch224q::Ch224q<'d>,
    #[cfg(feature = "pd")]
    offered: [(u32, u32); 7],
    #[cfg(feature = "pd")]
    offered_n: usize,
    /// PA2, PA3 (USART2 TX, RX) and PB3, PB4 (USART3 TX, RX).
    sbu_pins: [Flex<'d>; 4],
    uart2: Option<uart::Controller>,
    uart3: Option<uart::Controller>,
    bridge: Option<uart::Uart>,
    tcpc_ok: bool,
    auto: bool,
    state: State,
    cc2: bool,
    rp: u8,
    caps: Caps,
    cap_timer: u8,
    detach_count: u8,
    dfu_sent_at: Option<Instant>,
    dfu_retry_at: Option<Instant>,
    dfu_retries: u8,
    ticks: u32,
    line: [u8; 80],
    line_len: usize,
}

fn word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(' ') {
        Some(i) => (&s[..i], s[i..].trim_start()),
        None => (s, ""),
    }
}
fn hex(s: &str) -> Option<u32> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u32::from_str_radix(s, 16).ok()
}
fn on_off(s: &str) -> Option<bool> {
    match s {
        "on" | "1" => Some(true),
        "off" | "0" => Some(false),
        _ => None,
    }
}

impl Bench<'_> {
    async fn banner(&mut self) {
        say!(self.out, "");
        say!(
            self.out,
            "{} bench firmware {}. `help` lists the commands.",
            BOARD,
            env!("CARGO_PKG_VERSION")
        );
    }

    // ------------------------------------------------------------ PD engine

    async fn tcpc_init(&mut self) {
        match self.tcpc.init().await {
            Some((vid, pid, did)) => {
                say!(
                    self.out,
                    "HUSB311 found: vid {:04x} pid {:04x} dev {:04x}",
                    vid,
                    pid,
                    did
                );
                self.tcpc_ok = true;
                if self.auto {
                    self.tcpc.set_rp(self.rp);
                }
            }
            None => {
                if self.tcpc_ok || self.ticks < 40 {
                    say!(
                        self.out,
                        "HUSB311 not answering at 0x{:02x}; retrying every second",
                        tcpc::ADDR
                    );
                }
                self.tcpc_ok = false;
            }
        }
    }

    async fn detach(&mut self, why: &str) {
        self.power.off();
        // Take the charger back to 5 V and stop offering its PDOs: after a
        // re-attach the target is on host power again.
        #[cfg(feature = "pd")]
        {
            self.chg.request(5);
            if self.caps == Caps::Charger {
                self.caps = Caps::Lite;
            }
        }
        self.leave_bridge().await;
        self.tcpc.end_session();
        self.tcpc.set_rp(self.rp);
        self.state = State::Detached;
        self.detach_count = 0;
        say!(self.out, "detached ({}); target power off", why);
    }

    async fn attach(&mut self, cc1: u8, cc2: u8) {
        self.cc2 = !(cc1 > cc2);
        self.tcpc.start_session(self.cc2);
        self.power.on();
        self.state = State::Attached;
        self.detach_count = 0;
        say!(
            self.out,
            "attached: cc1={} cc2={}, {} cable, 5 V on",
            cc1,
            cc2,
            if self.cc2 {
                "flipped (CC2)"
            } else {
                "normal (CC1)"
            }
        );
        // tTypeCSendSourceCap: 100-200 ms after VBUS before the first message.
        Timer::after_millis(150).await;
        self.poke().await;
        self.send_caps().await;
    }

    /// An empty Apple VDM on SOP'' Debug. A GoodCRC back means the HUSB311
    /// really sends that frame type and the Mac's debug path answers it.
    async fn poke(&mut self) -> Tx {
        let res = self
            .tcpc
            .transmit(tcpc::SOP_DBG2, pd::header(pd::DATA_VENDOR_DEF, 1), &[0])
            .await;
        say!(self.out, ">SOP'' Debug poke: {}", res.name());
        res
    }

    async fn send_caps(&mut self) -> Tx {
        let mut pdos = [0u32; 7];
        let n = match self.caps {
            Caps::Lite => {
                pdos[0] = 1 << 26 | pd::fixed_pdo(5000, 450, false);
                1
            }
            Caps::FiveVolt => {
                pdos[0] = pd::fixed_pdo(5000, HOST_MA, true);
                1
            }
            #[cfg(feature = "pd")]
            Caps::Charger => {
                for (i, &(mv, ma)) in self.offered[..self.offered_n].iter().enumerate() {
                    pdos[i] = pd::fixed_pdo(mv, ma, i == 0);
                }
                self.offered_n
            }
        };
        let res = self
            .tcpc
            .transmit(
                tcpc::SOP,
                pd::header(pd::DATA_SOURCE_CAP, n as u16),
                &pdos[..n],
            )
            .await;
        say!(self.out, ">Source_Capabilities ({} PDO): {}", n, res.name());
        self.cap_timer = 0;
        res
    }

    async fn ctrl(&mut self, kind: u16, name: &str) -> Tx {
        let res = self
            .tcpc
            .transmit(tcpc::SOP, pd::header(kind, 0), &[])
            .await;
        say!(self.out, ">{}: {}", name, res.name());
        res
    }

    async fn on_request(&mut self, rdo: u32) {
        let position = (rdo >> 28 & 7) as usize;
        say!(
            self.out,
            "<Request: PDO {}, {} mA operating, {} mA max",
            position,
            (rdo >> 10 & 0x3FF) * 10,
            (rdo & 0x3FF) * 10
        );
        #[cfg(feature = "pd")]
        if self.caps == Caps::Charger {
            if position == 0 || position > self.offered_n {
                self.ctrl(pd::CTRL_REJECT, "Reject").await;
                return;
            }
            if self.ctrl(pd::CTRL_ACCEPT, "Accept").await != Tx::Ok {
                return;
            }
            let mv = self.offered[position - 1].0;
            // CHG_HV has to be up before the charger leaves 5 V, or the mux
            // cuts the charger off at 6.4 V; and down only once it is back.
            if mv > 5000 {
                self.power.chg_hv.set_high();
            }
            let ok = self.chg.request(mv / 1000);
            Timer::after_millis(400).await;
            if mv <= 5000 {
                self.power.chg_hv.set_low();
            }
            say!(
                self.out,
                "charger asked for {} V: {}",
                mv / 1000,
                if ok {
                    "written"
                } else {
                    "CH224Q did not answer"
                }
            );
            if self.ctrl(pd::CTRL_PS_RDY, "PS_RDY").await == Tx::Ok {
                self.state = State::Contract;
            }
            return;
        }
        if self.ctrl(pd::CTRL_ACCEPT, "Accept").await == Tx::Ok
            && self.ctrl(pd::CTRL_PS_RDY, "PS_RDY").await == Tx::Ok
        {
            self.state = State::Contract;
        }
    }

    async fn on_message(&mut self, rx: tcpc::Rx) {
        let n = pd::cnt(rx.header);
        let kind = pd::kind(rx.header);
        let mut line = Line::new();
        let _ = write!(
            line,
            "<rx frame {} header {:04x} type {} words {}:",
            rx.frame, rx.header, kind, n
        );
        for w in &rx.data[..n] {
            let _ = write!(line, " {:08x}", w);
        }
        self.out.line(&line);
        if rx.frame != tcpc::SOP {
            return;
        }
        if n != 0 {
            match kind {
                pd::DATA_REQUEST => self.on_request(rx.data[0]).await,
                pd::DATA_VENDOR_DEF if rx.data[0] == 0xff00_8001 => {
                    // Discover Identity, answered the way dongle-lite-fw does.
                    let vdm = [
                        0xff00_8001 | 1 << 6,
                        1 << 30 | 0x5ac,
                        0,
                        0x0001 << 16 | 0x100,
                    ];
                    let header = pd::DATA_VENDOR_DEF | pd::REV20 << 6 | 4 << 12;
                    let res = self.tcpc.transmit(tcpc::SOP, header, &vdm).await;
                    say!(self.out, ">Discover Identity ack: {}", res.name());
                }
                _ => {}
            }
        } else {
            match kind {
                pd::CTRL_GET_SOURCE_CAP => {
                    self.send_caps().await;
                }
                pd::CTRL_GET_SINK_CAP => {
                    let res = self
                        .tcpc
                        .transmit(tcpc::SOP, pd::header(pd::DATA_SINK_CAP, 1), &[1 << 26])
                        .await;
                    say!(self.out, ">Sink_Capabilities: {}", res.name());
                }
                pd::CTRL_PR_SWAP | pd::CTRL_DR_SWAP | pd::CTRL_VCONN_SWAP => {
                    self.ctrl(pd::CTRL_REJECT, "Reject").await;
                }
                pd::CTRL_SOFT_RESET => {
                    self.tcpc.reset_msg_ids();
                    self.ctrl(pd::CTRL_ACCEPT, "Accept").await;
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
            say!(self.out, "HUSB311 stopped answering");
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
            say!(self.out, "<hard reset");
            self.tcpc.reset_msg_ids();
            if self.state != State::Detached {
                // TCPCI clears RECEIVE_DETECT on a received hard reset.
                self.tcpc.start_session(self.cc2);
                self.state = State::Attached;
            }
            if let Some(t) = self.dfu_sent_at.take() {
                if t.elapsed() < DFU_HARD_RESET_WINDOW && self.dfu_retries < DFU_MAX_RETRIES {
                    self.dfu_retries += 1;
                    self.dfu_retry_at = Some(Instant::now() + DFU_RETRY_DELAY);
                    say!(
                        self.out,
                        "dfu dropped by the hard reset; retry {} in {} s",
                        self.dfu_retries,
                        DFU_RETRY_DELAY.as_secs()
                    );
                }
            }
        }
        if alert & tcpc::ALERT_FAULT != 0 {
            let fault = self.tcpc.r8(tcpc::FAULT_STATUS).unwrap_or(0);
            self.tcpc.w8(tcpc::FAULT_STATUS, fault);
            say!(self.out, "HUSB311 fault status {:02x}", fault);
        }
        if alert & tcpc::ALERT_POWER != 0 {
            let present = self.tcpc.vbus_present().unwrap_or(false);
            say!(
                self.out,
                "HUSB311 sees target VBUS {}",
                if present { "present" } else { "gone" }
            );
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

    /// Every 50 ms.
    async fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        match self.state {
            State::Detached if self.ticks % 10 == 0 => self.led.toggle(),
            State::Detached => {}
            _ => self.led.set_high(),
        }
        #[cfg(feature = "pd")]
        if self.ticks % 20 == 0 {
            self.charger_poll().await;
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
        if !self.auto || !self.tcpc_ok {
            return;
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
            State::Attached | State::Contract => {
                if self.dfu_retry_at.is_some_and(|t| Instant::now() >= t) {
                    self.dfu_retry_at = None;
                    say!(self.out, "retrying dfu");
                    self.dfu().await;
                }
                // Keep offering until the target has taken a contract.
                if self.state == State::Attached {
                    self.cap_timer += 1;
                    if self.cap_timer > 10 {
                        self.send_caps().await;
                    }
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

    /// Drop Rp for a second so the target starts a fresh session, as the Lite
    /// does before every VDM action.
    /// Wait while still answering the target, which wants its Accept within
    /// tSenderResponse.
    async fn serve_for(&mut self, ms: u32) {
        let end = Instant::now() + Duration::from_millis(ms.into());
        while Instant::now() < end {
            Timer::after_millis(2).await;
            self.alerts().await;
        }
    }

    async fn reprime(&mut self) {
        self.tcpc.set_open();
        Timer::after_millis(1000).await;
        self.tcpc.set_rp(self.rp);
        for _ in 0..15 {
            Timer::after_millis(100).await;
            if let Some((cc1, cc2)) = self.tcpc.cc() {
                if cc1 == tcpc::CC_RD || cc2 == tcpc::CC_RD {
                    self.attach(cc1, cc2).await;
                    self.serve_for(200).await;
                    return;
                }
            }
        }
        say!(self.out, "target did not come back after the CC cycle");
    }

    async fn dfu(&mut self) {
        self.action("dfu", &[&pd::VDM_DFU_HOLD, &pd::VDM_DFU_HOLD_T2])
            .await;
        if self.state != State::Detached {
            self.dfu_sent_at = Some(Instant::now());
        }
    }

    /// Cut target power and open CC for about a second, then attach again.
    /// Recovers a booted Mac that stopped enumerating without anyone pulling
    /// the cable. It doesn't ask the target to reboot.
    async fn replug(&mut self) {
        if self.state == State::Detached {
            say!(self.out, "err replug: no target");
            return;
        }
        self.detach("replug").await;
        self.reprime().await;
        say!(self.out, "ok replug");
    }

    async fn action(&mut self, name: &str, variants: &[&[u32]]) {
        if self.state == State::Detached {
            say!(self.out, "err {}: no target", name);
            return;
        }
        self.reprime().await;
        let (mut sent, mut acked) = (0, 0);
        for _ in 0..12 {
            for words in variants {
                let header = pd::header(pd::DATA_VENDOR_DEF, words.len() as u16);
                sent += 1;
                if self.tcpc.transmit(tcpc::SOP_DBG2, header, words).await == Tx::Ok {
                    acked += 1;
                }
            }
            self.serve_for(120).await;
        }
        say!(
            self.out,
            "ok {}: {} of {} VDMs got a GoodCRC",
            name,
            acked,
            sent
        );
    }

    // ------------------------------------------------------------ SBU serial

    async fn enter_bridge(&mut self, flipped: bool) {
        self.leave_bridge().await;
        self.sbu_pu.set_high();
        // Normal cable: USART2, TX on SBU1 (PA2), RX on SBU2 (PA3).
        // Flipped: USART3, TX on SBU2 (PB3), RX on SBU1 (PB4).
        let slot = if flipped {
            &mut self.uart3
        } else {
            &mut self.uart2
        };
        let Some(controller) = slot.take() else {
            return;
        };
        self.bridge = Some(controller.open(115_200));
        say!(
            self.out,
            "SBU serial on USART{} at 115200 ({} cable). Ctrl-X leaves.",
            if flipped { 3 } else { 2 },
            if flipped { "flipped" } else { "normal" }
        );
    }

    async fn leave_bridge(&mut self) {
        let Some(uart) = self.bridge.take() else {
            return;
        };
        let controller = uart.close();
        // Hand the pins back as inputs: each SBU net also lands on the other
        // USART's pin, and a parked TX output would fight it.
        let pins = if controller.index() == 2 { 0..2 } else { 2..4 };
        for pin in &mut self.sbu_pins[pins] {
            pin.set_as_input(Pull::None);
        }
        if controller.index() == 2 {
            self.uart2 = Some(controller);
        } else {
            self.uart3 = Some(controller);
        }
        self.sbu_pu.set_low();
        say!(self.out, "");
        say!(self.out, "left the SBU bridge; level shifters off");
    }

    async fn bridge_poll(&mut self) {
        let mut buf = [0u8; 64];
        if let Ok(n) = self.out.console.try_read(&mut buf) {
            for &b in &buf[..n] {
                if b == CTRL_X {
                    self.leave_bridge().await;
                    return;
                }
                let Some(uart) = self.bridge.as_mut() else {
                    return;
                };
                for _ in 0..20_000 {
                    if uart.write(b) {
                        break;
                    }
                }
            }
        }
        let Some(uart) = self.bridge.as_mut() else {
            return;
        };
        let mut n = 0;
        while n < buf.len() {
            match uart.read() {
                Some(b) => {
                    buf[n] = b;
                    n += 1;
                }
                None => break,
            }
        }
        if n != 0 {
            self.out.raw(&buf[..n]);
        }
    }

    // ------------------------------------------------------------ charger

    #[cfg(feature = "pd")]
    async fn charger_poll(&mut self) {
        let was = self.chg.addr;
        let now = self.chg.detect();
        if was == now {
            return;
        }
        match now {
            Some(addr) => {
                // It asks for 9 V by itself at plug-in. Bring it back to 5 V
                // before anything else.
                let ok = self.chg.request(5);
                say!(
                    self.out,
                    "charger: CH224Q at 0x{:02x}, set to 5 V ({})",
                    addr,
                    if ok { "ok" } else { "write failed" }
                );
            }
            None => {
                self.power.chg_hv.set_low();
                self.power.chg_en.set_low();
                say!(self.out, "charger gone; CHG_EN and CHG_HV off");
                if self.caps == Caps::Charger {
                    self.caps = Caps::Lite;
                    self.offered_n = 0;
                }
            }
        }
    }

    #[cfg(feature = "pd")]
    async fn charger_status(&mut self) {
        let Some(addr) = self.chg.addr else {
            say!(self.out, "no charger: the CH224Q bus is dead");
            return;
        };
        let status = self.chg.r8(ch224q::STATUS);
        let voltage = self.chg.r8(ch224q::VOLTAGE);
        let current = self.chg.r8(ch224q::CURRENT);
        say!(
            self.out,
            "CH224Q at 0x{:02x}: status {:02x?} voltage reg {:02x?} current reg {:02x?}",
            addr,
            status,
            voltage,
            current
        );
        match self.chg.source_caps() {
            Some((header, pdos)) => {
                say!(
                    self.out,
                    "charger Source_Capabilities header {:04x}:",
                    header
                );
                for (i, &pdo) in pdos[..pd::cnt(header)].iter().enumerate() {
                    match pd::fixed(pdo) {
                        Some((mv, ma)) => say!(
                            self.out,
                            "  {}: {:08x} fixed {} mV {} mA",
                            i + 1,
                            pdo,
                            mv,
                            ma
                        ),
                        None => say!(self.out, "  {}: {:08x} not fixed", i + 1, pdo),
                    }
                }
            }
            None => say!(self.out, "could not read the charger's capabilities"),
        }
    }

    /// Offer the target the charger's fixed PDOs up to 20 V, capped at 3 A.
    #[cfg(feature = "pd")]
    async fn offer_charger(&mut self) {
        let Some((header, pdos)) = self.chg.source_caps() else {
            say!(self.out, "err: no charger capabilities to offer");
            return;
        };
        self.offered_n = 0;
        for &pdo in &pdos[..pd::cnt(header)] {
            if let Some((mv, ma)) = pd::fixed(pdo) {
                if (5000..=20_000).contains(&mv)
                    && mv % 1000 == 0
                    && self.chg.request_code(mv / 1000)
                {
                    self.offered[self.offered_n] = (mv, ma.min(3000));
                    self.offered_n += 1;
                }
            }
        }
        if self.offered_n == 0 || self.offered[0].0 != 5000 {
            say!(self.out, "err: the charger shows no usable fixed PDOs");
            self.offered_n = 0;
            return;
        }
        // Handover: charger at 5 V, then let it through. The mux prefers it.
        self.chg.request(5);
        Timer::after_millis(300).await;
        self.power.chg_en.set_high();
        self.caps = Caps::Charger;
        say!(
            self.out,
            "CHG_EN on; offering {} charger PDOs",
            self.offered_n
        );
        self.state = if self.state == State::Detached {
            State::Detached
        } else {
            State::Attached
        };
        if self.state != State::Detached {
            self.send_caps().await;
        }
    }

    // ------------------------------------------------------------ console

    async fn status(&mut self) {
        say!(
            self.out,
            "{}: {} | auto {} | cable {} | Rp level {} | caps {}",
            BOARD,
            self.state.name(),
            if self.auto { "on" } else { "off" },
            if self.cc2 { "flipped" } else { "normal" },
            self.rp,
            match self.caps {
                Caps::Lite => "lite",
                Caps::FiveVolt => "5v",
                #[cfg(feature = "pd")]
                Caps::Charger => "charger",
            }
        );
        if self.tcpc_ok {
            let cc = self.tcpc.r8(tcpc::CC_STATUS);
            let power = self.tcpc.r8(tcpc::POWER_STATUS);
            let role = self.tcpc.r8(tcpc::ROLE_CONTROL);
            let alert = self.tcpc.r16(tcpc::ALERT);
            say!(
                self.out,
                "HUSB311: cc_status {:02x?} power_status {:02x?} role_control {:02x?} alert {:04x?} INT_N {}",
                cc,
                power,
                role,
                alert,
                if self.pd_int.is_low() { "low" } else { "high" }
            );
        } else {
            say!(self.out, "HUSB311: not answering");
        }
        #[cfg(not(feature = "pd"))]
        say!(
            self.out,
            "power: VBUS_EN {}",
            if self.power.vbus_en.is_set_high() {
                "on"
            } else {
                "off"
            }
        );
        #[cfg(feature = "pd")]
        say!(
            self.out,
            "power: host {} CHG_EN {} CHG_HV {} | charger {}",
            if self.power.host { "on" } else { "off" },
            if self.power.chg_en.is_set_high() {
                "on"
            } else {
                "off"
            },
            if self.power.chg_hv.is_set_high() {
                "on"
            } else {
                "off"
            },
            if self.chg.addr.is_some() {
                "present"
            } else {
                "absent"
            }
        );
        say!(
            self.out,
            "SBU: shifters {} bridge {}",
            if self.sbu_pu.is_set_high() {
                "on"
            } else {
                "off"
            },
            if self.bridge.is_some() {
                "open"
            } else {
                "closed"
            }
        );
    }

    async fn help(&mut self) {
        for line in [
            "status                 everything at a glance",
            "auto on|off            attach handling by itself (on) or all by hand (off)",
            "scan                   which addresses answer on the HUSB311 bus",
            "tr REG [N]             read N HUSB311 registers from REG (hex)",
            "tw REG VAL             write a HUSB311 register (hex)",
            "rp 0|1|2               Rp level: default, 1.5 A, 3 A",
            "open                   CC open (no Rp)",
            "caps lite|5v           what to advertise, and send it now",
            "poke                   empty Apple VDM on SOP'' Debug",
            "dfu | reboot | debugusb   the Apple VDMs, as on the Lite",
            "replug                 target power off and CC open for 1 s, then attach again",
            "vdm W0 W1 ..           any VDM words (hex) on SOP'' Debug",
            "hardreset              PD hard reset",
            "serial                 Apple serial VDM, then bridge SBU to this console",
            "bridge 1|2             bridge without the VDM: 1 normal cable, 2 flipped",
            "sbu on|off             SBU level shifters",
        ] {
            say!(self.out, "{}", line);
        }
        #[cfg(not(feature = "pd"))]
        say!(self.out, "vbus on|off            target VBUS switch");
        #[cfg(feature = "pd")]
        for line in [
            "host on|off            host 5 V to the target",
            "chg on|off             charger to the target, while it is under about 6.4 V",
            "hv on|off              charger to the target at any voltage. Mind what is plugged in.",
            "chgs                   charger status and what it offers",
            "chgv 5|9|12|15|20      ask the charger for a voltage (decimal)",
            "cr REG | cw REG VAL    CH224Q register read / write (hex)",
            "caps chg               offer the target the charger's PDOs",
        ] {
            say!(self.out, "{}", line);
        }
    }

    async fn command(&mut self, text: &str) {
        let (cmd, args) = word(text);
        let (a0, rest) = word(args);
        let (a1, _) = word(rest);
        match cmd {
            "" => {}
            "help" | "?" => self.help().await,
            "status" => self.status().await,
            "auto" => match on_off(a0) {
                Some(on) => {
                    self.auto = on;
                    say!(self.out, "auto {}", a0);
                }
                None => say!(self.out, "auto on|off"),
            },
            "scan" => {
                let mut line = Line::new();
                let _ = write!(line, "answering:");
                for addr in 0x08..0x78 {
                    if self.tcpc.bus.probe(addr) {
                        let _ = write!(line, " {:02x}", addr);
                    }
                }
                self.out.line(&line);
            }
            "tr" => match hex(a0) {
                Some(reg) => {
                    let n = hex(a1).unwrap_or(1).clamp(1, 32);
                    let mut line = Line::new();
                    let _ = write!(line, "{:02x}:", reg);
                    for i in 0..n {
                        match self.tcpc.r8(reg as u8 + i as u8) {
                            Some(v) => {
                                let _ = write!(line, " {:02x}", v);
                            }
                            None => {
                                let _ = write!(line, " --");
                            }
                        }
                    }
                    self.out.line(&line);
                }
                None => say!(self.out, "tr REG [N]"),
            },
            "tw" => match (hex(a0), hex(a1)) {
                (Some(reg), Some(val)) => {
                    let ok = self.tcpc.w8(reg as u8, val as u8);
                    say!(
                        self.out,
                        "{:02x} <- {:02x}: {}",
                        reg,
                        val,
                        if ok { "ok" } else { "no ack" }
                    );
                }
                _ => say!(self.out, "tw REG VAL"),
            },
            "rp" => match hex(a0) {
                Some(level @ 0..=2) => {
                    self.rp = level as u8;
                    self.tcpc.set_rp(self.rp);
                    say!(self.out, "Rp level {}", level);
                }
                _ => say!(self.out, "rp 0|1|2"),
            },
            "open" => {
                self.tcpc.set_open();
                say!(self.out, "CC open");
            }
            "caps" => {
                match a0 {
                    "lite" => self.caps = Caps::Lite,
                    "5v" => self.caps = Caps::FiveVolt,
                    #[cfg(feature = "pd")]
                    "chg" => {
                        self.offer_charger().await;
                        return;
                    }
                    _ => {
                        say!(
                            self.out,
                            "caps lite|5v{}",
                            if cfg!(feature = "pd") { "|chg" } else { "" }
                        );
                        return;
                    }
                }
                if self.state != State::Detached {
                    self.state = State::Attached;
                    self.send_caps().await;
                }
            }
            "poke" => {
                self.poke().await;
            }
            "dfu" => {
                self.dfu_retries = 0;
                self.dfu_retry_at = None;
                self.dfu().await;
            }
            "replug" => self.replug().await,
            "reboot" => self.action("reboot", &[&pd::VDM_REBOOT]).await,
            "debugusb" => self.action("debugusb", &[&pd::VDM_DEBUGUSB]).await,
            "vdm" => {
                let mut words = [0u32; 7];
                let mut n = 0;
                let mut rest = args;
                while n < words.len() {
                    let (w, tail) = word(rest);
                    let Some(v) = hex(w) else { break };
                    words[n] = v;
                    n += 1;
                    rest = tail;
                }
                if n == 0 {
                    say!(self.out, "vdm W0 W1 ..");
                } else {
                    let res = self
                        .tcpc
                        .transmit(
                            tcpc::SOP_DBG2,
                            pd::header(pd::DATA_VENDOR_DEF, n as u16),
                            &words[..n],
                        )
                        .await;
                    say!(self.out, ">VDM {} words on SOP'' Debug: {}", n, res.name());
                }
            }
            "hardreset" => {
                self.tcpc.hard_reset();
                self.tcpc.reset_msg_ids();
                say!(self.out, ">hard reset");
            }
            "serial" => {
                if self.state == State::Detached {
                    say!(self.out, "err serial: no target");
                } else {
                    for _ in 0..2 {
                        let header =
                            pd::header(pd::DATA_VENDOR_DEF, pd::VDM_SERIAL_SBU.len() as u16);
                        let res = self
                            .tcpc
                            .transmit(tcpc::SOP_DBG2, header, &pd::VDM_SERIAL_SBU)
                            .await;
                        say!(self.out, ">VDM serial: {}", res.name());
                    }
                    self.enter_bridge(self.cc2).await;
                }
            }
            "bridge" => match a0 {
                "1" => self.enter_bridge(false).await,
                "2" => self.enter_bridge(true).await,
                _ => say!(self.out, "bridge 1|2"),
            },
            "sbu" => match on_off(a0) {
                Some(on) => {
                    self.sbu_pu
                        .set_level(if on { Level::High } else { Level::Low });
                    say!(self.out, "SBU shifters {}", a0);
                }
                None => say!(self.out, "sbu on|off"),
            },
            #[cfg(not(feature = "pd"))]
            "vbus" => match on_off(a0) {
                Some(true) => {
                    self.power.on();
                    say!(self.out, "VBUS_EN on");
                }
                Some(false) => {
                    self.power.off();
                    say!(self.out, "VBUS_EN off");
                }
                None => say!(self.out, "vbus on|off"),
            },
            #[cfg(feature = "pd")]
            "host" | "chg" | "hv" => match on_off(a0) {
                Some(on) => {
                    let level = if on { Level::High } else { Level::Low };
                    match cmd {
                        "host" => self.power.host(on),
                        "chg" => self.power.chg_en.set_level(level),
                        _ => self.power.chg_hv.set_level(level),
                    }
                    say!(self.out, "{} {}", cmd, a0);
                }
                None => say!(self.out, "{} on|off", cmd),
            },
            #[cfg(feature = "pd")]
            "chgs" => self.charger_status().await,
            #[cfg(feature = "pd")]
            "chgv" => match a0.parse::<u32>() {
                Ok(v) if self.chg.request_code(v) => {
                    let ok = self.chg.request(v);
                    say!(
                        self.out,
                        "charger asked for {} V: {}",
                        v,
                        if ok {
                            "written"
                        } else {
                            "CH224Q did not answer"
                        }
                    );
                }
                _ => say!(self.out, "chgv 5|9|12|15|20"),
            },
            #[cfg(feature = "pd")]
            "cr" => match hex(a0) {
                Some(reg) => say!(self.out, "{:02x}: {:02x?}", reg, self.chg.r8(reg as u8)),
                None => say!(self.out, "cr REG"),
            },
            #[cfg(feature = "pd")]
            "cw" => match (hex(a0), hex(a1)) {
                (Some(reg), Some(val)) => {
                    let ok = self.chg.w8(reg as u8, val as u8);
                    say!(
                        self.out,
                        "{:02x} <- {:02x}: {}",
                        reg,
                        val,
                        if ok { "ok" } else { "no ack" }
                    );
                }
                _ => say!(self.out, "cw REG VAL"),
            },
            _ => say!(self.out, "unknown command `{}`; try `help`", cmd),
        }
    }

    async fn console_poll(&mut self) {
        let mut buf = [0u8; 32];
        let Ok(n) = self.out.console.try_read(&mut buf) else {
            return;
        };
        for &b in &buf[..n] {
            match b {
                b'\r' | b'\n' => {
                    self.out.raw(b"\r\n");
                    let line = self.line;
                    let len = core::mem::take(&mut self.line_len);
                    if let Ok(text) = core::str::from_utf8(&line[..len]) {
                        self.command(text.trim()).await;
                    }
                    if self.bridge.is_some() {
                        return;
                    }
                    self.out.raw(b"> ");
                }
                0x08 | 0x7f => {
                    if self.line_len != 0 {
                        self.line_len -= 1;
                        self.out.raw(b"\x08 \x08");
                    }
                }
                0x20..=0x7e if self.line_len < self.line.len() => {
                    self.line[self.line_len] = b;
                    self.line_len += 1;
                    self.out.raw(&[b]);
                }
                _ => {}
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
    // pid.codes test identity on purpose: bench firmware must not be picked
    // up by restorekit as a dongle.
    let usb = usb::run(
        p.USBFS,
        p.PC17,
        p.PC16,
        usb::Options {
            vid: 0x1209,
            pid: 0x0001,
            product: if cfg!(feature = "pd") {
                "Dongle-Pro v2 PD bench"
            } else {
                "Dongle-Pro v2 bench"
            },
            serial: usb::serial_number(),
            second_port: false,
            vendor: None,
        },
    );
    let console = Port::console();

    // The design has 10k pull-ups, but the first board's bus floats low
    // without the MCU's own.
    let bus = I2c::new(Flex::new(p.PB9), Flex::new(p.PB10), Pull::Up);
    #[cfg(not(feature = "pd"))]
    let power = Power {
        vbus_en: Output::new(p.PB7, Level::Low, Default::default()),
    };
    #[cfg(feature = "pd")]
    let power = {
        let mut power = Power {
            host_en_n: Flex::new(p.PA6),
            host: false,
            chg_en: Output::new(p.PA7, Level::Low, Default::default()),
            chg_hv: Output::new(p.PB0, Level::Low, Default::default()),
        };
        power.host(false);
        power
    };
    let mut sbu_pins = [
        Flex::new(p.PA2),
        Flex::new(p.PA3),
        Flex::new(p.PB3),
        Flex::new(p.PB4),
    ];
    for pin in &mut sbu_pins {
        pin.set_as_input(Pull::None);
    }

    let mut bench = Bench {
        out: Out { console },
        tcpc: Tcpc::new(bus),
        pd_int: Input::new(p.PB8, Pull::Up),
        led: Output::new(p.PA1, Level::Low, Default::default()),
        sbu_pu: Output::new(p.PB6, Level::Low, Default::default()),
        power,
        #[cfg(feature = "pd")]
        chg: ch224q::Ch224q::new(I2c::new(Flex::new(p.PA4), Flex::new(p.PA5), Pull::None)),
        #[cfg(feature = "pd")]
        offered: [(0, 0); 7],
        #[cfg(feature = "pd")]
        offered_n: 0,
        sbu_pins,
        uart2: Some(uart::Controller::from(p.USART2)),
        uart3: Some(uart::Controller::from(p.USART3)),
        bridge: None,
        tcpc_ok: false,
        auto: true,
        state: State::Detached,
        cc2: false,
        rp: 0,
        caps: Caps::Lite,
        cap_timer: 0,
        detach_count: 0,
        dfu_sent_at: None,
        dfu_retry_at: None,
        dfu_retries: 0,
        ticks: 0,
        line: [0; 80],
        line_len: 0,
    };

    let run = async {
        let mut greeted = false;
        let mut next_tick = Instant::now();
        loop {
            Timer::after_millis(if bench.bridge.is_some() { 1 } else { 5 }).await;
            if bench.out.console.connected() != greeted {
                greeted = !greeted;
                if greeted {
                    Timer::after_millis(200).await;
                    bench.banner().await;
                    bench.out.raw(b"> ");
                }
            }
            if bench.bridge.is_some() {
                bench.bridge_poll().await;
            } else {
                bench.console_poll().await;
            }
            // A sink wants its Accept within tSenderResponse (24-30 ms), so don't
            // leave a received message for the 50 ms tick.
            if bench.tcpc_ok && (bench.pd_int.is_low() || bench.state != State::Detached) {
                bench.alerts().await;
            }
            if Instant::now() >= next_tick {
                next_tick = Instant::now() + Duration::from_millis(50);
                bench.tick().await;
            }
        }
    };
    join(usb, run).await;
    unreachable!()
}
