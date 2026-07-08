//! RecoverKit Dongle Lite - M0 bench firmware.
//!
//! Target: RP2040 (e.g. a Raspberry Pi Pico) wired to an FUSB302B breakout and
//! two 74AVC1T45 level translators on the target's SBU lines. This is the M0
//! de-risking milestone from the PRD: prove, against a real Apple Silicon / T2
//! Mac, that (a) the Apple DFU-trigger VDM works and (b) the 1.2 V SBU serial
//! console works in BOTH cable orientations, before committing a PCB.
//!
//! The host sees two USB CDC serial ports:
//!   CDC0 - control console. Type `dfu`, `reboot`, `serial`, `status`, `help`.
//!   CDC1 - the target's AP/SEP UART, bridged from the SBU pins at 115200 8N1.
//!
//! PD / VDM logic is a Rust port of AsahiLinux vdmtool and the Central
//! Scrutinizer, driving the FUSB302B in source mode over I2C.

#![no_std]
#![no_main]

#[allow(dead_code)]
mod fusb302;

use core::fmt::Write as _;

use defmt::info;
use embassy_executor::Spawner;
use embassy_futures::join::{join, join4};
use embassy_futures::select::{select, select3, Either, Either3};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::i2c::{self, I2c};
use embassy_rp::peripherals::{I2C0, PIO0, USB};
use embassy_rp::pio::{self, Pio};
use embassy_rp::peripherals::{PIN_12, PIN_13};
use embassy_rp::pio_programs::uart::{PioUartRx, PioUartRxProgram, PioUartTx, PioUartTxProgram};
use embassy_rp::usb::{Driver, InterruptHandler as UsbInterruptHandler};
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::pipe::Pipe;
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::{Builder, Config};
use embedded_io_async::{Read, Write};
use heapless::String;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

use fusb302::*;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => UsbInterruptHandler<USB>;
    I2C0_IRQ => i2c::InterruptHandler<I2C0>;
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
});

// --- Apple vendor-defined messages (SVID 0x05AC). ---
const VDM_DFU_HOLD: [u32; 3] = [0x5ac_8012, 0x0106, 0x8001_0000];
const VDM_REBOOT: [u32; 3] = [0x5ac_8012, 0x0105, 0x8000_0000];
const VDM_PD_RESET: [u32; 3] = [0x5ac_8012, 0x0103, 0x8000_0000];
// Mux the debug UART onto SBU1/2 (0x01800306 | 1<<(2+16) for the SBU pin set).
const VDM_SERIAL_SBU: [u32; 2] = [0x5ac_8012, 0x0184_0306];

// SBU serial console parameters (Asahi: 1.2 V, 115200 8N1).
const SBU_BAUD: u32 = 115_200;

// 74AVC1T45 DIR levels. Wiring: A-side = RP2040 GPIO, B-side = SBU pin.
// DIR high => A->B (RP2040 drives target = our TX). DIR low => B->A (our RX).
const DIR_TO_TARGET: Level = Level::High;
const DIR_FROM_TARGET: Level = Level::Low;

#[derive(Copy, Clone, defmt::Format)]
enum Command {
    Dfu,
    Reboot,
    Serial,
    Status,
    Help,
}

type LogLine = String<160>;
static CMD: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
static LOG: Channel<CriticalSectionRawMutex, LogLine, 16> = Channel::new();
// Signalled by the PD engine when serial mode is enabled; payload is polarity
// (0 = CC1/normal, 1 = CC2/flipped) so the bridge picks the right SBU pins.
static SERIAL_ENABLE: Signal<CriticalSectionRawMutex, u8> = Signal::new();

macro_rules! logline {
    ($($arg:tt)*) => {{
        let mut s: LogLine = String::new();
        let _ = core::write!(s, $($arg)*);
        let _ = LOG.try_send(s);
    }};
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    info!("dongle-lite M0 firmware boot");

    // --- USB composite device: two CDC-ACM ports. ---
    let driver = Driver::new(p.USB, Irqs);
    let mut config = Config::new(0x2e8a, 0x000a);
    config.manufacturer = Some("RecoverKit");
    config.product = Some("Dongle Lite (M0 bench)");
    config.serial_number = Some("M0-0001");
    config.max_power = 250;
    config.max_packet_size_0 = 64;
    // Composite device with IADs.
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
    static STATE0: StaticCell<State> = StaticCell::new();
    static STATE1: StaticCell<State> = StaticCell::new();

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        &mut [],
        CONTROL_BUF.init([0; 64]),
    );

    let control = CdcAcmClass::new(&mut builder, STATE0.init(State::new()), 64);
    let target_serial = CdcAcmClass::new(&mut builder, STATE1.init(State::new()), 64);
    let mut usb = builder.build();

    // --- FUSB302B on I2C0. ---
    let sda = p.PIN_16;
    let scl = p.PIN_17;
    let mut i2c_cfg = i2c::Config::default();
    i2c_cfg.frequency = 400_000;
    let i2c = I2c::new_async(p.I2C0, scl, sda, Irqs, i2c_cfg);
    let fusb = Fusb302::new(i2c);

    // FUSB302 INT (active low), target VBUS enable, status LED.
    let mut int = Input::new(p.PIN_20, Pull::Up);
    let vbus = Output::new(p.PIN_19, Level::Low);
    let led = Output::new(p.PIN_25, Level::Low);

    // SBU level-translator control.
    let shifter_supply = Output::new(p.PIN_14, Level::Low);
    let sbu1_dir = Output::new(p.PIN_10, Level::Low);
    let sbu2_dir = Output::new(p.PIN_11, Level::Low);

    let mut engine = Engine {
        fusb,
        vbus,
        led,
        shifter_supply,
        sbu1_dir,
        sbu2_dir,
        state: PdState::Disconnected,
        source_cap_timer: 0,
        cc_debounce: 0,
        cc_line: false,
    };

    // --- Concurrent futures on one executor. ---
    let usb_fut = usb.run();

    let (mut ctl_tx, mut ctl_rx) = control.split();
    let control_read = read_control(&mut ctl_rx);
    let log_write = drain_log(&mut ctl_tx);
    let control_fut = join(control_read, log_write);

    let pd_fut = engine.run(&mut int);

    let serial_fut = serial_bridge(
        p.PIO0, p.PIN_12, p.PIN_13, target_serial,
    );

    join4(usb_fut, control_fut, pd_fut, serial_fut).await;
}

// ---------------------------------------------------------------------------
// Control CDC: parse line-oriented commands, drain log lines back out.
// ---------------------------------------------------------------------------

async fn read_control<'d>(
    rx: &mut embassy_usb::class::cdc_acm::Receiver<'d, Driver<'d, USB>>,
) {
    let mut line: String<32> = String::new();
    let mut buf = [0u8; 64];
    loop {
        rx.wait_connection().await;
        loop {
            let n = match rx.read_packet(&mut buf).await {
                Ok(n) => n,
                Err(_) => break,
            };
            for &b in &buf[..n] {
                match b {
                    b'\r' | b'\n' => {
                        if let Some(cmd) = parse_command(line.as_str()) {
                            let _ = CMD.try_send(cmd);
                        } else if !line.is_empty() {
                            logline!("? unknown command '{}' (try 'help')", line.as_str());
                        }
                        line.clear();
                    }
                    _ => {
                        if line.push(b as char).is_err() {
                            line.clear();
                        }
                    }
                }
            }
        }
    }
}

fn parse_command(s: &str) -> Option<Command> {
    match s.trim() {
        "dfu" => Some(Command::Dfu),
        "reboot" => Some(Command::Reboot),
        "serial" => Some(Command::Serial),
        "status" => Some(Command::Status),
        "help" | "?" => Some(Command::Help),
        _ => None,
    }
}

async fn drain_log<'d>(tx: &mut embassy_usb::class::cdc_acm::Sender<'d, Driver<'d, USB>>) {
    loop {
        tx.wait_connection().await;
        // Greeting once connected.
        let _ = write_line(tx, "RecoverKit Dongle Lite (M0 bench). Type 'help'.").await;
        loop {
            let line = LOG.receive().await;
            if write_line(tx, line.as_str()).await.is_err() {
                break;
            }
        }
    }
}

async fn write_line<'d>(
    tx: &mut embassy_usb::class::cdc_acm::Sender<'d, Driver<'d, USB>>,
    s: &str,
) -> Result<(), EndpointError> {
    for chunk in s.as_bytes().chunks(60) {
        tx.write_packet(chunk).await?;
    }
    tx.write_packet(b"\r\n").await
}

// ---------------------------------------------------------------------------
// PD engine: FUSB302 source-mode state machine + Apple VDMs.
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, PartialEq, Eq)]
enum PdState {
    Disconnected,
    DfpVbusOn,
    DfpConnected,
    DfpAccept,
    Idle,
}

struct Engine<'a, I2C: embedded_hal_async::i2c::I2c> {
    fusb: Fusb302<I2C>,
    vbus: Output<'a>,
    led: Output<'a>,
    shifter_supply: Output<'a>,
    sbu1_dir: Output<'a>,
    sbu2_dir: Output<'a>,
    state: PdState,
    source_cap_timer: i32,
    cc_debounce: i32,
    cc_line: bool,
}

impl<'a, I2C: embedded_hal_async::i2c::I2c> Engine<'a, I2C> {
    async fn run(&mut self, int: &mut Input<'a>) {
        // Probe the FUSB302.
        let id = self.fusb.device_id().await;
        if id & 0x80 == 0 {
            logline!("FUSB302 not responding (id=0x{:02x})", id);
        } else {
            logline!("FUSB302 device id 0x{:02x}", id);
        }
        self.fusb.init().await;
        self.fusb.pd_reset().await;
        let _ = self.fusb.set_rx_enable(false).await;
        self.fusb.set_cc_open().await;
        Timer::after_millis(500).await;
        self.disconnect().await;

        loop {
            // React to an interrupt, a periodic tick, or a host command.
            match select3(int.wait_for_low(), Timer::after_millis(50), CMD.receive()).await {
                Either3::First(_) => {
                    self.handle_irq().await;
                    self.state_machine().await;
                }
                Either3::Second(_) => {
                    self.state_machine().await;
                }
                Either3::Third(cmd) => {
                    self.handle_command(cmd).await;
                }
            }
        }
    }

    async fn vbus_on(&mut self) {
        self.vbus.set_high();
        logline!("VBUS on");
    }

    async fn vbus_off(&mut self) {
        self.vbus.set_low();
    }

    async fn disconnect(&mut self) {
        self.vbus_off().await;
        self.fusb.set_vconn(false).await;
        self.fusb.pd_reset().await;
        let _ = self.fusb.set_rx_enable(false).await;
        self.fusb.select_rp_usb().await;
        // Re-arm as source (Rp) so a target's Rd is detected.
        self.fusb.set_cc_rp().await;
        self.state = PdState::Disconnected;
        self.led.set_low();
    }

    async fn dfp_connect(&mut self, cc1: i8, cc2: i8) {
        self.fusb.set_vconn(false).await;
        self.fusb.pd_reset().await;
        self.fusb.set_msg_header(true, true).await; // Source, DFP
        self.cc_line = !(cc1 > cc2);
        self.fusb.set_polarity(self.cc_line as i8).await;
        logline!(
            "connected: cc1={} cc2={} polarity=CC{} ({})",
            cc1,
            cc2,
            self.cc_line as u8 + 1,
            if self.cc_line { "flipped" } else { "normal" }
        );
        let _ = self.fusb.set_rx_enable(true).await;
        self.vbus_on().await;
        self.state = PdState::DfpVbusOn;
        self.led.set_high();
        self.debug_poke().await;
    }

    async fn debug_poke(&mut self) {
        let hdr = pd_header(PD_DATA_VENDOR_DEF, 1, 1, 0, 1, PD_REV20);
        self.fusb.transmit(TxSop::DebugPrimePrime, hdr, &[0]).await;
    }

    async fn send_source_cap(&mut self) {
        let hdr = pd_header(PD_DATA_SOURCE_CAP, 1, 1, 0, 1, PD_REV20);
        // Variable non-battery PS, 0V/0mA - we only signal, never power.
        let cap: u32 = 1u32 << 31;
        self.fusb.transmit(TxSop::Sop, hdr, &[cap]).await;
        self.source_cap_timer = 0;
    }

    async fn send_power_request(&mut self) {
        let hdr = pd_header(PD_DATA_REQUEST, 0, 0, 0, 1, PD_REV20);
        let req: u32 = (1u32 << 28) | (1u32 << 25);
        self.fusb.transmit(TxSop::Sop, hdr, &[req]).await;
    }

    async fn send_sink_cap(&mut self) {
        let hdr = pd_header(PD_DATA_SINK_CAP, 1, 1, 0, 1, PD_REV20);
        let cap: u32 = 1u32 << 26;
        self.fusb.transmit(TxSop::Sop, hdr, &[cap]).await;
        self.state = PdState::Idle;
    }

    async fn handle_discover_identity(&mut self) {
        let hdr = pd_header(PD_DATA_VENDOR_DEF, 0, 0, 0, 4, PD_REV20);
        let vdm = [
            0xff00_8001u32 | (1u32 << 6),
            (1u32 << 30) | 0x5acu32,
            0u32,
            (0x0001u32 << 16) | 0x100u32,
        ];
        self.fusb.transmit(TxSop::Sop, hdr, &vdm).await;
    }

    async fn accept_power_request(&mut self) {
        let hdr = pd_header(PD_CTRL_ACCEPT, 1, 1, 0, 0, PD_REV20);
        self.fusb.transmit(TxSop::Sop, hdr, &[]).await;
        self.state = PdState::DfpAccept;
    }

    async fn send_ps_rdy(&mut self) {
        let hdr = pd_header(PD_CTRL_PS_RDY, 1, 1, 0, 0, PD_REV20);
        self.fusb.transmit(TxSop::Sop, hdr, &[]).await;
        self.state = PdState::Idle;
    }

    async fn send_reject(&mut self) {
        let hdr = pd_header(PD_CTRL_REJECT, 1, 1, 0, 0, PD_REV20);
        self.fusb.transmit(TxSop::Sop, hdr, &[]).await;
        self.state = PdState::Idle;
    }

    async fn handle_msg(&mut self, hdr: u16, msg: &[u32]) {
        let len = pd_header_cnt(hdr);
        let mtype = pd_header_type(hdr);
        if len != 0 {
            match mtype {
                x if x == PD_DATA_SOURCE_CAP => self.send_power_request().await,
                x if x == PD_DATA_REQUEST => {
                    logline!("<REQUEST 0x{:08x}", msg[0]);
                    self.accept_power_request().await;
                }
                x if x == PD_DATA_VENDOR_DEF => {
                    if msg[0] == 0xff00_8001 {
                        self.handle_discover_identity().await;
                        self.state = PdState::Idle;
                    }
                }
                _ => {}
            }
        } else {
            match mtype {
                x if x == PD_CTRL_GET_SINK_CAP => self.send_sink_cap().await,
                x if x == PD_CTRL_PR_SWAP => self.send_reject().await,
                x if x == PD_CTRL_DR_SWAP => self.send_reject().await,
                _ => {}
            }
        }
        let _ = PD_CTRL_GOOD_CRC;
    }

    async fn handle_irq(&mut self) {
        let (irq, irqa, irqb) = self.fusb.get_irq().await;

        if irq & INT_VBUSOK != 0 {
            if self.fusb.vbus_ok().await {
                self.send_source_cap().await;
                self.debug_poke().await;
            } else {
                self.disconnect().await;
            }
        }
        if irqa & INTA_HARDRESET != 0 {
            logline!("hard reset");
            self.disconnect().await;
        }
        if irqa & INTA_TX_SUCCESS != 0 {
            self.on_tx_sent().await;
        }
        let _ = INTA_HARDSENT;
        if irqb & INTB_GCRCSENT != 0 {
            while !self.fusb.rx_fifo_is_empty().await {
                let mut payload = [0u32; 16];
                if let Some((hdr, n)) = self.fusb.get_message(&mut payload).await {
                    self.handle_msg(hdr, &payload[..n]).await;
                } else {
                    break;
                }
            }
        }
    }

    async fn on_tx_sent(&mut self) {
        match self.state {
            PdState::DfpVbusOn => {
                self.state = PdState::DfpConnected;
            }
            PdState::DfpAccept => {
                self.send_ps_rdy().await;
            }
            _ => {}
        }
    }

    async fn state_machine(&mut self) {
        match self.state {
            PdState::Disconnected => {
                // The FUSB302 needs a moment after an interrupt before the CC
                // measurement is reliable.
                Timer::after_millis(100).await;
                let (cc1, cc2) = self.fusb.get_cc().await;
                if cc1 >= 2 || cc2 >= 2 {
                    self.dfp_connect(cc1, cc2).await;
                } else {
                    self.vbus_off().await;
                }
                return;
            }
            PdState::DfpVbusOn => {
                self.source_cap_timer += 1;
                if self.source_cap_timer > 37 {
                    self.send_source_cap().await;
                    self.debug_poke().await;
                }
            }
            PdState::Idle | PdState::DfpConnected | PdState::DfpAccept => {}
        }

        // Disconnect detection.
        let (cc1, cc2) = self.fusb.get_cc().await;
        if cc1 < 2 && cc2 < 2 {
            self.cc_debounce += 1;
            if self.cc_debounce > 5 {
                logline!("disconnect: cc1={} cc2={}", cc1, cc2);
                self.disconnect().await;
                self.cc_debounce = 0;
            }
        } else {
            self.cc_debounce = 0;
        }
    }

    async fn handle_command(&mut self, cmd: Command) {
        match cmd {
            Command::Dfu => {
                self.fusb.transmit(TxSop::DebugPrimePrime, vdm_hdr(3), &VDM_DFU_HOLD).await;
                logline!(">VDM DFU hold (0x0106)");
            }
            Command::Reboot => {
                self.fusb.transmit(TxSop::DebugPrimePrime, vdm_hdr(3), &VDM_REBOOT).await;
                logline!(">VDM reboot (0x0105)");
            }
            Command::Serial => {
                // Re-establish the SOP'' path, then mux the UART onto SBU.
                self.fusb.transmit(TxSop::DebugPrimePrime, vdm_hdr(3), &VDM_PD_RESET).await;
                self.fusb.transmit(TxSop::DebugPrimePrime, vdm_hdr(2), &VDM_SERIAL_SBU).await;
                // Power the 1.2 V translators and set direction by orientation.
                self.shifter_supply.set_high();
                if self.cc_line {
                    // CC2/flipped: target TX on SBU2 (our RX), our TX on SBU1.
                    self.sbu2_dir.set_level(DIR_FROM_TARGET);
                    self.sbu1_dir.set_level(DIR_TO_TARGET);
                } else {
                    // CC1/normal: target TX on SBU1 (our RX), our TX on SBU2.
                    self.sbu1_dir.set_level(DIR_FROM_TARGET);
                    self.sbu2_dir.set_level(DIR_TO_TARGET);
                }
                SERIAL_ENABLE.signal(self.cc_line as u8);
                logline!(
                    ">VDM serial on SBU (polarity CC{}); bridging to CDC1",
                    self.cc_line as u8 + 1
                );
            }
            Command::Status => {
                let st = match self.state {
                    PdState::Disconnected => "disconnected",
                    PdState::DfpVbusOn => "vbus-on",
                    PdState::DfpConnected => "connected",
                    PdState::DfpAccept => "accept",
                    PdState::Idle => "idle",
                };
                logline!(
                    "status: {} polarity=CC{} vbus={}",
                    st,
                    self.cc_line as u8 + 1,
                    if self.vbus.is_set_high() { "on" } else { "off" }
                );
            }
            Command::Help => {
                logline!("commands: dfu | reboot | serial | status | help");
            }
        }
    }
}

fn vdm_hdr(cnt: u16) -> u16 {
    pd_header(PD_DATA_VENDOR_DEF, 1, 1, 0, cnt, PD_REV20)
}

// ---------------------------------------------------------------------------
// SBU serial bridge: PIO UART <-> CDC1, pins chosen by orientation.
// ---------------------------------------------------------------------------

async fn serial_bridge(
    pio: Peri<'static, PIO0>,
    pin12: Peri<'static, PIN_12>,
    pin13: Peri<'static, PIN_13>,
    cdc: CdcAcmClass<'static, Driver<'static, USB>>,
) {
    // Wait until the PD engine has muxed the UART and told us the orientation.
    let polarity = SERIAL_ENABLE.wait().await;

    let Pio {
        mut common, sm0, sm1, ..
    } = Pio::new(pio, Irqs);

    let tx_prog = PioUartTxProgram::new(&mut common);
    let rx_prog = PioUartRxProgram::new(&mut common);

    // Active-CC-side SBU is the target's TX (our RX); the other is our TX.
    // polarity 0 (CC1/normal): RX=SBU1(PIN_12), TX=SBU2(PIN_13).
    // polarity 1 (CC2/flipped): RX=SBU2(PIN_13), TX=SBU1(PIN_12).
    let (mut uart_rx, mut uart_tx) = if polarity == 0 {
        let rx = PioUartRx::new(SBU_BAUD, &mut common, sm1, pin12, &rx_prog);
        let tx = PioUartTx::new(SBU_BAUD, &mut common, sm0, pin13, &tx_prog);
        (rx, tx)
    } else {
        let rx = PioUartRx::new(SBU_BAUD, &mut common, sm1, pin13, &rx_prog);
        let tx = PioUartTx::new(SBU_BAUD, &mut common, sm0, pin12, &tx_prog);
        (rx, tx)
    };

    let mut to_usb: Pipe<CriticalSectionRawMutex, 64> = Pipe::new();
    let (to_usb_r, to_usb_w) = to_usb.split();
    let mut to_uart: Pipe<CriticalSectionRawMutex, 64> = Pipe::new();
    let (to_uart_r, to_uart_w) = to_uart.split();

    let (mut usb_tx, mut usb_rx) = cdc.split();

    let uart_read = async {
        let mut b = [0u8; 64];
        loop {
            let n = uart_rx.read(&mut b).await.unwrap_or(0);
            if n > 0 {
                to_usb_w.write(&b[..n]).await;
            }
        }
    };
    let usb_write = async {
        let mut b = [0u8; 64];
        loop {
            let n = to_usb_r.read(&mut b).await;
            if usb_tx.write_packet(&b[..n]).await.is_err() {
                usb_tx.wait_connection().await;
            }
        }
    };
    let usb_read = async {
        let mut b = [0u8; 64];
        loop {
            usb_rx.wait_connection().await;
            while let Ok(n) = usb_rx.read_packet(&mut b).await {
                to_uart_w.write(&b[..n]).await;
            }
        }
    };
    let uart_write = async {
        let mut b = [0u8; 64];
        loop {
            let n = to_uart_r.read(&mut b).await;
            let _ = uart_tx.write(&b[..n]).await;
        }
    };

    match select(join(uart_read, usb_write), join(usb_read, uart_write)).await {
        Either::First(_) | Either::Second(_) => {}
    }
}
