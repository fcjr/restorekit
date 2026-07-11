/* Flash layout shared with the app (dongle-lite-fw/memory.x) — keep in sync.
 *
 *   0x10000000  BOOT2             0x100   ROM second-stage (QSPI setup)
 *   0x10000100  FLASH (this)      24K-256 the embassy-boot bootloader
 *   0x10006000  BOOTLOADER_STATE  4K      swap/boot state
 *   0x10007000  ACTIVE            768K    the running app image
 *   0x100C7000  DFU               772K    staged update (ACTIVE + one sector)
 */
MEMORY {
    BOOT2            : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH            : ORIGIN = 0x10000100, LENGTH = 24K - 0x100
    BOOTLOADER_STATE : ORIGIN = 0x10006000, LENGTH = 4K
    ACTIVE           : ORIGIN = 0x10007000, LENGTH = 768K
    DFU              : ORIGIN = 0x100C7000, LENGTH = 772K
    RAM              : ORIGIN = 0x20000000, LENGTH = 264K
}

/* embassy-boot partition symbols, as offsets from the start of flash. */
__bootloader_state_start = ORIGIN(BOOTLOADER_STATE) - ORIGIN(BOOT2);
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE) - ORIGIN(BOOT2);

__bootloader_active_start = ORIGIN(ACTIVE) - ORIGIN(BOOT2);
__bootloader_active_end = ORIGIN(ACTIVE) + LENGTH(ACTIVE) - ORIGIN(BOOT2);

__bootloader_dfu_start = ORIGIN(DFU) - ORIGIN(BOOT2);
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU) - ORIGIN(BOOT2);
