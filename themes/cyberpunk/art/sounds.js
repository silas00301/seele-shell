#!/usr/bin/env node
// Synthesises the Cyberpunk theme's interface sounds, byte for byte the files
// in `sounds/` beside theme.json:
//
//   node themes/cyberpunk/art/sounds.js themes/cyberpunk
//
// Every cue is built here from oscillators, an envelope, a bit crusher and one
// seeded noise sequence -- no recording, sample or game audio goes into any of
// them -- so they are original works under the repository's licence, and
// running this again reproduces them exactly. Each is a short mono 16-bit WAV
// at 48 kHz, PipeWire's own rate, the one format Qt's SoundEffect decodes, and
// each is mixed well below full scale: a cue is there to be noticed, not to
// startle.
'use strict';
const fs = require('node:fs');
const path = require('node:path');

const out = process.argv[2];
if (!out) {
  console.error('usage: node sounds.js <theme folder>');
  process.exit(2);
}

const RATE = 48000;
const seconds = value => Math.round(value * RATE);
// Note frequencies, equal temperament from A4.
const note = semitones => 440 * Math.pow(2, semitones / 12);
const A5 = note(12), CS6 = note(16), E6 = note(19), A6 = note(24), B6 = note(26), E7 = note(31);
const CS5 = note(4), FS5 = note(9);

// xorshift32, so every run makes the same noise.
let state = 0x5ee1e240;
function random() {
  state ^= state << 13; state >>>= 0;
  state ^= state >>> 17;
  state ^= state << 5; state >>>= 0;
  return state / 0x100000000 * 2 - 1;
}

// One waveform sample at `phase` turns. A square or a saw is summed from its
// first few harmonics rather than drawn as a hard edge, so it buzzes without
// aliasing into a hiss at these pitches.
function wave(shape, phase) {
  const turn = 2 * Math.PI * phase;
  if (shape === 'sine') return Math.sin(turn);
  let sum = 0;
  if (shape === 'square') for (let k = 1; k <= 9; k += 2) sum += Math.sin(k * turn) / k;
  else for (let k = 1; k <= 8; ++k) sum += Math.sin(k * turn) / k * (k % 2 ? 1 : -1);
  return sum * (shape === 'square' ? 4 / Math.PI : 2 / Math.PI);
}

// A voice: a pitch gliding exponentially from `from` to `to` Hz over its
// length, a 2 ms attack and an exponential decay to silence.
function voice(buffer, {at, length, from, to = from, shape = 'square', gain = 1, decay = 4}) {
  const start = seconds(at), count = seconds(length), attack = seconds(0.002);
  let phase = 0;
  for (let i = 0; i < count && start + i < buffer.length; ++i) {
    const t = i / count;
    phase += from * Math.pow(to / from, t) / RATE;
    const envelope = Math.min(1, i / attack) * Math.exp(-decay * t) * (1 - t);
    buffer[start + i] += wave(shape, phase) * envelope * gain;
  }
}
// A burst of the seeded noise, as the static a screen makes switching on.
function crackle(buffer, {at, length, gain = 1}) {
  const start = seconds(at), count = seconds(length);
  for (let i = 0; i < count && start + i < buffer.length; ++i)
    buffer[start + i] += random() * Math.pow(1 - i / count, 2) * gain;
}
// Digital grit: held for `hold` samples and quantised to `bits`.
function crush(buffer, bits, hold) {
  const levels = Math.pow(2, bits - 1);
  for (let i = 0; i < buffer.length; ++i) {
    const held = buffer[i - (i % hold)];
    buffer[i] = Math.round(held * levels) / levels;
  }
}
// Scaled so the loudest sample sits at `peak` dBFS, with a 3 ms fade at the
// end so a cue cut short never clicks.
function master(buffer, peak) {
  const loudest = buffer.reduce((most, sample) => Math.max(most, Math.abs(sample)), 0) || 1;
  const scale = Math.pow(10, peak / 20) / loudest, fade = seconds(0.003);
  return buffer.map((sample, i) => sample * scale * Math.min(1, (buffer.length - 1 - i) / fade));
}

function wav(samples) {
  const data = Buffer.alloc(samples.length * 2);
  samples.forEach((sample, i) => data.writeInt16LE(Math.max(-32768, Math.min(32767, Math.round(sample * 32767))), i * 2));
  const header = Buffer.alloc(44);
  header.write('RIFF', 0);
  header.writeUInt32LE(36 + data.length, 4);
  header.write('WAVE', 8);
  header.write('fmt ', 12);
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20);          // PCM
  header.writeUInt16LE(1, 22);          // mono
  header.writeUInt32LE(RATE, 24);
  header.writeUInt32LE(RATE * 2, 28);   // bytes a second
  header.writeUInt16LE(2, 32);          // bytes a frame
  header.writeUInt16LE(16, 34);         // bits a sample
  header.write('data', 36);
  header.writeUInt32LE(data.length, 40);
  return Buffer.concat([header, data]);
}

function cue(length, build, peak) {
  const buffer = new Float64Array(seconds(length));
  build(buffer);
  return wav(master(buffer, peak));
}

const cues = {
  // Two rising blips, a terminal acknowledging a message.
  'notification.wav': cue(0.2, buffer => {
    voice(buffer, {at: 0, length: 0.07, from: E6, decay: 3});
    voice(buffer, {at: 0.085, length: 0.11, from: B6, decay: 4});
    voice(buffer, {at: 0.085, length: 0.11, from: B6 * 1.005, shape: 'sine', gain: 0.4, decay: 4});
    crush(buffer, 6, 3);
  }, -16),
  // Three hard pulses falling a fourth and back: something needs you now.
  'notification-urgent.wav': cue(0.42, buffer => {
    [FS5, CS5, FS5].forEach((pitch, index) => {
      voice(buffer, {at: index * 0.13, length: 0.11, from: pitch, shape: 'saw', decay: 2});
      voice(buffer, {at: index * 0.13, length: 0.11, from: pitch / 2, gain: 0.5, decay: 2});
    });
    crush(buffer, 5, 4);
  }, -13),
  // A rising arpeggio that settles on the octave: the timer has run out.
  'focus-done.wav': cue(0.64, buffer => {
    [A5, CS6, E6].forEach((pitch, index) => {
      voice(buffer, {at: index * 0.09, length: 0.2, from: pitch, shape: 'sine', decay: 5});
      voice(buffer, {at: index * 0.09, length: 0.2, from: pitch, gain: 0.25, decay: 6});
    });
    voice(buffer, {at: 0.27, length: 0.37, from: A6, shape: 'sine', decay: 3});
    voice(buffer, {at: 0.27, length: 0.37, from: A6 * 1.004, gain: 0.2, decay: 4});
    crush(buffer, 8, 2);
  }, -15),
  // Static, then a sweep up: a panel switching on.
  'panel-open.wav': cue(0.11, buffer => {
    crackle(buffer, {at: 0, length: 0.012, gain: 0.6});
    voice(buffer, {at: 0.006, length: 0.1, from: 520, to: 1760, decay: 2});
    crush(buffer, 5, 3);
  }, -20),
  // A sweep down: a panel switching off.
  'panel-close.wav': cue(0.1, buffer => {
    voice(buffer, {at: 0, length: 0.09, from: 1500, to: 420, decay: 2});
    crackle(buffer, {at: 0.082, length: 0.012, gain: 0.3});
    crush(buffer, 5, 3);
  }, -21),
  // One short tick, quiet enough to repeat under a held key.
  'level-step.wav': cue(0.035, buffer => {
    voice(buffer, {at: 0, length: 0.03, from: E7, shape: 'sine', decay: 6});
    crush(buffer, 7, 2);
  }, -22),
};

const folder = path.join(out, 'sounds');
fs.mkdirSync(folder, {recursive: true});
for (const [name, bytes] of Object.entries(cues)) fs.writeFileSync(path.join(folder, name), bytes);
console.log(`Synthesised ${Object.keys(cues).map(name => path.join(folder, name)).join(', ')}`);
