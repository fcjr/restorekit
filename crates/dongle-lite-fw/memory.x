/* The app runs from the ACTIVE slot behind the embassy-boot bootloader
 * (../dongle-lite-boot) — the full flash layout lives in its memory.x; keep
 * the two in sync. Updates are staged into DFU over the vendor USB interface
 * and swapped in by the bootloader on reboot.
 *
 * RP2354A: 2 MiB internal flash at 0x10000000, 520 KiB SRAM. Unlike RP2040
 * there is no BOOT2 region — the boot ROM launches the bootloader via its
 * IMAGE_DEF, and the bootloader jumps straight to this app's vector table at
 * ORIGIN(FLASH). So FLASH here starts at the ACTIVE partition and this image
 * carries no start/end block of its own (embassy-rp "imagedef-none").
 */
MEMORY {
    FLASH            : ORIGIN = 0x10007000, LENGTH = 768K
    BOOTLOADER_STATE : ORIGIN = 0x10006000, LENGTH = 4K
    DFU              : ORIGIN = 0x100C7000, LENGTH = 772K
    RAM              : ORIGIN = 0x20000000, LENGTH = 512K
}

/* embassy-boot partition symbols, as byte offsets from the start of flash
 * (0x10000000) — the addressing the RP2350 flash driver expects. */
__bootloader_state_start = ORIGIN(BOOTLOADER_STATE) - 0x10000000;
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE) - 0x10000000;

__bootloader_dfu_start = ORIGIN(DFU) - 0x10000000;
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU) - 0x10000000;
