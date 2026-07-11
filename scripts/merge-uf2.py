#!/usr/bin/env python3
"""Merge UF2 files into one: merge-uf2.py IN.uf2 [IN.uf2 ...] OUT.uf2

The RP2040 bootrom reboots once it has seen `numBlocks` blocks, so simply
concatenating UF2 files stops after the first one. This renumbers every
block across all inputs (blockNo / numBlocks) so the bootrom flashes the
whole set — e.g. the dongle bootloader and app — as a single image.
"""
import struct
import sys

UF2_MAGIC0 = 0x0A324655

if len(sys.argv) < 3:
    sys.exit(__doc__)

blocks = []
for path in sys.argv[1:-1]:
    data = open(path, "rb").read()
    if len(data) % 512 != 0:
        sys.exit(f"{path}: not a UF2 (size not a multiple of 512)")
    blocks += [bytearray(data[i : i + 512]) for i in range(0, len(data), 512)]

for i, block in enumerate(blocks):
    if struct.unpack_from("<I", block, 0)[0] != UF2_MAGIC0:
        sys.exit(f"bad UF2 magic in block {i}")
    struct.pack_into("<II", block, 20, i, len(blocks))

out = sys.argv[-1]
open(out, "wb").write(b"".join(blocks))
print(f"{out}: {len(blocks)} blocks from {len(sys.argv) - 2} files")
