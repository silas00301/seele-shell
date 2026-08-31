#!/usr/bin/env node
// Generates the shell's grain tile: a seeded monochrome noise PNG that every
// textured surface repeats at low opacity. Generating it at build time keeps
// the repository free of binary assets while the seed keeps the tile
// reproducible across rebuilds.
"use strict";

const zlib = require("node:zlib");
const fs = require("node:fs");

const SIZE = 128;
const SEED = 0x5ee1e0;

// xorshift32. A linear congruential generator correlates neighbouring pixels
// strongly enough that the repeated tile shows a visible weave.
let state = SEED;
function next() {
  state ^= state << 13;
  state >>>= 0;
  state ^= state >> 17;
  state ^= state << 5;
  state >>>= 0;
  return state;
}

function chunk(type, body) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(body.length, 0);
  const payload = Buffer.concat([Buffer.from(type, "ascii"), body]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(payload), 0);
  return Buffer.concat([length, payload, crc]);
}

const crcTable = (() => {
  const table = new Int32Array(256);
  for (let i = 0; i < 256; i++) {
    let value = i;
    for (let bit = 0; bit < 8; bit++) value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
    table[i] = value;
  }
  return table;
})();

function crc32(buffer) {
  let crc = -1;
  for (let i = 0; i < buffer.length; i++) crc = crcTable[(crc ^ buffer[i]) & 0xff] ^ (crc >>> 8);
  return (crc ^ -1) >>> 0;
}

// Greyscale, 8 bits per pixel, one filter byte per scanline.
const raw = Buffer.alloc(SIZE * (SIZE + 1));
for (let y = 0; y < SIZE; y++) {
  const row = y * (SIZE + 1);
  raw[row] = 0;
  for (let x = 0; x < SIZE; x++) raw[row + 1 + x] = next() & 0xff;
}

const header = Buffer.alloc(13);
header.writeUInt32BE(SIZE, 0);
header.writeUInt32BE(SIZE, 4);
header[8] = 8; // bit depth
header[9] = 0; // colour type: greyscale
header[10] = 0; // compression
header[11] = 0; // filter
header[12] = 0; // interlace

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", header),
  chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0))
]);

const target = process.argv[2];
if (!target) {
  console.error("usage: grain.js <output.png>");
  process.exit(2);
}
fs.writeFileSync(target, png);
