#!/usr/bin/env python3
"""Merge UF2 files into one: merge-uf2.py [--fill ADDR:LEN] IN.uf2 [IN.uf2 ...] OUT.uf2

The RP2040 bootrom reboots once it has seen `numBlocks` blocks, so simply
concatenating UF2 files stops after the first one. This renumbers every
block across all inputs (blockNo / numBlocks) so the bootrom flashes the
whole set — e.g. the dongle bootloader and app — as a single image.

--fill ADDR:LEN appends 0xFF blocks over [ADDR, ADDR+LEN): the bootrom
erases each sector it writes, so this wipes flash the inputs don't cover
(e.g. a bootloader state sector holding stale bytes from older firmware).
"""
import struct
import sys

UF2_MAGIC0 = 0x0A324655

args = sys.argv[1:]
fills = []
while "--fill" in args:
    i = args.index("--fill")
    addr, length = (int(x, 0) for x in args[i + 1].split(":"))
    if addr % 256 or length % 256:
        sys.exit("--fill ADDR and LEN must be multiples of 256")
    fills.append((addr, length))
    del args[i : i + 2]

if len(args) < 2:
    sys.exit(__doc__)
sys.argv = [sys.argv[0]] + args

blocks = {}
for path in sys.argv[1:-1]:
    data = open(path, "rb").read()
    if len(data) % 512 != 0:
        sys.exit(f"{path}: not a UF2 (size not a multiple of 512)")
    for i in range(0, len(data), 512):
        block = bytearray(data[i : i + 512])
        if struct.unpack_from("<I", block, 0)[0] != UF2_MAGIC0:
            sys.exit(f"{path}: bad UF2 magic in block {i // 512}")
        # Dedupe by target address, first file wins — a later file must never
        # overwrite flash a earlier one claimed (e.g. app padding over the
        # bootloader's sector).
        addr = struct.unpack_from("<I", block, 12)[0]
        blocks.setdefault(addr, block)

if not blocks:
    sys.exit("no input blocks")

# 0xFF-fill blocks, cloning flags/family/payload-size from a real block.
template = next(iter(blocks.values()))
flags, _, payload_size = struct.unpack_from("<III", template, 8)
family = struct.unpack_from("<I", template, 28)[0]
for addr, length in fills:
    for a in range(addr, addr + length, payload_size):
        block = bytearray(512)
        struct.pack_into(
            "<8I", block, 0, UF2_MAGIC0, 0x9E5D5157, flags, a, payload_size, 0, 0, family
        )
        block[32 : 32 + payload_size] = b"\xff" * payload_size
        block[508:512] = struct.pack("<I", 0x0AB16F30)
        blocks.setdefault(a, block)

# Ascending address order, so each flash sector is erased before its pages.
blocks = [blocks[a] for a in sorted(blocks)]
for i, block in enumerate(blocks):
    struct.pack_into("<II", block, 20, i, len(blocks))

out = sys.argv[-1]
open(out, "wb").write(b"".join(blocks))
print(f"{out}: {len(blocks)} blocks from {len(sys.argv) - 2} files")
