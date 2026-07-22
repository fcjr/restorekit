//! RecoverKit dongle bootloader: embassy-boot on RP2354A (RP2350).
//!
//! Runs before the app (`dongle-lite-fw`). If the app staged an update in the
//! DFU slot and marked it, this swaps it into ACTIVE (power-fail-safe: a swap
//! interrupted by power loss resumes on the next boot) and boots it; if the
//! new firmware never calls `mark_booted`, the next boot reverts the swap.
//! No USB, no mass storage — the host only ever sees the app.

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m_rt::entry;
use embassy_boot_rp::{BootLoader, BootLoaderConfig, WatchdogFlash};
use embassy_rp::gpio::{Level, Output};
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::Duration;

const FLASH_SIZE: usize = 2 * 1024 * 1024;

#[entry]
fn main() -> ! {
    let p = embassy_rp::init(Default::default());

    // Probe-less boot beacon: a deliberate ~300 ms LED pulse, off again just
    // before handing over to the app (which lights it anew). Never lights =
    // never got here; solid on forever = hung/crashed below; a single pulse
    // then dark = the app never came up; fast strobe = panic-reset loop.
    let mut led = Output::new(p.PIN_25, Level::High);
    cortex_m::asm::delay(45_000_000); // ~300 ms at 150 MHz (RP2350 default)

    // The watchdog-fed flash keeps a wedged swap from bricking the board: if
    // the bootloader hangs mid-copy, the watchdog resets and the swap resumes.
    let flash = WatchdogFlash::<FLASH_SIZE>::start(p.FLASH, p.WATCHDOG, Duration::from_secs(8));
    let flash = Mutex::new(RefCell::new(flash));

    let config = BootLoaderConfig::from_linkerfile_blocking(&flash, &flash, &flash);
    let active_offset = config.active.offset();
    // Buffer size = one erase sector, the unit the swap copies at a time.
    let bl: BootLoader<4096> = BootLoader::prepare(config);

    led.set_low();
    unsafe { bl.load(embassy_rp::flash::FLASH_BASE as u32 + active_offset) }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // A bootloader panic has no console to report to; reset and retry. With
    // the beacon above this shows as a fast LED strobe.
    cortex_m::peripheral::SCB::sys_reset()
}
