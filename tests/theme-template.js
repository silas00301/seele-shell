#!/usr/bin/env node
// The template a new theme is copied from, and the guide that says how
// (themes/AUTHORING.md), held to what they promise:
//
// - themes/template supplies only a theme's identity, the eleven palette roles
//   and the Base16 palette, so every other slot keeps its default; each role is
//   the Base16 slot the presets project it from; it clears every floor
//   tests/theme-check.js measures; and no line of themes/default.nix ships it.
// - The guide's light start does the same in light mode.
// - Every folder the shell ships, and the example fixture, clear those floors
//   too, so the floors the guide asks for are ones real themes meet; and a
//   document that misses them, or contradicts its mode, fails, naming each.
// - The guide's table of floors and its sample check line are the check's own.
//
// Registration of the template, a copy of it and every recipe in the guide is
// projects/config-tools/tests/theme_folders.py's, through the real helper.
//
//   node tests/theme-template.js
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {legibility, measured, floors, projection, CARD_FLOOR} = require('./theme-check.js');

const themes = path.join(__dirname, '../themes');
const read = file => JSON.parse(fs.readFileSync(file, 'utf8'));
const template = read(path.join(themes, 'template/theme.json'));
const guide = fs.readFileSync(path.join(themes, 'AUTHORING.md'), 'utf8');
const roles = Object.keys(projection);
const base16 = Array.from('0123456789ABCDEF', digit => `base0${digit}`);

function onProjection(theme, label) {
  assert.deepEqual(Object.keys(theme.palette).sort(), [...base16].sort(), `${label}: the sixteen Base16 colours`);
  for (const key of base16) assert.match(theme.palette[key], /^#[0-9a-f]{6}$/, `${label}: palette.${key} is a lowercase #rrggbb`);
  for (const role of roles) assert.equal(theme[role], theme.palette[projection[role]], `${label}: ${role} is ${projection[role]}, as a preset projects it`);
  const result = legibility(theme);
  assert.deepEqual(result.failures, [], `${label} is legible`);
  assert.deepEqual(result.notes, [], `${label}: the applications draw what the shell draws`);
  return result;
}

// The template: identity, roles and palette, nothing else.
assert.equal(template.id, 'template');
assert.equal(template.mode, 'dark');
assert.deepEqual(Object.keys(template).sort(), ['id', 'name', 'mode', 'palette', ...roles].sort(), 'the template supplies no optional slot');
const shipped = fs.readFileSync(path.join(themes, 'default.nix'), 'utf8').split('\n')
  .map(line => line.trim()).filter(line => line.startsWith('./')).map(line => line.slice(2));
assert.ok(!shipped.includes('template'), 'no line of default.nix ships the template');
const ratios = onProjection(template, 'the template').ratios;

// The guide's sample line is what the check prints for a copy of the template.
assert.ok(guide.includes(`themes/aurora: registers; ${measured(ratios)}\n`), `AUTHORING.md shows the check's line for the template: ${measured(ratios)}`);

// The light start: the first JSON block of the guide that names a mode.
const recipes = [...guide.matchAll(/^```json\n(.*?)^```$/gms)].map(match => JSON.parse(match[1]));
const light = recipes.find(recipe => recipe.mode);
assert.ok(light, 'AUTHORING.md gives a light start');
assert.equal(light.mode, 'light');
onProjection({...template, ...light}, 'the light start');

// Every floor the check holds a role to is the one the guide's table names.
for (const [role, floor] of Object.entries(floors)) {
  const row = guide.split('\n').find(line => line.startsWith(`| \`${role}\` |`));
  assert.ok(row, `AUTHORING.md has a row for ${role}`);
  assert.ok(row.trimEnd().endsWith(` ${floor}:1 |`) || row.includes(` ${floor}:1, `), `AUTHORING.md gives ${role} the floor ${floor}:1: ${row}`);
}
assert.ok(guide.includes(`${CARD_FLOOR}:1 on \`surface\``), 'AUTHORING.md gives text its floor on a card');

// The shipped folders and the example meet the same floors.
for (const folder of [...shipped.map(name => path.join(themes, name)), path.join(__dirname, 'theme-folders/example')]) {
  const theme = read(path.join(folder, 'theme.json'));
  assert.deepEqual(legibility(theme).failures, [], `${path.basename(folder)} is legible`);
}

// A theme that misses them fails, naming each.
const dim = legibility({...template, subtext: '#4a4f5a', yellow: '#3a3420', surface: '#a0a4ac'});
assert.deepEqual(dim.failures, [
  'subtext: 2.1:1 against base, below 4.5:1',
  'yellow: 1.4:1 against base, below 3:1',
  'text: 1.9:1 against surface, below 4.5:1 on a card',
]);
assert.deepEqual(legibility({...template, mode: 'light'}).failures, ['mode: "light", but text is lighter than base']);
assert.deepEqual(legibility({...template, accent: '#7aa6f080'}).failures, ['accent: not an opaque #rrggbb, so nothing can be measured']);
assert.deepEqual(legibility({...template, accent: '#e8955a'}).notes, ['accent #e8955a is not palette.base0D #7aa6f0; the other applications draw base0D']);

console.log(`Theme template: legible on the presets' projection, dark and light; ${shipped.length} shipped folders and the example meet its floors; AUTHORING.md's floors and sample line are the check's`);
