//! RecoverKit Dongle Lite - M0 bench firmware.
//!
//! Target: RP2040 (e.g. a Raspberry Pi Pico) wired to an FUSB302B breakout and
//! two 74AVC1T45 level translators on the target's SBU lines. This is the M0
//! de-risking milestone from the PRD: prove, against a real Apple Silicon / T2
//! Mac, that (a) the Apple DFU-trigger VDM works and (b) the 1.2 V SBU serial
//! console works in BOTH cable orientations, before committing a PCB.
//!
//! The host sees two USB CDC serial ports:
//!   CDC0 - control console. Commands (matching macvdmtool): `nop`, `dfu`,
//!          `reboot`, `serial`, `debugusb`, `reboot serial`, `reboot debugusb`,
//!          `status`, `help`, `bootsel`. Each answers with a terminal `ok <cmd>` or
//!          `err <cmd> <reason>` line so a host tool can drive it.
//!   CDC1 - the target's AP/SEP UART, bridged from the SBU pins at 115200 8N1.
//!
//! PD / VDM logic is a Rust port of AsahiLinux vdmtool and the Central
//! Scrutinizer, driving the FUSB302B in source mode over I2C.

#![no_std]
#![no_main]

#[allow(dead_code)]
mod fusb302;

use core::cell::RefCell;
use core::fmt::Write as _;
use core::sync::atomic::Ordering;

use portable_atomic::{AtomicBool, AtomicU8};

use defmt::info;
use embassy_boot_rp::{AlignedBuffer, BlockingFirmwareUpdater, FirmwareUpdaterConfig};
use embassy_embedded_hal::flash::partition::BlockingPartition;
use embassy_executor::Spawner;
use embassy_futures::join::{join, join4};
use embassy_futures::select::{select, select3, Either, Either3};
use embassy_rp::bind_interrupts;
use embassy_rp::flash::{Blocking, Flash};
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::i2c::{self, I2c};
use embassy_rp::peripherals::{I2C0, PIO0, USB};
use embassy_rp::peripherals::{PIN_12, PIN_13};
use embassy_rp::pio::{self, Pio};
use embassy_rp::pio_programs::uart::{PioUartRx, PioUartRxProgram, PioUartTx, PioUartTxProgram};
use embassy_rp::usb::{Driver, InterruptHandler as UsbInterruptHandler};
use embassy_rp::watchdog::Watchdog;
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::pipe::Pipe;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::control::{InResponse, OutResponse, Recipient, Request, RequestType};
use embassy_usb::driver::EndpointError;
use embassy_usb::msos;
use embassy_usb::types::InterfaceNumber;
use embassy_usb::{Builder, Config, Handler};
use embedded_io_async::{Read, Write};
use heapless::String;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

// The USB contract shared with the host SDK (`restorekit::dongle`): VID/PID,
// string descriptors, and the vendor control protocol.
use restorekit_dongle_proto as proto;
use restorekit_dongle_proto::{
    crc32, FLAG_POLARITY_CC2, FLAG_TARGET_ATTACHED, FW_CHUNK, RES_NONE, RES_NOTARGET, RES_OK,
    RES_PENDING, STATUS_LEN, STATUS_VERSION, VCMD_BOOTSEL, VCMD_DEBUGUSB, VCMD_DFU, VCMD_NOP,
    VCMD_REBOOT, VCMD_SERIAL, VREQ_CMD, VREQ_FW_BEGIN, VREQ_FW_DATA, VREQ_FW_DONE, VREQ_STATUS,
};

use fusb302::*;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => UsbInterruptHandler<USB>;
    I2C0_IRQ => i2c::InterruptHandler<I2C0>;
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
});

// --- Apple vendor-defined messages (SVID 0x05AC), matching macvdmtool. ---
// All are sent over the SOP'' debug path.
const VDM_DFU_HOLD: [u32; 3] = [0x5ac_8012, 0x0106, 0x8001_0000];
// Intel T2 Macs NAK the Apple Silicon DFU action (0x0106); they enter DFU via
// the reboot action (0x0105) with the PMU-reset + DFU-hold flag (0x8002_0000),
// the "PMU Reset + DFU Hold" encoding from AsahiLinux/vdmtool. We can't read
// the target's NAK over SOP'', so `dfu` sprays both and whichever the target
// accepts lands.
const VDM_DFU_HOLD_T2: [u32; 3] = [0x5ac_8012, 0x0105, 0x8002_0000];
const VDM_REBOOT: [u32; 3] = [0x5ac_8012, 0x0105, 0x8000_0000];
// Mux the debug UART onto SBU1/2 (0x01800306 | 1<<(2+16) for the SBU pin set).
const VDM_SERIAL_SBU: [u32; 2] = [0x5ac_8012, 0x0184_0306];
// Switch the target's D+/D- to its debug-USB interface.
const VDM_DEBUGUSB: [u32; 2] = [0x5ac_8012, 0x0182_4606];

// How long after a `reboot serial` / `reboot debugusb` we wait for the target
// to re-attach before giving up on the follow-up action.
const REBOOT_RECONNECT_WINDOW_SECS: u64 = 30;

// Consecutive ~50 ms disconnect-detection ticks of CC loss before we tear the
// connection down. ~2 s, to ride out flaky hand-wired-bench CC contacts.
const DISCONNECT_DEBOUNCE_TICKS: i32 = 40;

// Hardware watchdog period. The PD loop feeds it every iteration; if the engine
// livelocks (e.g. an FUSB302 that goes unreachable mid-I2C) it resets, which
// also clears the GPIOs — dropping VBUS to the target — for a clean recovery.
const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(4);

// Consecutive I2C failures before we force a reset rather than run blind on
// zeroed reads that masquerade as real register values.
const I2C_FAIL_RESET_THRESHOLD: u32 = 100;

// Cap on messages drained from the Rx FIFO per interrupt, so a stuck "not
// empty" status (e.g. an I2C read failing) can't spin the drain loop forever.
const RX_DRAIN_MAX: usize = 16;

// While attached but not yet in a PD contract, how many ~50 ms ticks between
// re-sending source cap + debug poke. Reaching `Connected` needs one handshake
// to land during a good-contact moment, so retry briskly (~500 ms) to catch
// that window faster on a marginal contact.
const HANDSHAKE_RETRY_TICKS: i32 = 10;

// SBU serial console parameters (Asahi: 1.2 V, 115200 8N1).
const SBU_BAUD: u32 = 115_200;

// 74AVC1T45 DIR levels. Wiring: A-side = RP2040 GPIO, B-side = SBU pin.
// DIR high => A->B (RP2040 drives target = our TX). DIR low => B->A (our RX).
const DIR_TO_TARGET: Level = Level::High;
const DIR_FROM_TARGET: Level = Level::Low;

// --- Vendor USB interface (class 0xFF), driven by the recoverkit SDK over
// nusb control transfers. Runs alongside the human CDC console; both funnel
// into the same command path. ---
const FLASH_SIZE: usize = 2 * 1024 * 1024; // Pico: 2 MiB QSPI flash.

// The VREQ_*/VCMD_*/RES_*/FLAG_* wire constants live in the shared
// restorekit-dongle-proto crate (imported above).

// Vendor code (device recipient) Windows uses to fetch the MS OS 2.0 descriptor
// that auto-binds WinUSB to the vendor interface. Distinct recipient from the
// VREQ_* interface requests, so no collision.
const MSOS_VENDOR_CODE: u8 = 0x17;

// Published by the PD engine, read by the vendor STATUS control-IN handler.
static VENDOR_STATE: AtomicU8 = AtomicU8::new(0);
static VENDOR_FLAGS: AtomicU8 = AtomicU8::new(0);
static VENDOR_RESULT: AtomicU8 = AtomicU8::new(RES_NONE);
static VENDOR_SEQ: AtomicU8 = AtomicU8::new(0);

// Set once a streamed firmware update is verified and marked; the PD engine
// notices and reboots into the bootloader, which swaps the new image in.
static FW_REBOOT: AtomicBool = AtomicBool::new(false);

// The staged-update (DFU) slot, from the linker script; the app never
// executes from it, it's only the landing zone for streamed images.
extern "C" {
    static __bootloader_dfu_start: u8;
    static __bootloader_dfu_end: u8;
}

// NoopRawMutex: flash is only ever touched from the single thread-mode
// executor (UID read at startup, updater calls from the USB task).
type AppFlash = Flash<'static, embassy_rp::peripherals::FLASH, Blocking, FLASH_SIZE>;
type FlashMutex = embassy_sync::blocking_mutex::Mutex<
    embassy_sync::blocking_mutex::raw::NoopRawMutex,
    RefCell<AppFlash>,
>;
type FlashPartition =
    BlockingPartition<'static, embassy_sync::blocking_mutex::raw::NoopRawMutex, AppFlash>;
type Updater = BlockingFirmwareUpdater<'static, FlashPartition, FlashPartition>;

#[derive(Copy, Clone, defmt::Format)]
enum Command {
    Nop,
    Dfu,
    Reboot,
    Serial,
    DebugUsb,
    RebootThen(PostReboot),
    Status,
    Help,
    Bootsel,
}

/// Mode to enter automatically once the target re-attaches after a reboot.
#[derive(Copy, Clone, defmt::Format)]
enum PostReboot {
    Serial,
    DebugUsb,
}

impl PostReboot {
    fn name(self) -> &'static str {
        match self {
            PostReboot::Serial => "serial",
            PostReboot::DebugUsb => "debugusb",
        }
    }
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

/// Vendor-interface control handler. A command OUT funnels into the shared
/// [`CMD`] channel (same path as the CDC console); the async PD engine does the
/// work and publishes the outcome, which the host reads back via the status IN.
/// Firmware-update requests are handled inline instead: each chunk is written
/// to the DFU slot before the transfer completes, so the host is naturally
/// flow-controlled by the control pipe.
struct VendorHandler {
    iface: InterfaceNumber,
    updater: Updater,
    /// Announced image size; 0 = no update in progress.
    fw_size: u32,
    /// Bytes staged into the DFU slot so far.
    fw_staged: u32,
    /// Absolute (XIP) address and length of the DFU slot.
    dfu_addr: u32,
    dfu_len: u32,
}

impl VendorHandler {
    fn is_ours(&self, req: &Request) -> bool {
        req.request_type == RequestType::Vendor
            && req.recipient == Recipient::Interface
            && req.index as u8 == self.iface.0
    }

    fn fw_abort(&mut self) -> Option<OutResponse> {
        self.fw_size = 0;
        self.fw_staged = 0;
        Some(OutResponse::Rejected)
    }

    /// Start a streamed firmware update: data = image size (u32 LE).
    fn fw_begin(&mut self, data: &[u8]) -> Option<OutResponse> {
        let Some(size) = data
            .get(..4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        else {
            return self.fw_abort();
        };
        if size == 0 || size > self.dfu_len {
            logline!("err update: size {} won't fit the DFU slot", size);
            return self.fw_abort();
        }
        self.fw_size = size;
        self.fw_staged = 0;
        logline!("update: receiving {} bytes", size);
        Some(OutResponse::Accepted)
    }

    /// Stage one chunk; wValue = sequential chunk index, data = FW_CHUNK bytes.
    fn fw_data(&mut self, req: &Request, data: &[u8]) -> Option<OutResponse> {
        if self.fw_size == 0 || data.len() != FW_CHUNK {
            return self.fw_abort();
        }
        let offset = req.value as u32 * FW_CHUNK as u32;
        if offset != self.fw_staged || offset + FW_CHUNK as u32 > self.dfu_len {
            logline!("err update: chunk out of order");
            return self.fw_abort();
        }
        // Blocks until the sector is erased + written (~50 ms): the control
        // transfer's status stage is the host's write barrier.
        if self.updater.write_firmware(offset as usize, data).is_err() {
            logline!("err update: flash write failed at {}", offset);
            return self.fw_abort();
        }
        self.fw_staged += FW_CHUNK as u32;
        Some(OutResponse::Accepted)
    }

    /// Verify the staged image (data = CRC-32 LE), mark it, and reboot.
    fn fw_done(&mut self, data: &[u8]) -> Option<OutResponse> {
        let Some(crc) = data
            .get(..4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        else {
            return self.fw_abort();
        };
        if self.fw_size == 0 || self.fw_staged < self.fw_size {
            return self.fw_abort();
        }
        // The DFU slot is memory-mapped; read it back straight from XIP.
        let staged = unsafe {
            core::slice::from_raw_parts(self.dfu_addr as *const u8, self.fw_size as usize)
        };
        if crc32(staged) != crc {
            logline!("err update: CRC mismatch, not applying");
            return self.fw_abort();
        }
        if self.updater.mark_updated().is_err() {
            logline!("err update: marking failed");
            return self.fw_abort();
        }
        self.fw_size = 0;
        self.fw_staged = 0;
        logline!("ok update; rebooting into the new firmware");
        FW_REBOOT.store(true, Ordering::Relaxed);
        Some(OutResponse::Accepted)
    }
}

impl Handler for VendorHandler {
    fn control_out(&mut self, req: Request, data: &[u8]) -> Option<OutResponse> {
        if !self.is_ours(&req) {
            return None;
        }
        match req.request {
            VREQ_FW_BEGIN => return self.fw_begin(data),
            VREQ_FW_DATA => return self.fw_data(&req, data),
            VREQ_FW_DONE => return self.fw_done(data),
            VREQ_CMD => {}
            _ => return Some(OutResponse::Rejected),
        }
        let cmd = match req.value {
            VCMD_NOP => Command::Nop,
            VCMD_DFU => Command::Dfu,
            VCMD_REBOOT => Command::Reboot,
            VCMD_SERIAL => Command::Serial,
            VCMD_DEBUGUSB => Command::DebugUsb,
            // Resets into the bootloader, so its result is never published.
            VCMD_BOOTSEL => Command::Bootsel,
            _ => return Some(OutResponse::Rejected),
        };
        // Mark pending and enqueue; the engine resolves the result.
        VENDOR_RESULT.store(RES_PENDING, Ordering::Relaxed);
        VENDOR_SEQ.fetch_add(1, Ordering::Relaxed);
        let _ = CMD.try_send(cmd);
        Some(OutResponse::Accepted)
    }

    fn control_in<'a>(&'a mut self, req: Request, buf: &'a mut [u8]) -> Option<InResponse<'a>> {
        if !self.is_ours(&req) {
            return None;
        }
        if req.request != VREQ_STATUS {
            return Some(InResponse::Rejected);
        }
        // Status struct: [version, pd_state, flags, last_result, seq].
        buf[0] = STATUS_VERSION;
        buf[1] = VENDOR_STATE.load(Ordering::Relaxed);
        buf[2] = VENDOR_FLAGS.load(Ordering::Relaxed);
        buf[3] = VENDOR_RESULT.load(Ordering::Relaxed);
        buf[4] = VENDOR_SEQ.load(Ordering::Relaxed);
        Some(InResponse::Accepted(&buf[..STATUS_LEN]))
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    info!("dongle-lite M0 firmware boot");

    // Boot beacon: the bootloader pulses the LED and hands it over dark; light
    // it again to show the app is alive, until the PD engine takes it over.
    // With no debug probe: one pulse then dark = the app never got here;
    // pulse + relight then stuck on = app died before the engine.
    let led = Output::new(p.PIN_25, Level::High);

    // --- USB composite device: two CDC-ACM ports + a vendor interface. ---
    let driver = Driver::new(p.USB, Irqs);

    // Flash is shared between the one-shot UID read and the firmware updater.
    static FLASH_CELL: StaticCell<FlashMutex> = StaticCell::new();
    let flash = FLASH_CELL.init(embassy_sync::blocking_mutex::Mutex::new(RefCell::new(
        Flash::new_blocking(p.FLASH),
    )));

    // Unique per-unit USB serial derived from the RP2040 flash UID, so multiple
    // dongles on one host are individually addressable (e.g. "DL-1A2B3C4D").
    static SERIAL: StaticCell<String<24>> = StaticCell::new();
    let serial = {
        let mut uid = [0u8; 8];
        flash.lock(|f| {
            let _ = f.borrow_mut().blocking_unique_id(&mut uid);
        });
        let s = SERIAL.init(String::new());
        let _ = core::write!(
            s,
            "{}{:02X}{:02X}{:02X}{:02X}",
            proto::SERIAL_PREFIX_LITE,
            uid[4],
            uid[5],
            uid[6],
            uid[7]
        );
        s.as_str()
    };

    // The embassy-boot updater for streamed firmware updates. Marking this
    // boot healthy confirms a freshly swapped image, so the bootloader won't
    // revert it on the next reset.
    // The state buffer must be exactly the flash's WRITE_SIZE (1 on RP2040);
    // embassy-boot asserts on it, and a panic here would boot-loop the board.
    static UPDATER_BUF: StaticCell<AlignedBuffer<1>> = StaticCell::new();
    let mut updater = BlockingFirmwareUpdater::new(
        FirmwareUpdaterConfig::from_linkerfile_blocking(flash, flash),
        &mut UPDATER_BUF.init(AlignedBuffer([0; 1])).0,
    );
    if updater.mark_booted().is_err() {
        info!("could not mark boot state; updates may revert");
    }

    // RecoverKit's VID/PID (16D0:14F0, assigned via MCS Electronics). Every
    // RecoverKit model shares it; the host tells models apart by the iProduct
    // string (see DongleModel in the SDK's dongle.rs).
    let mut config = Config::new(proto::VID, proto::PID);
    config.manufacturer = Some(proto::MANUFACTURER);
    config.product = Some(proto::PRODUCT_LITE);
    config.serial_number = Some(serial);
    config.max_power = 250;
    config.max_packet_size_0 = 64;
    // Composite device with IADs.
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    // Sized for a whole firmware-update chunk in one control transfer.
    static CONTROL_BUF: StaticCell<[u8; proto::FW_CHUNK]> = StaticCell::new();
    static STATE0: StaticCell<State> = StaticCell::new();
    static STATE1: StaticCell<State> = StaticCell::new();

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        MSOS_DESC.init([0; 256]),
        CONTROL_BUF.init([0; proto::FW_CHUNK]),
    );

    // MS OS 2.0 descriptors: make Windows auto-bind WinUSB to the vendor
    // interface so the SDK can talk to it with no manual driver install.
    builder.msos_descriptor(msos::windows_version::WIN8_1, MSOS_VENDOR_CODE);

    let control = CdcAcmClass::new(&mut builder, STATE0.init(State::new()), 64);
    let target_serial = CdcAcmClass::new(&mut builder, STATE1.init(State::new()), 64);

    // Vendor-specific interface (class 0xFF), control-transfer only — the SDK
    // transport. Not claimed by any OS driver, so nusb can talk to it directly.
    static VHANDLER: StaticCell<VendorHandler> = StaticCell::new();
    let vendor_iface = {
        let mut func = builder.function(0xFF, 0x00, 0x00);
        // Tell Windows this function speaks WinUSB.
        func.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
        let mut iface = func.interface();
        let n = iface.interface_number();
        let _alt = iface.alt_setting(0xFF, 0x00, 0x00, None);
        n
    };
    let (dfu_addr, dfu_len) = unsafe {
        let start = &__bootloader_dfu_start as *const u8 as u32;
        let end = &__bootloader_dfu_end as *const u8 as u32;
        (embassy_rp::flash::FLASH_BASE as u32 + start, end - start)
    };
    builder.handler(VHANDLER.init(VendorHandler {
        iface: vendor_iface,
        updater,
        fw_size: 0,
        fw_staged: 0,
        dfu_addr,
        dfu_len,
    }));

    let mut usb = builder.build();

    // --- FUSB302B on I2C0. ---
    let sda = p.PIN_16;
    let scl = p.PIN_17;
    let mut i2c_cfg = i2c::Config::default();
    i2c_cfg.frequency = 400_000;
    let i2c = I2c::new_async(p.I2C0, scl, sda, Irqs, i2c_cfg);
    let fusb = Fusb302::new(i2c);

    // FUSB302 INT (active low), target VBUS enable. The boot-beacon LED from
    // the top of main is handed to the engine, which manages it from here on.
    let mut int = Input::new(p.PIN_20, Pull::Up);
    let vbus = Output::new(p.PIN_19, Level::Low);

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
        pending_after_reconnect: None,
        pending_expiry: Instant::from_ticks(0),
        watchdog: Watchdog::new(p.WATCHDOG),
    };

    // --- Concurrent futures on one executor. ---
    let usb_fut = usb.run();

    let (mut ctl_tx, mut ctl_rx) = control.split();
    let control_read = read_control(&mut ctl_rx);
    let log_write = drain_log(&mut ctl_tx);
    let control_fut = join(control_read, log_write);

    let pd_fut = engine.run(&mut int);

    let serial_fut = serial_bridge(p.PIO0, p.PIN_12, p.PIN_13, target_serial);

    join4(usb_fut, control_fut, pd_fut, serial_fut).await;
}

// ---------------------------------------------------------------------------
// Control CDC: parse line-oriented commands, drain log lines back out.
// ---------------------------------------------------------------------------

async fn read_control<'d>(rx: &mut embassy_usb::class::cdc_acm::Receiver<'d, Driver<'d, USB>>) {
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
        "nop" => Some(Command::Nop),
        "dfu" => Some(Command::Dfu),
        "reboot" => Some(Command::Reboot),
        "serial" => Some(Command::Serial),
        "debugusb" => Some(Command::DebugUsb),
        "reboot serial" => Some(Command::RebootThen(PostReboot::Serial)),
        "reboot debugusb" => Some(Command::RebootThen(PostReboot::DebugUsb)),
        "status" => Some(Command::Status),
        "help" | "?" => Some(Command::Help),
        "bootsel" => Some(Command::Bootsel),
        _ => None,
    }
}

async fn drain_log<'d>(tx: &mut embassy_usb::class::cdc_acm::Sender<'d, Driver<'d, USB>>) {
    loop {
        tx.wait_connection().await;
        // Greeting once connected.
        let _ = write_line(tx, "RecoverKit Dongle-Lite. Type 'help'.").await;
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
    // Armed by `reboot serial` / `reboot debugusb`: the mode to enter once the
    // target re-attaches, and the deadline past which we stop waiting for it.
    pending_after_reconnect: Option<PostReboot>,
    pending_expiry: Instant,
    watchdog: Watchdog,
}

impl<'a, I2C: embedded_hal_async::i2c::I2c> Engine<'a, I2C> {
    async fn run(&mut self, int: &mut Input<'a>) {
        // Arm the watchdog before touching the (possibly flaky) I2C bus, so a
        // hang anywhere below resets us instead of bricking until unplugged.
        self.watchdog.start(WATCHDOG_TIMEOUT);

        // Probe the FUSB302, retrying — a dead probe is a reset/wiring issue,
        // and running init() over dead I2C just yields a zombie dongle.
        loop {
            let id = self.fusb.device_id().await;
            if id & 0x80 != 0 {
                logline!("FUSB302 device id 0x{:02x}", id);
                break;
            }
            logline!("FUSB302 not responding (id=0x{:02x}); retrying", id);
            self.watchdog.feed(WATCHDOG_TIMEOUT);
            Timer::after_millis(200).await;
        }

        self.fusb.init().await;
        self.fusb.pd_reset().await;
        let _ = self.fusb.set_rx_enable(false).await;
        self.fusb.set_cc_open().await;
        Timer::after_millis(500).await;
        self.disconnect().await;

        self.publish_status();
        loop {
            // A verified update was staged and marked: reboot so the
            // bootloader swaps it in. The delay lets USB traffic drain.
            if FW_REBOOT.load(Ordering::Relaxed) {
                Timer::after_millis(250).await;
                self.watchdog.trigger_reset();
            }
            self.watchdog.feed(WATCHDOG_TIMEOUT);
            // A persistently unreachable FUSB302 returns zeroed reads that look
            // like real register state; reset to recover rather than run blind.
            if self.fusb.i2c_fail_streak() > I2C_FAIL_RESET_THRESHOLD {
                self.watchdog.trigger_reset();
            }
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
            self.publish_status();
        }
    }

    /// Publish the current PD state + flags for the vendor STATUS request.
    fn publish_status(&self) {
        let st = match self.state {
            PdState::Disconnected => proto::PD_DISCONNECTED,
            PdState::DfpVbusOn => proto::PD_VBUS_ON,
            PdState::DfpConnected => proto::PD_CONNECTED,
            PdState::DfpAccept => proto::PD_ACCEPT,
            PdState::Idle => proto::PD_IDLE,
        };
        VENDOR_STATE.store(st, Ordering::Relaxed);
        let mut flags = 0u8;
        if self.connected() {
            flags |= FLAG_TARGET_ATTACHED;
        }
        if self.cc_line {
            flags |= FLAG_POLARITY_CC2;
        }
        VENDOR_FLAGS.store(flags, Ordering::Relaxed);
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

    /// Bring up the source PD connection + SOP'' debug path from a CC reading.
    /// No action VDM and no pending follow-up — that's [`dfp_connect`].
    async fn establish(&mut self, cc1: i8, cc2: i8) {
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
        // Establish the SOP'' debug path. No action VDM is sent automatically;
        // the host must issue an explicit command.
        let poke = self.debug_poke().await;
        self.advance_after_tx(poke);
    }

    async fn dfp_connect(&mut self, cc1: i8, cc2: i8) {
        self.establish(cc1, cc2).await;

        // If a `reboot serial` / `reboot debugusb` armed a follow-up and the
        // target came back in time, fire it now.
        if let Some(mode) = self.pending_after_reconnect.take() {
            if Instant::now() <= self.pending_expiry {
                logline!("reconnect: firing pending {}", mode.name());
                match mode {
                    PostReboot::Serial => self.enter_serial().await,
                    PostReboot::DebugUsb => self.action("debugusb", &[&VDM_DEBUGUSB], true).await,
                }
            } else {
                logline!("reconnect window expired; dropping pending {}", mode.name());
            }
        }
    }

    /// Once any transmit is acknowledged we know PD comms are up.
    fn advance_after_tx(&mut self, res: TxResult) {
        if res.is_ok() && self.state == PdState::DfpVbusOn {
            self.state = PdState::DfpConnected;
        }
    }

    fn connected(&self) -> bool {
        self.state != PdState::Disconnected
    }

    async fn debug_poke(&mut self) -> TxResult {
        let hdr = pd_header(PD_DATA_VENDOR_DEF, 1, 1, 0, 1, PD_REV20);
        self.fusb.transmit(TxSop::DebugPrimePrime, hdr, &[0]).await
    }

    /// Send an Apple action VDM over SOP''.
    ///
    /// These VDMs (dfu/reboot/debugusb) make the target *act* — it reboots
    /// rather than returning a GoodCRC — so there is no ack to wait on, and the
    /// FUSB's auto-retry reporting RETRYFAIL is the normal, successful case.
    /// We fire a few times for reliability over a marginal link and report it
    /// as sent; the real confirmation is the Mac re-enumerating in DFU on the
    /// host, which the recoverkit SDK watches for.
    async fn action(&mut self, name: &str, variants: &[&[u32]], reprime: bool) {
        if !self.connected() {
            logline!("err {} no-target", name);
            VENDOR_RESULT.store(RES_NOTARGET, Ordering::Relaxed);
            return;
        }
        logline!(">VDM {}", name);
        // Commands whose target keeps running (dfu/debugusb) can hit a stale PD
        // session: the Mac can reboot without ever dropping CC, so its session
        // moves on and the VDM lands on nothing. Force a fresh one by briefly
        // opening our CC pull-up — the same detach the FUSB re-init does at boot
        // (which is why a fresh flash makes dfu work). Reboot must NOT do this;
        // it works bare and any pre-traffic stops the Mac acting on it.
        if reprime {
            self.reestablish_session().await;
        }
        // Spray the VDM over ~1.5 s. Booted macOS acts on the first, but a Mac
        // in the DFU bootrom processes these far less reliably, so repeat to
        // land one in its window. When several variants are given (e.g. the
        // Apple Silicon and T2 DFU encodings), send each per round — the target
        // NAKs and ignores the ones it doesn't implement.
        for _ in 0..12 {
            for words in variants {
                self.send_vdm(words).await;
            }
            self.watchdog.feed(WATCHDOG_TIMEOUT);
            Timer::after_millis(120).await;
        }
        logline!("ok {} (sent)", name);
        VENDOR_RESULT.store(RES_OK, Ordering::Relaxed);
    }

    /// Force the target to reset its PD session: briefly drop our CC pull-up so
    /// it sees a source detach, then re-run the connect handshake. Mirrors the
    /// CC-open the FUSB re-init performs at boot.
    async fn reestablish_session(&mut self) {
        self.watchdog.feed(WATCHDOG_TIMEOUT);
        self.fusb.set_cc_open().await;
        Timer::after_millis(1000).await;
        self.fusb.set_cc_rp().await;
        // Wait for the target to re-present its Rd before re-running the connect
        // handshake — a fixed delay races the Mac's re-attach. Feed the watchdog
        // since this runs outside the main loop's feed.
        for _ in 0..15 {
            self.watchdog.feed(WATCHDOG_TIMEOUT);
            Timer::after_millis(100).await;
            let (cc1, cc2) = self.fusb.get_cc().await;
            if cc1 >= 2 || cc2 >= 2 {
                self.establish(cc1, cc2).await;
                Timer::after_millis(200).await;
                return;
            }
        }
    }

    async fn send_vdm(&mut self, words: &[u32]) -> TxResult {
        self.fusb
            .transmit(TxSop::DebugPrimePrime, vdm_hdr(words.len() as u16), words)
            .await
    }

    async fn send_source_cap(&mut self) -> TxResult {
        let hdr = pd_header(PD_DATA_SOURCE_CAP, 1, 1, 0, 1, PD_REV20);
        // Variable non-battery PS, 0V/0mA - we only signal, never power.
        let cap: u32 = 1u32 << 31;
        let res = self.fusb.transmit(TxSop::Sop, hdr, &[cap]).await;
        self.source_cap_timer = 0;
        res
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
        self.state = PdState::DfpAccept;
        let res = self.fusb.transmit(TxSop::Sop, hdr, &[]).await;
        if res.is_ok() {
            self.send_ps_rdy().await;
        }
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
        logline!("<rx msg type=0x{:x} len={}", mtype, len);
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
    }

    async fn handle_irq(&mut self) {
        let (irq, irqa, irqb) = self.fusb.get_irq().await;

        if irq & INT_VBUSOK != 0 {
            if self.fusb.vbus_ok().await {
                let cap = self.send_source_cap().await;
                let poke = self.debug_poke().await;
                self.advance_after_tx(cap);
                self.advance_after_tx(poke);
            } else {
                self.disconnect().await;
            }
        }
        if irqa & INTA_HARDRESET != 0 {
            logline!("hard reset");
            self.disconnect().await;
        }
        // TX_SUCCESS / RETRYFAIL are consumed inline by `transmit`; the main
        // loop only handles VBUSOK, target-initiated hard reset, and inbound
        // messages (GCRCSENT) here.
        if irqb & INTB_GCRCSENT != 0 {
            let mut drained = 0;
            while drained < RX_DRAIN_MAX && !self.fusb.rx_fifo_is_empty().await {
                let mut payload = [0u32; 16];
                if let Some((hdr, n)) = self.fusb.get_message(&mut payload).await {
                    self.handle_msg(hdr, &payload[..n]).await;
                } else {
                    break;
                }
                drained += 1;
            }
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
            PdState::DfpVbusOn | PdState::DfpConnected | PdState::Idle | PdState::DfpAccept => {
                // Keep the PD contract + SOP'' debug session alive even after a
                // contract is up. The target can reboot underneath us without
                // the CC line ever dropping, which resets its PD/debug state; a
                // steady source-cap + debug-poke keeps a freshly-rebooted target
                // ready for the next command without a physical re-attach. This
                // runs in the background state machine, decoupled from the
                // command path, so it doesn't disturb an action VDM in flight.
                self.source_cap_timer += 1;
                if self.source_cap_timer > HANDSHAKE_RETRY_TICKS {
                    let cap = self.send_source_cap().await;
                    let poke = self.debug_poke().await;
                    self.advance_after_tx(cap);
                    self.advance_after_tx(poke);
                }
            }
        }

        // Disconnect detection. Hand-wired benches have flaky CC contacts that
        // blip open for tens of ms; tearing down (VBUS off + PD reset) on every
        // blip means the link never stays up long enough to finish a handshake
        // or land a command. So ride out brief losses — only tear down after CC
        // has been gone continuously for DISCONNECT_DEBOUNCE_TICKS. VBUS stays
        // on and the PD contract is preserved throughout the debounce.
        let (cc1, cc2) = self.fusb.get_cc().await;
        if cc1 < 2 && cc2 < 2 {
            self.cc_debounce += 1;
            if self.cc_debounce > DISCONNECT_DEBOUNCE_TICKS {
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
            // `nop` sends nothing; it just confirms the console is alive.
            Command::Nop => {
                logline!("ok nop");
                VENDOR_RESULT.store(RES_OK, Ordering::Relaxed);
            }
            Command::Dfu => {
                self.action("dfu", &[&VDM_DFU_HOLD, &VDM_DFU_HOLD_T2], true)
                    .await
            }
            Command::Reboot => self.action("reboot", &[&VDM_REBOOT], true).await,
            Command::DebugUsb => self.action("debugusb", &[&VDM_DEBUGUSB], true).await,
            Command::Serial => self.enter_serial().await,
            Command::RebootThen(mode) => self.reboot_then(mode).await,
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
                logline!("commands: nop | dfu | reboot | serial | debugusb | reboot serial | reboot debugusb | status | help | bootsel");
            }
            // Reboot into the RP2040 USB bootloader for firmware update. The
            // device drops off the bus and reappears as the RPI-RP2 drive /
            // picoboot interface; no BOOTSEL button needed.
            Command::Bootsel => {
                logline!("ok bootsel; entering USB bootloader");
                // Let the response flush over USB before we reset.
                Timer::after_millis(120).await;
                embassy_rp::rom_data::reset_to_usb_boot(0, 0);
            }
        }
    }

    /// Mux the target's debug UART onto SBU and start bridging it to CDC1.
    async fn enter_serial(&mut self) {
        if !self.connected() {
            logline!("err serial no-target");
            VENDOR_RESULT.store(RES_NOTARGET, Ordering::Relaxed);
            return;
        }
        logline!(">VDM serial");
        for _ in 0..2 {
            self.send_vdm(&VDM_SERIAL_SBU).await;
        }
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
            "serial on SBU polarity CC{}; bridging to CDC1",
            self.cc_line as u8 + 1
        );
        logline!("ok serial");
        VENDOR_RESULT.store(RES_OK, Ordering::Relaxed);
    }

    /// Reboot the target, then arm a follow-up mode to fire once it re-attaches.
    async fn reboot_then(&mut self, mode: PostReboot) {
        if !self.connected() {
            logline!("err reboot no-target");
            return;
        }
        logline!(">VDM reboot");
        for _ in 0..3 {
            self.send_vdm(&VDM_REBOOT).await;
        }
        logline!("ok reboot (sent)");
        self.pending_after_reconnect = Some(mode);
        self.pending_expiry = Instant::now() + Duration::from_secs(REBOOT_RECONNECT_WINDOW_SECS);
        logline!("armed {} for after reconnect", mode.name());
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
        mut common,
        sm0,
        sm1,
        ..
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
