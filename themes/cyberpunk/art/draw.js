#!/usr/bin/env node
// Draws the Cyberpunk theme's own images, byte for byte the files beside
// theme.json:
//
//   node themes/cyberpunk/art/draw.js themes/cyberpunk
//
// writes `wallpaper.png`, a night city under a perspective grid with HUD
// brackets, glitch bands and scanlines, and `scanlines.png`, the tile the
// theme lays over every surface. Both are drawn here from rectangles, lines
// and one seeded random sequence -- no photograph, font, logo or game asset
// goes into either -- so they are original works under the repository's
// licence, and running this again reproduces them exactly.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');

const out = process.argv[2];
if (!out) {
  console.error('usage: node draw.js <theme folder>');
  process.exit(2);
}

// The theme's own palette (theme.json).
const ink = hex('#06070c'), base = hex('#0b0c12'), horizonGlow = hex('#1a0b1e');
const red = hex('#ff3355'), cyan = hex('#00e5ff'), yellow = hex('#f3e600');
function hex(value) {
  return [1, 3, 5].map(index => parseInt(value.slice(index, index + 2), 16));
}

// xorshift32, so every run draws the same city.
let state = 0x5ee1e239;
function random() {
  state ^= state << 13; state >>>= 0;
  state ^= state >>> 17;
  state ^= state << 5; state >>>= 0;
  return state / 0x100000000;
}
const between = (low, high) => low + random() * (high - low);

// ------------------------------------------------------------- the canvas
// Drawn at twice the size and averaged down, so every edge is antialiased.
const W = 3840, H = 2160, S = 2, CW = W * S, CH = H * S;
const canvas = new Uint8Array(CW * CH * 3);
function blendPixel(index, colour, alpha) {
  canvas[index] += (colour[0] - canvas[index]) * alpha;
  canvas[index + 1] += (colour[1] - canvas[index + 1]) * alpha;
  canvas[index + 2] += (colour[2] - canvas[index + 2]) * alpha;
}
// A rectangle in final pixels.
function rect(x, y, width, height, colour, alpha = 1) {
  const x0 = Math.max(0, Math.round(x * S)), x1 = Math.min(CW, Math.round((x + width) * S));
  const y0 = Math.max(0, Math.round(y * S)), y1 = Math.min(CH, Math.round((y + height) * S));
  for (let row = y0; row < y1; ++row)
    for (let column = x0; column < x1; ++column) blendPixel((row * CW + column) * 3, colour, alpha);
}
// A filled polygon in final pixels, even-odd, sampled at pixel centres.
function polygon(points, colour, alpha = 1) {
  const scaled = points.map(([x, y]) => [x * S, y * S]);
  const top = Math.max(0, Math.floor(Math.min(...scaled.map(point => point[1]))));
  const bottom = Math.min(CH - 1, Math.ceil(Math.max(...scaled.map(point => point[1]))));
  for (let row = top; row <= bottom; ++row) {
    const y = row + 0.5, crossings = [];
    for (let index = 0; index < scaled.length; ++index) {
      const [ax, ay] = scaled[index], [bx, by] = scaled[(index + 1) % scaled.length];
      if ((ay <= y) !== (by <= y)) crossings.push(ax + (y - ay) / (by - ay) * (bx - ax));
    }
    crossings.sort((a, b) => a - b);
    for (let pair = 0; pair + 1 < crossings.length; pair += 2) {
      const from = Math.max(0, Math.ceil(crossings[pair] - 0.5)), to = Math.min(CW - 1, Math.floor(crossings[pair + 1] - 0.5));
      for (let column = from; column <= to; ++column) blendPixel((row * CW + column) * 3, colour, alpha);
    }
  }
}
// A straight line `width` final pixels wide.
function line(ax, ay, bx, by, width, colour, alpha = 1) {
  const length = Math.hypot(bx - ax, by - ay) || 1;
  const nx = -(by - ay) / length * width / 2, ny = (bx - ax) / length * width / 2;
  polygon([[ax + nx, ay + ny], [bx + nx, by + ny], [bx - nx, by - ny], [ax - nx, ay - ny]], colour, alpha);
}

// ------------------------------------------------------------- the scene
const horizon = Math.round(H * 0.63);

// Sky: ink overhead, warming to a dim magenta haze at the horizon.
for (let y = 0; y < horizon; ++y) {
  const t = Math.pow(y / horizon, 2.2);
  const colour = ink.map((channel, index) => Math.round(channel + (horizonGlow[index] - channel) * t));
  rect(0, y, W, 1, colour);
}
// Floor: the ground falls from the haze back to ink toward the viewer.
for (let y = horizon; y < H; ++y) {
  const t = Math.pow((y - horizon) / (H - horizon), 0.6);
  const colour = horizonGlow.map((channel, index) => Math.round(channel + (base[index] - channel) * t));
  rect(0, y, W, 1, colour);
}

// The city: a far row of towers, then a near one, each lit here and there.
function skyline(count, low, high, shade, windowChance) {
  let x = -between(0, 120);
  while (x < W) {
    const width = between(70, 230), height = between(low, high);
    const top = horizon - height;
    rect(x, top, width, height, shade);
    // A stepped crown or a mast on some towers.
    if (random() < 0.35) rect(x + width * 0.2, top - height * 0.08, width * 0.6, height * 0.08, shade);
    if (random() < 0.25) rect(x + width * between(0.3, 0.7), top - between(60, 180), 4, between(60, 180), shade);
    // Windows: a grid of slits, a few of them lit.
    for (let wy = top + 14; wy < horizon - 18; wy += 22)
      for (let wx = x + 10; wx < x + width - 14; wx += 16) {
        if (random() >= windowChance) continue;
        const pick = random();
        const colour = pick < 0.55 ? cyan : pick < 0.85 ? yellow : red;
        rect(wx, wy, 7, 10, colour, between(0.25, 0.7));
      }
    x += width + between(count, count * 3);
  }
}
skyline(4, 180, 640, [10, 11, 19], 0.035);
skyline(10, 120, 460, [14, 15, 25], 0.06);

// Signs: a few tall neon strips hung on the near towers, each with a halo.
const magenta = hex('#ff2bd6');
for (let sign = 0; sign < 9; ++sign) {
  const x = between(W * 0.04, W * 0.96), height = between(120, 300), top = horizon - between(height + 40, 420);
  const colour = [red, cyan, magenta, yellow][sign % 4];
  for (let halo = 18; halo > 0; halo -= 3) rect(x - halo, top - halo, 10 + halo * 2, height + halo * 2, colour, 0.018);
  rect(x, top, 10, height, colour, 0.9);
  for (let notch = top + 16; notch < top + height - 8; notch += 28) rect(x + 2, notch, 6, 10, [6, 7, 12], 0.75);
}

// A hairline of neon where the city meets the ground, and its haze.
for (let step = 0; step < 90; ++step) rect(0, horizon - step, W, 1, red, 0.12 * Math.pow(1 - step / 90, 3));
rect(0, horizon - 1, W, 2, red, 0.85);

// The grid, converging on the vanishing point and fading into the haze.
const vanishX = W * 0.5;
for (let index = 1; index < 26; ++index) {
  const y = horizon + Math.pow(index / 25, 2.1) * (H - horizon);
  line(0, y, W, y, 1.6, cyan, 0.08 + 0.22 * (index / 25));
}
// The receding lines stop short of the horizon, where they would merge.
const reach = 70 / (H - horizon);
for (let index = -30; index <= 30; ++index) {
  const footX = vanishX + index * 260;
  line(vanishX + (footX - vanishX) * reach, horizon + 70, footX, H, 1.6, cyan, 0.2);
}

// HUD brackets framing the skyline, with the theme's trim and glitch.
const box = {left: W * 0.08, top: H * 0.16, right: W * 0.92, bottom: H * 0.86}, arm = 120, weight = 3, cut = 36;
for (const [x, y, dx, dy] of [[box.right, box.top, -1, 1], [box.left, box.bottom, 1, -1], [box.right, box.bottom, -1, -1]]) {
  line(x, y, x + dx * arm, y, weight, red, 0.9);
  line(x, y, x, y + dy * arm, weight, red, 0.9);
}
// The top left bracket is cut as the panels are: the cut lit, a trim along
// the top from it, and past the trim a glitch knocked down off the edge.
line(box.left, box.top + cut, box.left, box.top + arm, weight, red, 0.9);
polygon([[box.left + cut, box.top], [box.left + cut + 18, box.top], [box.left, box.top + cut + 18], [box.left, box.top + cut]], red, 0.95);
rect(box.left + cut + 18, box.top, 260, 9, red, 0.95);
rect(box.left + cut + 18 + 260 + 24, box.top + 9, 80, 9, cyan, 0.95);
// A ruler of ticks along the foot, every fifth one long.
for (let tick = 0; tick <= 80; ++tick) {
  const x = box.left + 160 + tick * ((box.right - box.left - 320) / 80);
  rect(x, box.bottom - (tick % 5 === 0 ? 26 : 14), 2, tick % 5 === 0 ? 26 : 14, cyan, 0.55);
}
// A barcode block beside the top right bracket.
for (let bar = 0, x = box.right - 360; bar < 28; ++bar) {
  const width = random() < 0.5 ? 3 : 7;
  if (random() < 0.8) rect(x, box.top + 18, width, 46, yellow, 0.8);
  x += width + 4;
}
// Four small status squares under it.
for (let square = 0; square < 4; ++square) rect(box.right - 360 + square * 22, box.top + 78, 12, 12, square === 1 ? red : cyan, square === 3 ? 0.35 : 0.85);

// ------------------------------------------------------------- downsample
const image = new Uint8Array(W * H * 3);
for (let y = 0; y < H; ++y)
  for (let x = 0; x < W; ++x)
    for (let channel = 0; channel < 3; ++channel) {
      let total = 0;
      for (let sy = 0; sy < S; ++sy) for (let sx = 0; sx < S; ++sx) total += canvas[((y * S + sy) * CW + x * S + sx) * 3 + channel];
      image[(y * W + x) * 3 + channel] = Math.round(total / (S * S));
    }

// Glitch bands: a few strips knocked sideways, red and blue pulled apart.
for (let band = 0; band < 7; ++band) {
  const top = Math.floor(between(H * 0.08, H * 0.94)), height = Math.floor(between(4, 34));
  const shift = Math.floor(between(18, 110)) * (random() < 0.5 ? -1 : 1), split = Math.floor(between(4, 12));
  for (let y = top; y < Math.min(H, top + height); ++y) {
    const row = image.slice(y * W * 3, (y + 1) * W * 3);
    const at = (x, channel) => row[(((x % W) + W) % W) * 3 + channel];
    for (let x = 0; x < W; ++x) {
      const to = (y * W + x) * 3;
      image[to] = at(x - shift - split, 0);
      image[to + 1] = at(x - shift, 1);
      image[to + 2] = at(x - shift + split, 2);
    }
  }
}
// Scanlines: every third row a little darker.
for (let y = 0; y < H; y += 3)
  for (let index = y * W * 3; index < (y + 1) * W * 3; ++index) image[index] = Math.round(image[index] * 0.82);

// ------------------------------------------------------------- PNG
function crc32(buffer) {
  let crc = ~0;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; ++bit) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return ~crc >>> 0;
}
function chunk(type, body) {
  const length = Buffer.alloc(4), crc = Buffer.alloc(4), named = Buffer.concat([Buffer.from(type, 'latin1'), body]);
  length.writeUInt32BE(body.length);
  crc.writeUInt32BE(crc32(named));
  return Buffer.concat([length, named, crc]);
}
// Each row takes whichever of PNG's filters leaves the smallest residue.
function png(pixels, width, height, channels) {
  const stride = width * channels, rows = [];
  let previous = new Uint8Array(stride);
  for (let y = 0; y < height; ++y) {
    const line = pixels.subarray(y * stride, (y + 1) * stride);
    let best = null, bestCost = Infinity;
    for (let filter = 0; filter < 5; ++filter) {
      const filtered = Buffer.alloc(stride + 1);
      filtered[0] = filter;
      let cost = 0;
      for (let x = 0; x < stride; ++x) {
        const left = x >= channels ? line[x - channels] : 0, up = previous[x], corner = x >= channels ? previous[x - channels] : 0;
        let predictor = 0;
        if (filter === 1) predictor = left;
        else if (filter === 2) predictor = up;
        else if (filter === 3) predictor = (left + up) >> 1;
        else if (filter === 4) {
          const estimate = left + up - corner, a = Math.abs(estimate - left), b = Math.abs(estimate - up), c = Math.abs(estimate - corner);
          predictor = a <= b && a <= c ? left : b <= c ? up : corner;
        }
        const value = (line[x] - predictor) & 0xff;
        filtered[x + 1] = value;
        cost += value < 128 ? value : 256 - value;
      }
      if (cost < bestCost) { best = filtered; bestCost = cost; }
    }
    rows.push(best);
    previous = line;
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8; header[9] = channels === 4 ? 6 : 2;
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', header),
    chunk('IDAT', zlib.deflateSync(Buffer.concat(rows), {level: 9})), chunk('IEND', Buffer.alloc(0))]);
}

fs.writeFileSync(path.join(out, 'wallpaper.png'), png(image, W, H, 3));
// The surface tile: one lit row in three, white, laid at `grainOpacity`.
const tile = new Uint8Array(4 * 3 * 4);
for (let x = 0; x < 4; ++x) tile.set([255, 255, 255, 255], x * 4);
fs.writeFileSync(path.join(out, 'scanlines.png'), png(tile, 4, 3, 4));
console.log(`Drew ${path.join(out, 'wallpaper.png')} and ${path.join(out, 'scanlines.png')}`);
