#!/usr/bin/env node
// Merge UF2 files into one: merge-uf2.mjs [--fill ADDR:LEN] IN.uf2 [IN.uf2 ...] OUT.uf2
//
// The RP2040 bootrom reboots once it has seen `numBlocks` blocks, so simply
// concatenating UF2 files stops after the first one. This renumbers every
// block across all inputs (blockNo / numBlocks) so the bootrom flashes the
// whole set — e.g. the dongle bootloader and app — as a single image.
//
// --fill ADDR:LEN appends 0xFF blocks over [ADDR, ADDR+LEN): the bootrom
// erases each sector it writes, so this wipes flash the inputs don't cover
// (e.g. a bootloader state sector holding stale bytes from older firmware).
//
// Blocks are deduped by target address (first file wins — a later file must
// never overwrite flash an earlier one claimed) and written in ascending
// address order, so each flash sector is erased before its pages.

import { readFileSync, writeFileSync } from "node:fs";

const UF2_MAGIC0 = 0x0a324655;
const UF2_MAGIC1 = 0x9e5d5157;
const UF2_MAGIC_END = 0x0ab16f30;

const die = (msg) => {
  console.error(msg);
  process.exit(1);
};

const args = process.argv.slice(2);
const fills = [];
for (let i; (i = args.indexOf("--fill")) !== -1; ) {
  const [addr, len] = (args[i + 1] ?? "").split(":").map(Number);
  if (!Number.isInteger(addr) || !Number.isInteger(len) || addr % 256 || len % 256) {
    die("--fill ADDR and LEN must be multiples of 256");
  }
  fills.push([addr, len]);
  args.splice(i, 2);
}
if (args.length < 2) {
  die("usage: merge-uf2.mjs [--fill ADDR:LEN] IN.uf2 [IN.uf2 ...] OUT.uf2");
}

const inputs = args.slice(0, -1);
const out = args[args.length - 1];

const blocks = new Map(); // target address -> 512-byte block
for (const path of inputs) {
  const data = readFileSync(path);
  if (data.length % 512 !== 0) {
    die(`${path}: not a UF2 (size not a multiple of 512)`);
  }
  for (let off = 0; off < data.length; off += 512) {
    const block = data.subarray(off, off + 512);
    if (block.readUInt32LE(0) !== UF2_MAGIC0) {
      die(`${path}: bad UF2 magic in block ${off / 512}`);
    }
    const addr = block.readUInt32LE(12);
    if (!blocks.has(addr)) {
      blocks.set(addr, Buffer.from(block));
    }
  }
}
if (blocks.size === 0) {
  die("no input blocks");
}

// 0xFF-fill blocks, cloning flags/family/payload-size from a real block.
const template = blocks.values().next().value;
const flags = template.readUInt32LE(8);
const payloadSize = template.readUInt32LE(16);
const family = template.readUInt32LE(28);
for (const [addr, len] of fills) {
  for (let a = addr; a < addr + len; a += payloadSize) {
    if (blocks.has(a)) continue;
    const block = Buffer.alloc(512);
    block.writeUInt32LE(UF2_MAGIC0, 0);
    block.writeUInt32LE(UF2_MAGIC1, 4);
    block.writeUInt32LE(flags, 8);
    block.writeUInt32LE(a, 12);
    block.writeUInt32LE(payloadSize, 16);
    block.writeUInt32LE(family, 28);
    block.fill(0xff, 32, 32 + payloadSize);
    block.writeUInt32LE(UF2_MAGIC_END, 508);
    blocks.set(a, block);
  }
}

const sorted = [...blocks.keys()].sort((a, b) => a - b).map((a) => blocks.get(a));
sorted.forEach((block, i) => {
  block.writeUInt32LE(i, 20);
  block.writeUInt32LE(sorted.length, 24);
});
writeFileSync(out, Buffer.concat(sorted));
console.log(`${out}: ${sorted.length} blocks from ${inputs.length} files`);
