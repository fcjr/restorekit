/* Flash layout shared with the app (dongle-lite-fw/memory.x) — keep in sync.
 *
 *   0x10000000  FLASH (this)      24K     the embassy-boot bootloader (+ IMAGE_DEF)
 *   0x10006000  BOOTLOADER_STATE  4K      swap/boot state
 *   0x10007000  ACTIVE            768K    the running app image
 *   0x100C7000  DFU               772K    staged update (ACTIVE + one sector)
 *
 * RP2354A: 2 MiB internal flash, 520 KiB SRAM. Unlike RP2040 there is no BOOT2
 * region: the boot ROM finds this bootloader's IMAGE_DEF (the .start_block
 * below, kept in the first 4K of flash) and launches it directly.
 */
MEMORY {
    FLASH            : ORIGIN = 0x10000000, LENGTH = 24K
    BOOTLOADER_STATE : ORIGIN = 0x10006000, LENGTH = 4K
    ACTIVE           : ORIGIN = 0x10007000, LENGTH = 768K
    DFU              : ORIGIN = 0x100C7000, LENGTH = 772K
    RAM              : ORIGIN = 0x20000000, LENGTH = 512K
}

/* embassy-boot partition symbols, as byte offsets from the start of flash. */
__bootloader_state_start = ORIGIN(BOOTLOADER_STATE) - 0x10000000;
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE) - 0x10000000;

__bootloader_active_start = ORIGIN(ACTIVE) - 0x10000000;
__bootloader_active_end = ORIGIN(ACTIVE) + LENGTH(ACTIVE) - 0x10000000;

__bootloader_dfu_start = ORIGIN(DFU) - 0x10000000;
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU) - 0x10000000;

/* --- RP2350 boot ROM blocks (from the embassy rp235x example). ------------
 * The boot ROM walks a linked list of blocks; the IMAGE_DEF that tells it how
 * to boot this image lives in .start_block, which must sit in the first 4K of
 * flash (right after the vector table). embassy-rp emits the IMAGE_DEF itself
 * (default "secure exe") into .start_block; these sections place it and move
 * .text to start after it.
 */
SECTIONS {
    .start_block : ALIGN(4)
    {
        __start_block_addr = .;
        KEEP(*(.start_block));
        KEEP(*(.boot_info));
    } > FLASH
} INSERT AFTER .vector_table;

_stext = ADDR(.start_block) + SIZEOF(.start_block);

SECTIONS {
    .bi_entries : ALIGN(4)
    {
        __bi_entries_start = .;
        KEEP(*(.bi_entries));
        . = ALIGN(4);
        __bi_entries_end = .;
    } > FLASH
} INSERT AFTER .text;

SECTIONS {
    .end_block : ALIGN(4)
    {
        __end_block_addr = .;
        KEEP(*(.end_block));
    } > FLASH
} INSERT AFTER .uninit;

PROVIDE(start_to_end = __end_block_addr - __start_block_addr);
PROVIDE(end_to_start = __start_block_addr - __end_block_addr);
