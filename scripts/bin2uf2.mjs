#!/usr/bin/env node
// Convert a raw binary to UF2: bin2uf2.mjs --base ADDR --family NAME IN.bin OUT.uf2
//
// Replaces `picotool uf2 convert -t bin`, which we can't rely on in CI:
// picotool ships no prebuilt binaries and isn't packaged before Ubuntu 25.04.
// Verified byte-identical to `picotool uf2 convert` for both images we ship
// (bootloader at 0x10000000, app at 0x10007000).
//
// elf2uf2-rs isn't an option either — it only knows the RP2040 family id, and
// the RP2350 bootrom ignores blocks stamped with the wrong one.

import { readFileSync, writeFileSync } from "node:fs";

const UF2_MAGIC0 = 0x0a324655;
const UF2_MAGIC1 = 0x9e5d5157;
const UF2_MAGIC_END = 0x0ab16f30;
const UF2_FLAG_FAMILY_ID = 0x2000;
const PAYLOAD = 256;

// picotool's family names (see its uf2.h). Only the ones we target.
const FAMILIES = {
  rp2040: 0xe48bff56,
  absolute: 0xe48bff57,
  data: 0xe48bff58,
  "rp2350-arm-s": 0xe48bff59,
  "rp2350-riscv": 0xe48bff5a,
  "rp2350-arm-ns": 0xe48bff5b,
};

const die = (msg) => {
  console.error(msg);
  process.exit(1);
};

const args = process.argv.slice(2);
const take = (flag) => {
  const i = args.indexOf(flag);
  if (i === -1) return undefined;
  const [value] = args.splice(i, 2).slice(1);
  return value;
};

const baseArg = take("--base");
const familyArg = take("--family");
if (args.length !== 2) {
  die("usage: bin2uf2.mjs --base ADDR --family NAME IN.bin OUT.uf2");
}
const [input, out] = args;

const base = Number(baseArg);
if (!Number.isInteger(base) || base < 0) {
  die(`--base must be an address (got '${baseArg}')`);
}
const family = FAMILIES[familyArg];
if (family === undefined) {
  die(`--family must be one of ${Object.keys(FAMILIES).join(", ")} (got '${familyArg}')`);
}

const data = readFileSync(input);
if (data.length === 0) {
  die(`${input}: empty`);
}

// A short final chunk is zero-padded to a full 256-byte payload rather than
// given a smaller payloadSize: that's what picotool emits, and the bootrom
// writes whole flash pages anyway.
const numBlocks = Math.ceil(data.length / PAYLOAD);
const uf2 = Buffer.alloc(numBlocks * 512);
for (let i = 0; i < numBlocks; i++) {
  const block = uf2.subarray(i * 512, (i + 1) * 512);
  block.writeUInt32LE(UF2_MAGIC0, 0);
  block.writeUInt32LE(UF2_MAGIC1, 4);
  block.writeUInt32LE(UF2_FLAG_FAMILY_ID, 8);
  block.writeUInt32LE(base + i * PAYLOAD, 12);
  block.writeUInt32LE(PAYLOAD, 16);
  block.writeUInt32LE(i, 20);
  block.writeUInt32LE(numBlocks, 24);
  block.writeUInt32LE(family, 28);
  data.copy(block, 32, i * PAYLOAD, Math.min((i + 1) * PAYLOAD, data.length));
  block.writeUInt32LE(UF2_MAGIC_END, 508);
}
writeFileSync(out, uf2);
console.log(`${out}: ${numBlocks} blocks from ${input}`);
