//! Resets: into the application again, or into the CH32's ROM USB loader
//! (`1a86:55e0`), which restorekit and WCH's ISP tools flash through.

use ch32_hal::pac;

/// Let the host see the device leave, then reset. `rom` boots the ROM loader.
fn reset(rom: bool) -> ! {
    critical_section::with(|_| {
        pac::AFIO.ctlr().modify(|w| {
            w.set_udp_pue(0);
            w.set_udm_pue(0);
        });
        // QingKe V4C has no mcycle CSR, so a bounded loop: about 100 ms at 48 MHz.
        qingke::riscv::asm::delay(4_800_000);
        if rom {
            for key in [0x4567_0123, 0xcdef_89ab] {
                pac::FLASH.keyr().write(|w| w.set_keyr(key));
            }
            for key in [0x4567_0123, 0xcdef_89ab] {
                pac::FLASH.boot_modekeyp().write(|w| w.set_modekeyr(key));
            }
            pac::FLASH.statr().write(|w| w.set_boot_mode(true));
            pac::FLASH.ctlr().modify(|w| w.set_lock(true));
        }
        pac::RCC.rstsckr().modify(|w| w.set_rmvf(true));
        // WCH's NVIC_SystemReset for the X035: the keyed PFIC CFGR request.
        pac::PFIC.cfgr().write(|w| {
            w.set_keycode(pac::pfic::vals::Keycode::KEY3);
            w.set_resetsys(true);
        });
        loop {
            core::hint::spin_loop();
        }
    })
}

/// Leave the application for the ROM USB loader.
pub fn enter_rom() -> ! {
    reset(true)
}

/// Start over: a stuck or panicked firmware comes back as a fresh device.
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    reset(false)
}
