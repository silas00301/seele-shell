#!/usr/bin/env node
// A theme folder as its author checks it before rendering anything
// (themes/AUTHORING.md): it registers, its palette roles read, and its mode is
// the one its colours say.
//
//   node tests/theme-check.js [--helper <seele-theme>] <themes/<id>>...
//
// With --helper, each folder is registered through the real `seele-theme
// register` into a temporary directory, which holds it to the whole theme
// contract and the folder rules exactly as the build does, and prints the
// build's own message when it refuses. Then the roles are measured against
// `base`, where the shell sets them: text, subtext and overlay against the
// floors the presets are held to (themes.rs), and the accent and the three
// status colours at 3:1, the contrast an interface mark needs. Text has to read
// on a card too. A dark theme's text is lighter than its ground and a light
// theme's darker, since the switcher files a theme under its mode and the other
// applications take it as their polarity. Registration checks none of this:
// legibility is the author's, and this is where it is measured.
//
// The ground, the text, the accent and the status colours are what the other
// applications paint most of: where one of those roles is not the Base16 slot
// the presets take it from (themes.rs), it is reported as a note, not a
// failure, since the shell draws the role while GTK, Qt, Neovim and the
// terminal draw the slot. A darker crust or a lifted surface is a theme's own
// choice and goes unremarked.
'use strict';
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawnSync} = require('node:child_process');

// Against `base`, as the presets' projection holds them, plus marks and cards.
const floors = {
  text: 7, subtext: 4.5, overlay: 3,
  accent: 3, red: 3, green: 3, yellow: 3,
};
const CARD_FLOOR = 4.5;
// Where the presets take each role from in their Base16 palette.
const projection = {
  base: 'base00', mantle: 'base01', crust: 'base00', surface: 'base02', overlay: 'base03',
  text: 'base05', subtext: 'base04', accent: 'base0D', red: 'base08', green: 'base0B', yellow: 'base0A',
};
const noted = ['base', 'text', 'accent', 'red', 'green', 'yellow'];

function luminance(hex) {
  const linear = index => {
    const value = parseInt(hex.slice(index, index + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(1) + 0.7152 * linear(3) + 0.0722 * linear(5);
}
function contrast(first, second) {
  const [a, b] = [luminance(first), luminance(second)];
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}
const ratio = value => `${(Math.floor(value * 10) / 10).toFixed(1)}:1`;
// The line the check prints for a folder that is ready.
const measured = ratios => Object.entries(ratios).map(([role, value]) => `${role} ${ratio(value)}`).join(', ');
const hex = value => typeof value === 'string' && /^#[0-9a-fA-F]{6}$/.test(value);

// What a theme document's roles say about its legibility: `failures` the
// author has to fix, `notes` worth knowing, and every measured ratio.
function legibility(theme) {
  const failures = [], notes = [], ratios = {};
  const missing = Object.keys(projection).filter(role => !hex(theme[role]));
  if (missing.length) {
    failures.push(`${missing.join(', ')}: not an opaque #rrggbb, so nothing can be measured`);
    return {failures, notes, ratios};
  }
  for (const [role, floor] of Object.entries(floors)) {
    ratios[role] = contrast(theme[role], theme.base);
    if (ratios[role] < floor) failures.push(`${role}: ${ratio(ratios[role])} against base, below ${floor}:1`);
  }
  ratios.card = contrast(theme.text, theme.surface);
  if (ratios.card < CARD_FLOOR) failures.push(`text: ${ratio(ratios.card)} against surface, below ${CARD_FLOOR}:1 on a card`);
  const dark = luminance(theme.base) < luminance(theme.text);
  if (theme.mode === 'dark' && !dark) failures.push('mode: "dark", but text is darker than base');
  if (theme.mode === 'light' && dark) failures.push('mode: "light", but text is lighter than base');
  const palette = theme.palette || {};
  for (const role of noted) {
    const slot = projection[role];
    if (hex(palette[slot]) && palette[slot].toLowerCase() !== theme[role].toLowerCase())
      notes.push(`${role} ${theme[role]} is not palette.${slot} ${palette[slot]}; the other applications draw ${slot}`);
  }
  return {failures, notes, ratios};
}

function register(helper, folder) {
  const out = fs.mkdtempSync(path.join(os.tmpdir(), 'seele-theme-check-'));
  try {
    const result = spawnSync(helper, ['register', path.basename(folder), folder, path.join(out, 'registered')], {encoding: 'utf8'});
    if (result.error) return [`${helper}: ${result.error.message}`];
    return result.status === 0 ? [] : result.stderr.trimEnd().split('\n');
  } finally {
    fs.rmSync(out, {recursive: true, force: true});
  }
}

function main(argv) {
  let helper = '';
  const folders = [];
  for (let index = 0; index < argv.length; ++index) {
    if (argv[index] === '--helper' && index + 1 < argv.length) helper = path.resolve(argv[++index]);
    else if (argv[index].startsWith('--')) return usage(`unknown option ${argv[index]}`);
    else folders.push(path.resolve(argv[index]));
  }
  if (!folders.length) return usage('name at least one theme folder');
  let failed = 0;
  for (const folder of folders) {
    const relative = path.relative(process.cwd(), folder);
    const label = relative && !relative.startsWith('..') ? relative : folder;
    const problems = [];
    let theme = null;
    try {
      theme = JSON.parse(fs.readFileSync(path.join(folder, 'theme.json'), 'utf8'));
    } catch (error) {
      problems.push(`theme.json: ${error.message}`);
    }
    // The build's own message, word for word, when it refuses the folder.
    const refusal = helper ? register(helper, folder) : [];
    const result = theme ? legibility(theme) : {failures: [], notes: [], ratios: {}};
    problems.push(...result.failures);
    if (problems.length || refusal.length) {
      ++failed;
      console.log(`${label}: not ready`);
      for (const line of refusal) console.log(`  ${line}`);
      for (const line of problems) console.log(`  ${line}`);
    } else {
      console.log(`${label}: ${helper ? 'registers' : 'registration not checked (--helper)'}; ${measured(result.ratios)}`);
    }
    for (const note of result.notes) console.log(`  note: ${note}`);
  }
  return failed ? 1 : 0;
}

function usage(message) {
  console.error(message);
  console.error('usage: node tests/theme-check.js [--helper <seele-theme>] <theme folder>...');
  return 2;
}

if (require.main === module) process.exitCode = main(process.argv.slice(2));
module.exports = {legibility, measured, contrast, floors, projection, CARD_FLOOR};
