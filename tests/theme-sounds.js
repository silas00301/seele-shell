#!/usr/bin/env node
// The Cyberpunk theme's interface sounds (SIL-240) as files: every sound slot
// its theme.json names is a file `art/sounds.js` writes again byte for byte,
// and each is what Qt's SoundEffect plays -- an uncompressed PCM WAV, 16-bit,
// mono, at PipeWire's 48 kHz -- short, mixed well below full scale, and ending
// on silence so a cue cut short never clicks. ThemeSounds' gating is
// tests/theme-sounds.sh; registration refuses a cue that is not a WAV.
//
//   node tests/theme-sounds.js <themes/cyberpunk>
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawnSync} = require('node:child_process');

const folder = path.resolve(process.argv[2] || path.join(__dirname, '../themes/cyberpunk'));
const theme = JSON.parse(fs.readFileSync(path.join(folder, 'theme.json'), 'utf8'));
const cues = ['soundNotification', 'soundNotificationUrgent', 'soundFocusDone', 'soundPanelOpen', 'soundPanelClose', 'soundLevelStep'];
for (const cue of cues) assert.equal(typeof theme[cue], 'string', `${cue} is given`);

const work = fs.mkdtempSync(path.join(os.tmpdir(), 'seele-theme-sounds-'));
try {
  const run = spawnSync(process.execPath, [path.join(folder, 'art/sounds.js'), work], {encoding: 'utf8'});
  assert.equal(run.status, 0, run.stderr);
  const written = fs.readdirSync(path.join(work, 'sounds')).sort();
  assert.deepEqual(written, cues.map(cue => path.basename(theme[cue])).sort(), 'sounds.js writes exactly the cues theme.json names');
  for (const cue of cues) {
    const file = path.join(folder, theme[cue]);
    const bytes = fs.readFileSync(file);
    assert.ok(bytes.equals(fs.readFileSync(path.join(work, theme[cue]))), `${theme[cue]} is what sounds.js writes`);
    assert.equal(bytes.toString('latin1', 0, 4), 'RIFF');
    assert.equal(bytes.readUInt32LE(4), bytes.length - 8, `${cue}: the RIFF size`);
    assert.equal(bytes.toString('latin1', 8, 16), 'WAVEfmt ');
    assert.deepEqual([bytes.readUInt16LE(20), bytes.readUInt16LE(22), bytes.readUInt32LE(24), bytes.readUInt16LE(34)],
      [1, 1, 48000, 16], `${cue}: PCM, mono, 48 kHz, 16-bit`);
    assert.equal(bytes.toString('latin1', 36, 40), 'data');
    const samples = bytes.readUInt32LE(40) / 2;
    assert.equal(44 + samples * 2, bytes.length, `${cue}: the data chunk is the rest of the file`);
    let peak = 0;
    for (let i = 0; i < samples; ++i) peak = Math.max(peak, Math.abs(bytes.readInt16LE(44 + i * 2)));
    const decibels = 20 * Math.log10(peak / 32767);
    assert.ok(decibels <= -12 && decibels >= -30, `${cue}: peaks at ${decibels.toFixed(1)} dBFS`);
    assert.ok(samples / 48000 <= 0.75, `${cue}: lasts ${(samples / 48).toFixed(0)} ms`);
    assert.equal(bytes.readInt16LE(bytes.length - 2), 0, `${cue}: ends on silence`);
  }
} finally {
  fs.rmSync(work, {recursive: true, force: true});
}
console.log(`Theme sounds: ${cues.length} Cyberpunk cues are reproducible 16-bit mono 48 kHz PCM WAVs, quiet and click-free`);
