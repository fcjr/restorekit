//! USB device: a CDC console, optionally a second CDC port and a vendor
//! interface, on ch32-hal's USBFS driver and embassy-usb.
//!
//! The engine never waits on USB. Each CDC port is a pair of pipes that
//! [`run`] pumps: [`Port::try_write`] drops output while no terminal has the
//! port open or its pipe is full, and [`Port::try_read`] returns what has come
//! in. Opening the console at 1200 baud and closing it enters the ROM USB
//! loader, as the chip's own USB ISP tools expect.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use ch32_hal::usb::EndpointDataBuffer;
use ch32_hal::usbfs::{self, Driver};
use ch32_hal::{Peri, bind_interrupts, peripherals};
use embassy_futures::join::{join, join3};
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::pipe::Pipe;
use embassy_time::Timer;
use embassy_usb::class::cdc_acm::device::{
    CdcAcmClass, Config as CdcConfig, Receiver, Sender, State,
};
use embassy_usb::msos;
use embassy_usb::{Builder, Config, Handler};
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    USBFS => usbfs::InterruptHandler<peripherals::USBFS>;
});

/// EP0 plus the two CDC ports' three endpoints each. EP4 shares EP0's buffer
/// on the X035, which leaves exactly EP1-3 and EP5-7.
const NR_EP: usize = 7;
type UsbDriver = Driver<'static, peripherals::USBFS, NR_EP, 64>;

/// `bMS_VendorCode` for the MS OS 2.0 descriptor set.
const MSOS_VENDOR_CODE: u8 = 0x17;

struct Shared {
    rx: Pipe<CriticalSectionRawMutex, 128>,
    tx: Pipe<CriticalSectionRawMutex, 256>,
    dtr: AtomicBool,
    baud: AtomicU32,
}
impl Shared {
    const fn new() -> Self {
        Self {
            rx: Pipe::new(),
            tx: Pipe::new(),
            dtr: AtomicBool::new(false),
            baud: AtomicU32::new(0),
        }
    }
}
static PORTS: [Shared; 2] = [Shared::new(), Shared::new()];

/// Interface number of the vendor interface, once [`run`] has built it.
static VENDOR_IFACE: AtomicU8 = AtomicU8::new(0xff);

/// The vendor interface's number, for a handler checking `wIndex`.
#[allow(dead_code)]
pub fn vendor_interface() -> u8 {
    VENDOR_IFACE.load(Ordering::Relaxed)
}

/// `DP2-` and the second 32-bit word of the chip UID. Twelve characters, like
/// the Lite's: macOS names the CDC ttys after a serial that short (not after
/// 16 characters), and the CLI finds the ports by that name.
pub fn serial_number() -> &'static str {
    static mut SERIAL: [u8; 12] = [0; 12];
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let uid = ch32_hal::signature::unique_id();
    // SAFETY: written once, before USB starts, and only read after.
    let serial = unsafe { &mut *core::ptr::addr_of_mut!(SERIAL) };
    serial[..4].copy_from_slice(restorekit_dongle_proto::SERIAL_PREFIX_PRO_V2.as_bytes());
    for (i, byte) in uid[4..8].iter().enumerate() {
        serial[4 + i * 2] = HEX[usize::from(byte >> 4)];
        serial[5 + i * 2] = HEX[usize::from(byte & 15)];
    }
    core::str::from_utf8(serial).unwrap()
}

/// One CDC port, as the engine sees it.
pub struct Port(usize);
impl Port {
    pub fn console() -> Self {
        Self(0)
    }
    #[allow(dead_code)]
    pub fn second() -> Self {
        Self(1)
    }
    fn shared(&self) -> &'static Shared {
        &PORTS[self.0]
    }
    /// Whether a host terminal has the port open (DTR).
    pub fn connected(&self) -> bool {
        self.shared().dtr.load(Ordering::Relaxed)
    }
    /// The baud rate the host last set.
    #[allow(dead_code)]
    pub fn baud(&self) -> u32 {
        self.shared().baud.load(Ordering::Relaxed)
    }
    /// Bytes the host sent, if any.
    pub fn try_read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        self.shared().rx.try_read(buf).map_err(|_| ())
    }
    /// Queue output. Fails while no terminal is open or the pipe is full.
    pub fn try_write(&mut self, bytes: &[u8]) -> Result<usize, ()> {
        if !self.connected() {
            return Err(());
        }
        self.shared().tx.try_write(bytes).map_err(|_| ())
    }
}

pub struct Options {
    pub product: &'static str,
    pub serial: &'static str,
    pub vid: u16,
    pub pid: u16,
    /// Offer a second CDC port.
    pub second_port: bool,
    /// A vendor interface (class 0xFF, control only, WinUSB on Windows) and
    /// its request handler.
    pub vendor: Option<&'static mut dyn Handler>,
}

/// Build the device and run it, with its port pumps. Never returns.
pub async fn run(
    usb: Peri<'static, peripherals::USBFS>,
    dp: Peri<'static, peripherals::PC17>,
    dm: Peri<'static, peripherals::PC16>,
    opts: Options,
) {
    static EP_BUFFERS: StaticCell<[EndpointDataBuffer<64>; NR_EP]> = StaticCell::new();
    static mut CONFIG_DESC: [u8; 256] = [0; 256];
    static mut BOS_DESC: [u8; 64] = [0; 64];
    static mut MSOS_DESC: [u8; 192] = [0; 192];
    static mut CONTROL_BUF: [u8; 64] = [0; 64];
    static mut STATES: [State; 2] = [State::new(), State::new()];
    let _ = Irqs;
    // SAFETY: run is called once; these are handed to the builder for good.
    let ep_buffers = EP_BUFFERS.init(core::array::from_fn(|_| EndpointDataBuffer::default()));
    let (config_desc, bos_desc, msos_desc, control_buf, states) = unsafe {
        (
            &mut *core::ptr::addr_of_mut!(CONFIG_DESC),
            &mut *core::ptr::addr_of_mut!(BOS_DESC),
            &mut *core::ptr::addr_of_mut!(MSOS_DESC),
            &mut *core::ptr::addr_of_mut!(CONTROL_BUF),
            &mut *core::ptr::addr_of_mut!(STATES),
        )
    };
    let [state0, state1] = states;

    let driver: UsbDriver = Driver::new(usb, dp, dm, ep_buffers);
    let mut config = Config::new(opts.vid, opts.pid);
    config.manufacturer = Some(restorekit_dongle_proto::MANUFACTURER);
    config.product = Some(opts.product);
    config.serial_number = Some(opts.serial);
    // The MCU sits behind the board's bus-powered CH334P: the host budgets
    // 100 mA per downstream port and refuses a configuration asking for more.
    config.max_power = 100;
    config.max_packet_size_0 = 64;
    config.device_release = u16::from_str_radix(env!("CARGO_PKG_VERSION_MAJOR"), 10).unwrap_or(0)
        << 8
        | u16::from_str_radix(env!("CARGO_PKG_VERSION_MINOR"), 10).unwrap_or(0) << 4
        | u16::from_str_radix(env!("CARGO_PKG_VERSION_PATCH"), 10).unwrap_or(0);
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;

    let mut builder = Builder::new(
        driver,
        config,
        config_desc,
        bos_desc,
        msos_desc,
        control_buf,
    );
    let has_vendor = opts.vendor.is_some();
    if has_vendor {
        builder.msos_descriptor(msos::windows_version::WIN8_1, MSOS_VENDOR_CODE);
    }
    let console = CdcAcmClass::new(&mut builder, state0, CdcConfig::new());
    let second = opts
        .second_port
        .then(|| CdcAcmClass::new(&mut builder, state1, CdcConfig::new()));
    if let Some(handler) = opts.vendor {
        let mut func = builder.function(0xFF, 0x00, 0x00);
        func.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
        let mut iface = func.interface();
        VENDOR_IFACE.store(iface.interface_number().0, Ordering::Relaxed);
        let _ = iface.alt_setting(0xFF, 0x00, 0x00, None);
        drop(func);
        builder.handler(handler);
    }
    let mut device = builder.build();

    let (tx0, rx0) = console.split();
    let pumps = async {
        match second {
            Some(second) => {
                let (tx1, rx1) = second.split();
                join(pump(0, tx0, rx0), pump(1, tx1, rx1)).await;
            }
            None => pump(0, tx0, rx0).await,
        }
    };
    join3(device.run(), pumps, rom_on_1200()).await;
}

/// Move bytes between a CDC port and its pipes, and track DTR and baud.
async fn pump(n: usize, mut tx: Sender<'static, UsbDriver>, mut rx: Receiver<'static, UsbDriver>) {
    let shared = &PORTS[n];
    let incoming = async {
        let mut buf = [0u8; 64];
        loop {
            rx.wait_connection().await;
            if let Ok(len) = rx.read_packet(&mut buf).await {
                // Dropped if nobody reads: the host never blocks on us.
                let _ = shared.rx.try_write(&buf[..len]);
            }
        }
    };
    let outgoing = async {
        let mut buf = [0u8; 64];
        loop {
            tx.wait_connection().await;
            match select(shared.tx.read(&mut buf), Timer::after_millis(10)).await {
                Either::First(len) => {
                    if tx.dtr() && tx.write_packet(&buf[..len]).await.is_ok() && len == buf.len() {
                        let _ = tx.write_packet(&[]).await;
                    }
                }
                Either::Second(()) => {}
            }
            shared.dtr.store(tx.dtr(), Ordering::Relaxed);
            shared
                .baud
                .store(tx.line_coding().data_rate(), Ordering::Relaxed);
        }
    };
    join(incoming, outgoing).await;
}

/// A host opening the console at 1200 baud and closing it asks for the ROM
/// USB loader.
async fn rom_on_1200() {
    let mut armed = false;
    loop {
        Timer::after_millis(20).await;
        let console = &PORTS[0];
        let at_1200 = console.baud.load(Ordering::Relaxed) == 1200;
        if at_1200 && console.dtr.load(Ordering::Relaxed) {
            armed = true;
        } else if armed && at_1200 {
            crate::rom::enter_rom();
        } else {
            armed = false;
        }
    }
}
