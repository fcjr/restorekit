/* The app runs from the ACTIVE slot behind the embassy-boot bootloader
 * (../dongle-lite-boot) — the full flash layout lives in its memory.x; keep
 * the two in sync. Updates are staged into DFU over the vendor USB interface
 * and swapped in by the bootloader on reboot.
 */
MEMORY {
    BOOT2            : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH            : ORIGIN = 0x10007000, LENGTH = 768K
    BOOTLOADER_STATE : ORIGIN = 0x10006000, LENGTH = 4K
    DFU              : ORIGIN = 0x100C7000, LENGTH = 772K
    RAM              : ORIGIN = 0x20000000, LENGTH = 264K
}

/* embassy-boot partition symbols, as offsets from the start of flash. */
__bootloader_state_start = ORIGIN(BOOTLOADER_STATE) - ORIGIN(BOOT2);
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE) - ORIGIN(BOOT2);

__bootloader_dfu_start = ORIGIN(DFU) - ORIGIN(BOOT2);
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU) - ORIGIN(BOOT2);
