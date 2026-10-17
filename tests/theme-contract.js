#!/usr/bin/env node
// The theme contract: every styling value the shell draws is a slot of
// projects/shared/theme-contract.json, implemented once by Theme.qml.
//
//   node tests/theme-contract.js <projects directory> [--report] [--update]
//
// Four checks, each failing loudly:
//
// 1. The contract is well formed: unique slot names, known types, literal
//    defaults of their declared type, required palette roles, fixed signal
//    colours, and optional sound and wallpaper slots with defined fallbacks.
// 2. Theme.qml and the contract agree slot for slot and in order: same name,
//    QML type, readonly-ness and default. A Theme.qml property the contract
//    does not list, or a slot Theme.qml does not implement, fails, as does a
//    shell root that shadows a slot with a value of its own.
// 3. Theme documents are checked against the contract: every preset the
//    shell receives today validates, and documents that break each rule fail.
// 4. No styling literal is left outside the contract in the shell, its
//    shared components or Notes: no colour literal, no colour arithmetic on a
//    literal, no easing or weight enum, and no number bound to a geometry,
//    type, shape, material or motion property except the structural values
//    listed in the contract's `structural` section. Artwork regions are
//    marked in the source with a reason and are listed in the report.
//
// `--report` prints every violation instead of the first twenty, plus the
// artwork regions, so a migration can be worked through file by file.
// `--update` rewrites the contract's slots from Theme.qml -- its sections are
// the groups, its comments the descriptions -- keeping everything else in the
// JSON, then checks the result. Review the diff as you would a baseline.
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const args = process.argv.slice(2);
const report = args.includes('--report');
const update = args.includes('--update');
const projects = path.resolve(args.find(argument => !argument.startsWith('--')) || 'projects');
const contractPath = path.join(projects, 'shared/theme-contract.json');
const themePath = path.join(projects, 'shared/Theme.qml');
const contract = JSON.parse(fs.readFileSync(contractPath, 'utf8'));
const palette = vm.runInNewContext(fs.readFileSync(path.join(projects, 'shared/Palette.js'), 'utf8').replace(/^\.pragma library\s*/, '') + '\nfallback');

// Theme.qml keeps one property per line, before its config IO, under a
// `// <Section> ----` header per group, each after the comment describing it.
function themeSlots(text) {
  const sections = new Map(contract.groups.map(group => [group.section, group]));
  const groups = [];
  let group = null, comment = [], block = '';
  for (const line of text.split('\n  function alpha(')[0].split('\n')) {
    const header = /^ {2}\/\/ ([A-Z][\w ]*?) -{5,}$/.exec(line);
    if (header) {
      assert.ok(sections.has(header[1]), `Theme.qml section ${header[1]} is not a contract group`);
      group = {...sections.get(header[1]), slots: []};
      groups.push(group); comment = []; block = '';
      continue;
    }
    const note = /^ {2}\/\/ ?(.*)$/.exec(line);
    if (note) { comment.push(note[1]); continue; }
    const match = /^ {2}(readonly )?property (\w+) (\w+): (.*?)(?:\s+\/\/\s*(.*))?$/.exec(line);
    if (!match) continue;
    assert.ok(group, `${match[3]} is declared before the first section`);
    if (comment.length) { block = comment.join(' ').replace(/\s+/g, ' ').trim(); comment = []; }
    const [, readonly, type, name, value, trailing] = match;
    const slot = {name};
    const bound = /^Palette\.fallback\.(\w+)$/.exec(value);
    const enumOf = table => Object.keys(contract.enums[table]).find(key => contract.enums[table][key] === value);
    if (bound) Object.assign(slot, {type, default: palette[bound[1]], binding: value});
    else if (!readonly && type === 'color' && /^"#[0-9a-fA-F]+"$/.test(value)) Object.assign(slot, {type, default: JSON.parse(value)});
    else if (!readonly && type === 'int' && enumOf('weight')) Object.assign(slot, {type: 'weight', default: enumOf('weight')});
    else if (!readonly && type === 'int' && enumOf('easing')) Object.assign(slot, {type: 'easing', default: enumOf('easing')});
    else if (!readonly && (type === 'int' || type === 'real') && /^-?\d+(\.\d+)?$/.test(value)) Object.assign(slot, {type, default: Number(value)});
    else if (!readonly && type === 'string' && /^"[^"]*"$/.test(value)) Object.assign(slot, {type: group.kind === 'optional' ? 'path' : 'string', default: JSON.parse(value)});
    else if (type === 'var' && /^\[/.test(value)) Object.assign(slot, {type: 'colorList', default: JSON.parse(value)});
    else if (!readonly && type === 'var' && /^\(\{.*\}\)$/.test(value)) Object.assign(slot, {type: 'outputPaths', default: JSON.parse(value.slice(1, -1))});
    else Object.assign(slot, {type: type === 'var' ? 'colorList' : type, derived: value});
    if (group.kind && !Object.hasOwn(slot, 'derived')) slot[group.kind] = true;
    if (group.kind === 'optional' && !Object.hasOwn(slot, 'derived')) slot.fallback = contract.fallbacks[group.name];
    slot.description = trailing ? trailing.charAt(0).toUpperCase() + trailing.slice(1) + '.' : block;
    group.slots.push(slot);
  }
  return groups;
}
if (update) {
  contract.groups = themeSlots(fs.readFileSync(themePath, 'utf8'));
  fs.writeFileSync(contractPath, JSON.stringify(contract, null, 2) + '\n');
  console.log(`Wrote ${contract.groups.reduce((total, group) => total + group.slots.length, 0)} slots in ${contract.groups.length} groups to ${contractPath}`);
}

// ------------------------------------------------------------- 1. contract
const types = {
  color: {qml: 'color', check: value => typeof value === 'string' && /^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(value)},
  colorList: {qml: 'var', check: value => Array.isArray(value) && value.length > 0 && value.every(types.color.check)},
  int: {qml: 'int', check: value => Number.isInteger(value)},
  real: {qml: 'real', check: value => typeof value === 'number' && Number.isFinite(value)},
  string: {qml: 'string', check: value => typeof value === 'string'},
  path: {qml: 'string', check: value => typeof value === 'string' && (value === '' || value.startsWith('/'))},
  outputPaths: {qml: 'var', check: value => Boolean(value) && typeof value === 'object' && !Array.isArray(value)
    && Object.entries(value).every(([output, file]) => /^[A-Za-z0-9._-]{1,64}$/.test(output) && typeof file === 'string' && file.startsWith('/'))},
  url: {qml: 'string', check: value => typeof value === 'string'},
  weight: {qml: 'int', check: value => Object.hasOwn(contract.enums.weight, value)},
  easing: {qml: 'int', check: value => Object.hasOwn(contract.enums.easing, value)},
};
assert.equal(contract.version, 1, 'contract version');
assert.deepEqual(Object.keys(contract.types).sort(), Object.keys(types).sort(), 'the contract describes exactly the types this check knows');
for (const [name, values] of [['weight', 'Font'], ['easing', 'Easing']])
  for (const [key, value] of Object.entries(contract.enums[name])) assert.equal(value, `${values}.${key}`, `enums.${name}.${key}`);
for (const key of ['id', 'name', 'mode']) assert.ok(contract.documents.identity[key], `documents.identity.${key}`);
assert.ok(Array.isArray(contract.documents.extensions), 'documents.extensions');
assert.ok(contract.structural && Array.isArray(contract.structural.values) && contract.structural.artwork, 'the structural values are stated');
assert.ok(contract.members && typeof contract.members.description === 'string', 'the non-styling members are stated');
assert.ok(Array.isArray(contract.groups) && contract.groups.length > 0, 'contract groups');
const slots = new Map();
for (const group of contract.groups) {
  assert.match(group.name, /^[a-z][a-z-]*$/, `group name ${group.name}`);
  assert.ok(typeof group.description === 'string' && group.description.length > 20, `group ${group.name} is described`);
  assert.ok(Array.isArray(group.slots) && group.slots.length > 0, `group ${group.name} has slots`);
  for (const slot of group.slots) {
    const where = `${group.name}.${slot.name}`;
    assert.match(slot.name || '', /^[a-z][A-Za-z0-9]*$/, `${where}: slot name`);
    assert.ok(!slots.has(slot.name), `${where}: duplicate slot`);
    assert.ok(!Object.hasOwn(contract.documents.identity, slot.name) && !contract.documents.extensions.includes(slot.name), `${where}: collides with a document key`);
    assert.ok(types[slot.type], `${where}: unknown type ${slot.type}`);
    assert.ok(typeof slot.description === 'string' && slot.description.length > 0, `${where}: description`);
    const derived = Object.hasOwn(slot, 'derived'), valued = Object.hasOwn(slot, 'default');
    assert.ok(derived !== valued, `${where}: exactly one of default and derived`);
    if (valued) assert.ok(types[slot.type].check(slot.default), `${where}: default ${JSON.stringify(slot.default)} is not a ${slot.type}`);
    if (derived) {
      assert.ok(typeof slot.derived === 'string' && slot.derived.length > 0, `${where}: derived expression`);
      assert.ok(!slot.optional && !slot.required && !slot.fixed, `${where}: a derived slot is computed, never supplied`);
    }
    assert.ok(!(slot.required && slot.optional) && !(slot.fixed && (slot.required || slot.optional)), `${where}: one of required, optional and fixed at most`);
    if (slot.optional) assert.equal(slot.fallback, contract.fallbacks[group.name], `${where}: an optional slot takes its group's fallback`);
    else assert.ok(!Object.hasOwn(slot, 'fallback'), `${where}: only an optional slot has a fallback beyond its default`);
    slots.set(slot.name, {...slot, group: group.name});
  }
}
const group = name => contract.groups.find(entry => entry.name === name) || {slots: []};
for (const entry of contract.groups) {
  assert.ok(typeof entry.section === 'string' && entry.section.length > 0, `group ${entry.name} names its Theme.qml section`);
  assert.ok([undefined, 'required', 'fixed', 'optional'].includes(entry.kind), `group ${entry.name}: kind ${entry.kind}`);
  for (const slot of entry.slots) for (const kind of ['required', 'fixed', 'optional'])
    assert.equal(Boolean(slot[kind]), entry.kind === kind && !Object.hasOwn(slot, 'derived'), `${entry.name}.${slot.name}: ${kind} follows its group`);
}
assert.equal(group('palette').kind, 'required', 'palette roles are required');
assert.equal(group('signal').kind, 'fixed', 'signal colours are fixed');
for (const name of ['sound', 'wallpaper']) assert.equal(group(name).kind, 'optional', `${name} slots are optional`);
assert.ok(group('palette').slots.every(slot => slot.required && slot.type === 'color'), 'every palette role is a required colour');
assert.deepEqual([...slots.values()].filter(slot => slot.required).map(slot => slot.group), group('palette').slots.map(() => 'palette'), 'only palette roles are required');
assert.ok(group('signal').slots.every(slot => slot.fixed), 'signal colours are fixed');
// An optional slot's default is empty, which is where its fallback applies:
// no file, or for the wallpaper's per-output images, no output named.
const empty = slot => slot.type === 'path' ? slot.default === '' : slot.type === 'outputPaths' && Object.keys(slot.default).length === 0;
for (const name of ['sound', 'wallpaper']) {
  const supplied = group(name).slots.filter(slot => !Object.hasOwn(slot, 'derived'));
  assert.ok(supplied.length > 0 && supplied.every(slot => slot.optional && (slot.type === 'path' || name === 'wallpaper') && empty(slot)), `every ${name} slot is optional and empty by default`);
  assert.ok(typeof contract.fallbacks[name] === 'string' && contract.fallbacks[name].length > 20, `the ${name} fallback is defined`);
}

// ------------------------------------------------------------- 2. parity
// Theme.qml keeps one property per line, before its config IO, in the
// contract's order.
function qmlLiteral(slot) {
  if (slot.type === 'color') return JSON.stringify(slot.default);
  if (slot.type === 'colorList') return `[${slot.default.map(value => JSON.stringify(value)).join(', ')}]`;
  if (slot.type === 'weight') return contract.enums.weight[slot.default];
  if (slot.type === 'easing') return contract.enums.easing[slot.default];
  if (['string', 'path', 'url'].includes(slot.type)) return JSON.stringify(slot.default);
  if (slot.type === 'outputPaths') return `(${JSON.stringify(slot.default)})`;
  return String(slot.default);
}
const normalise = text => text.replace(/\s+/g, ' ').trim();
const themeText = fs.readFileSync(themePath, 'utf8');
const themeBody = themeText.split('\n  function alpha(')[0];
const declared = new Map();
for (const match of themeBody.matchAll(/^ {2}(readonly )?property (\w+) (\w+): (.*?)(?:\s+\/\/.*)?$/gm)) {
  assert.ok(!declared.has(match[3]), `Theme.qml declares ${match[3]} twice`);
  declared.set(match[3], {readonly: Boolean(match[1]), type: match[2], value: match[4]});
}
// A palette slot binds the shared fallback, so its default is Palette.js's.
const parity = [];
for (const [name, slot] of slots) {
  const property = declared.get(name);
  if (!property) { parity.push(`${name}: in the contract, not in Theme.qml`); continue; }
  if (property.type !== types[slot.type].qml) parity.push(`${name}: Theme.qml type ${property.type}, contract ${slot.type}`);
  // Derived slots are computed; everything a theme supplies stays writable.
  if (property.readonly !== Object.hasOwn(slot, 'derived') && !slot.fixed) parity.push(`${name}: readonly in Theme.qml must match derived in the contract`);
  const expected = Object.hasOwn(slot, 'derived') ? slot.derived : slot.binding || qmlLiteral(slot);
  if (normalise(property.value) !== normalise(expected)) parity.push(`${name}: Theme.qml ${property.value}, contract ${expected}`);
  const bound = /^Palette\.fallback\.(\w+)$/.exec(slot.binding || '');
  if (slot.binding && (!bound || palette[bound[1]] !== slot.default)) parity.push(`${name}: default ${slot.default} is not what ${slot.binding} holds`);
}
for (const name of declared.keys()) if (!slots.has(name)) parity.push(`${name}: in Theme.qml, not in the contract`);
// Slots are read from the ordered block above the helpers, but a property can
// be declared anywhere in a QML object, so the whole file is scanned: a root
// property below the helpers is either a listed non-styling member or a slot
// that escaped the contract.
const members = Object.keys(contract.members).filter(name => name !== 'description');
const anywhere = [...themeText.matchAll(/^ {2}(?:readonly |default |required )*property\s+[\w<>.]+\s+(\w+)\b/gm)].map(match => match[1]);
for (const name of anywhere) if (!declared.has(name) && !members.includes(name)) parity.push(`${name}: declared below Theme.qml's helpers, outside the contract`);
for (const name of members) {
  if (!anywhere.includes(name)) parity.push(`${name}: a contract member Theme.qml does not declare`);
  if (declared.has(name) || slots.has(name)) parity.push(`${name}: a member is not a slot; declare it after the helpers`);
}
assert.deepEqual(parity, [], 'Theme.qml implements the contract exactly');
assert.deepEqual([...declared.keys()], [...slots.keys()], 'Theme.qml declares the slots in the contract\'s order');
assert.deepEqual(themeSlots(themeText), contract.groups, 'the contract is what --update writes from Theme.qml: run it and review the diff');

// A consumer that roots itself in Shared.Theme must not shadow a slot, or the
// shell would draw its own value while the contract claimed another.
for (const file of [path.join(projects, 'shell/shell.qml'), path.join(projects, 'notes/shell.qml'), path.join(__dirname, 'theme-snapshots/scene.qml')]) {
  const text = fs.readFileSync(file, 'utf8');
  for (const match of text.matchAll(/^ {2}(?:readonly )?property \w+ (\w+)\b/gm))
    assert.ok(!slots.has(match[1]), `${file} shadows the contract slot ${match[1]}`);
}

// ---------------------------------------------------------- 3. documents
// The rules a theme document follows, as the contract's `documents` states
// them. Registration (SIL-234) enforces the same rules when it builds a theme.
const roles = [...slots.values()].filter(slot => slot.required).map(slot => slot.name);
const {identity, extensions} = contract.documents;
function validate(document) {
  const errors = [];
  if (!document || typeof document !== 'object' || Array.isArray(document)) return ['a theme document is a JSON object'];
  for (const [key, rule] of Object.entries(identity)) {
    if (!Object.hasOwn(document, key)) { errors.push(`missing ${key}`); continue; }
    if (rule.enum ? !rule.enum.includes(document[key]) : typeof document[key] !== 'string' || !new RegExp(rule.pattern).test(document[key])) errors.push(`${key}: ${JSON.stringify(document[key])}`);
  }
  for (const name of roles) if (!Object.hasOwn(document, name)) errors.push(`missing required slot ${name}`);
  for (const [key, value] of Object.entries(document)) {
    if (Object.hasOwn(identity, key) || extensions.includes(key)) continue;
    const slot = slots.get(key);
    if (!slot) errors.push(`${key}: not a contract slot`);
    else if (Object.hasOwn(slot, 'derived')) errors.push(`${key}: derived, so a theme cannot supply it`);
    else if (slot.fixed) errors.push(`${key}: fixed by the contract`);
    else if (!types[slot.type].check(value)) errors.push(`${key}: ${JSON.stringify(value)} is not a ${slot.type}`);
  }
  return errors;
}
// Every slot a theme leaves out takes its default; an optional slot's empty
// default is where its group's fallback applies.
function resolve(document) {
  const resolved = {};
  for (const [name, slot] of slots) if (!Object.hasOwn(slot, 'derived')) resolved[name] = Object.hasOwn(document, name) && !slot.fixed ? document[name] : slot.default;
  return resolved;
}
// The existing presets, as `seele-theme list` publishes them (SIL-233): each
// is a valid document, and so is the theme.json it becomes once the helper
// adds the publication's version and the configured wallpaper. Each supplies
// its identity, the palette roles and the typeface and nothing else, so every
// other slot is the default the pixel baselines were recorded with.
const published = JSON.parse(fs.readFileSync(path.join(__dirname, 'theme-snapshots/themes.json'), 'utf8'));
const presetKeys = new Set([...Object.keys(identity), ...roles, 'fontFamily', ...extensions]);
for (const theme of published.themes) {
  assert.deepEqual(validate(theme), [], `${theme.id} is a valid theme document`);
  assert.deepEqual(validate({...theme, version: 2, wallpaper: palette.wallpaper}), [], `${theme.id} is a valid theme.json`);
  assert.deepEqual(Object.keys(theme).filter(key => !presetKeys.has(key)), [], `${theme.id} supplies only its identity, palette and typeface`);
}
const minimal = {id: 'example', name: 'Example', mode: 'dark', ...Object.fromEntries(roles.map(name => [name, slots.get(name).default]))};
assert.deepEqual(validate(minimal), [], 'the required slots alone make a theme');
const defaults = resolve(minimal);
assert.equal(defaults.wallpaper, '', 'a theme without a wallpaper keeps the configured one');
for (const slot of group('sound').slots) assert.equal(defaults[slot.name], '', `${slot.name} is silent unless a theme supplies it`);
assert.equal(defaults.radius, slots.get('radius').default, 'an omitted slot takes its default');
assert.equal(resolve({...minimal, radius: 12}).radius, 12, 'a supplied slot wins');
const broken = [
  [{...minimal, id: 'Not An Id'}, 'id'],
  [{...minimal, mode: 'dim'}, 'mode'],
  [(({name, ...rest}) => rest)(minimal), 'missing name'],
  [(({accent, ...rest}) => rest)(minimal), 'missing required slot accent'],
  [{...minimal, radius: 'round'}, 'radius'],
  [{...minimal, radius: 8.5}, 'radius'],
  [{...minimal, accent: 'lavender'}, 'accent'],
  [{...minimal, panelColor: '#000000'}, 'panelColor: derived'],
  [{...minimal, iosRed: '#000000'}, 'iosRed: fixed'],
  [{...minimal, sparkle: true}, 'sparkle: not a contract slot'],
  [{...minimal, easingStandard: 'Bounce'}, 'easingStandard'],
  [{...minimal, weightStrong: 600}, 'weightStrong'],
  [{...minimal, wallpaper: 'relative.jpg'}, 'wallpaper'],
  [{...minimal, soundNotification: 'ping.oga'}, 'soundNotification'],
  [{...minimal, wallpaperOutputs: {'DP-1': 'relative.jpg'}}, 'wallpaperOutputs'],
  [{...minimal, wallpaperOutputs: {'DP 1': '/x.png'}}, 'wallpaperOutputs'],
  [{...minimal, wallpaperOutputs: ['/x.png']}, 'wallpaperOutputs'],
  [[], 'a theme document is a JSON object'],
];
for (const [document, message] of broken) assert.ok(validate(document).some(error => error.startsWith(message)), `rejects ${message}: ${validate(document)}`);

// Theme.qml applies a published theme through Contract.js. It sets every slot
// a theme may supply, its own wallpaper included, and each slot takes exactly
// what `resolve` above says it does.
const runtime = vm.runInNewContext(fs.readFileSync(path.join(projects, 'shared/Contract.js'), 'utf8').replace(/^\.pragma library\s*/, '') + '\n({slots, values})');
const applied = runtime.slots(contract);
assert.deepEqual([...applied].map(slot => slot.name),
  [...slots.values()].filter(slot => !Object.hasOwn(slot, 'derived') && !slot.fixed).map(slot => slot.name),
  'Theme.qml applies every slot a theme may supply');
const appliedAs = document => Object.fromEntries(Object.entries(resolve(document)).filter(([name]) => applied.some(slot => slot.name === name)));
for (const document of [minimal, {...minimal, radius: 12, weightStrong: 'Bold', easingStandard: 'OutBack', fontFamily: 'Example Sans'}, ...published.themes])
  assert.deepEqual({...runtime.values(applied, document)}, appliedAs(document), `${document.id}: Contract.js resolves as the contract does`);
assert.equal(runtime.values(applied, {...minimal, radius: 'round'}).radius, slots.get('radius').default, 'a value of the wrong kind leaves the default');

// Drop-in theme folders (SIL-234). The fixture folders hold registered
// documents' sources, and the snapshot harness renders what the helper lists
// for them; both are theme documents, apart from the files a folder names.
// A folder's own wallpaper (SIL-235) is what its theme.json publishes.
const folders = JSON.parse(fs.readFileSync(path.join(__dirname, 'theme-snapshots/folders.json'), 'utf8'));
for (const theme of folders.themes) {
  assert.deepEqual(validate(theme), [], `${theme.id} is a valid theme document`);
  assert.deepEqual(validate({...theme, version: 2, wallpaper: palette.wallpaper}), [], `${theme.id} is a valid theme.json`);
}
assert.deepEqual(validate(JSON.parse(fs.readFileSync(path.join(__dirname, 'theme-folders/example/theme.json'), 'utf8'))), [], 'the example folder is a valid theme document');
const pictured = JSON.parse(fs.readFileSync(path.join(__dirname, 'theme-folders/pictured/theme.json'), 'utf8'));
const registeredImage = '/nix/store/registered/pictured/wallpaper.png';
assert.deepEqual(validate({...pictured, wallpaper: registeredImage}), [], 'the pictured folder, registered, is a valid theme document');
assert.equal(runtime.values(applied, {...pictured, wallpaper: registeredImage, version: 2}).wallpaper, registeredImage, 'a published theme\'s own wallpaper is applied');
assert.equal(runtime.values(applied, {...minimal, version: 2, wallpaper: palette.wallpaper}).wallpaper, palette.wallpaper, 'a theme without one is published with the configured wallpaper');
assert.equal(runtime.values(applied, minimal).wallpaper, '', 'and a document that names none leaves the configured wallpaper to Theme.qml');

// The host's wallpaper mapping (SIL-236): the helper publishes, beside
// `wallpaper`, the outputs that show another image, by output name. A theme
// that has none names no output, so every output draws `wallpaper`.
const mapped = {...minimal, version: 2, wallpaper: '/day.png', wallpaperOutputs: {'DP-1': '/portrait.png', 'HDMI-A-1': '/wide.jpg'}};
assert.deepEqual(validate(mapped), [], 'a theme.json with images per output is valid');
assert.deepEqual(defaults.wallpaperOutputs, {}, 'a theme without a mapping names no output');
assert.deepEqual(runtime.values(applied, mapped).wallpaperOutputs, mapped.wallpaperOutputs, 'the images per output are applied');
assert.deepEqual(runtime.values(applied, minimal).wallpaperOutputs, {}, 'and a theme without them leaves none behind');
for (const wrong of [['/x.png'], {'DP-1': 'relative.png'}, {'DP-1': 7}, 'DP-1'])
  assert.deepEqual(runtime.values(applied, {...minimal, wallpaperOutputs: wrong}).wallpaperOutputs, {}, `${JSON.stringify(wrong)} is not applied`);

// ------------------------------------------------------------- 4. the lint
// Comments and string contents are masked so a number in prose or a label is
// never read as styling; string values are kept to find colour literals.
function mask(text) {
  let out = '', index = 0;
  const strings = [];
  while (index < text.length) {
    const char = text[index], next = text[index + 1];
    if (char === '/' && next === '/') {
      const end = text.indexOf('\n', index);
      const stop = end < 0 ? text.length : end;
      out += ' '.repeat(stop - index); index = stop;
    } else if (char === '/' && next === '*') {
      const stop = text.indexOf('*/', index + 2) + 2;
      out += text.slice(index, stop).replace(/[^\n]/g, ' '); index = stop;
    } else if (char === '"' || char === "'" || char === '`') {
      let end = index + 1;
      while (end < text.length && text[end] !== char) end += text[end] === '\\' ? 2 : 1;
      strings.push({offset: index, value: text.slice(index + 1, end)});
      out += char + text.slice(index + 1, end).replace(/[^\n]/g, '_') + char; index = end + 1;
    } else { out += char; index += 1; }
  }
  return {masked: out, strings};
}

const styleExact = new Set(['x', 'y', 'width', 'height', 'spacing', 'radius', 'padding', 'margins', 'inset', 'opacity', 'scale', 'rotation',
  'duration', 'lineHeight', 'pixelSize', 'pointSize', 'letterSpacing', 'wordSpacing', 'weight', 'lineWidth', 'strokeWidth', 'strokeStyle',
  'fillStyle', 'globalAlpha', 'color', 'blurMax', 'blur', 'blurMultiplier', 'shadowBlur', 'shadowOpacity', 'shadowHorizontalOffset',
  'shadowVerticalOffset', 'shadowScale', 'brightness', 'contrast', 'saturation', 'colorizationColor', 'colorization', 'font', 'unit']);
const styleSuffix = /(?:Width|Height|Spacing|Radius|Margins?|Padding|Inset|CenterOffset|Size|Opacity|Color|Gap|Extent|Reach|Stroke)$/;
// Motion that is not a duration: flick physics, highlight travel, and the
// parameters of springs, smoothing and eased curves.
const motionExact = new Set(['flickDeceleration', 'maximumFlickVelocity', 'highlightMoveDuration', 'highlightResizeDuration',
  'highlightMoveVelocity', 'highlightResizeVelocity']);
// Drawing coordinates: what an artwork region may still state for itself.
function isCoordinate(name) {
  return /^(?:x|y|z|width|height|radius|spacing|margins|padding|inset|scale|rotation|pixelSize|pointSize|lineWidth|strokeWidth)$|(?:Width|Height|Radius|Spacing|Margins?|Padding|Inset|Size|CenterOffset)$/.test(name.split('.').pop());
}
function isStyling(name, line) {
  const last = name.split('.').pop();
  if (motionExact.has(last)) return true;
  if (/^(?:velocity|spring|damping|mass|epsilon)$/.test(last)) return /Animation\b/.test(line);
  if (/^(?:amplitude|period|overshoot)$/.test(last)) return /(?:^|\.)easing\.\w+$/.test(name);
  if (/^(?:position)$/.test(last)) return /GradientStop/.test(line);
  if (/^(?:from|to)$/.test(last)) return /Animation\b/.test(line);
  if (/^(?:count|maximumLineCount|contentHeight|contentWidth|contentX|contentY|currentIndex|index|z|stepSize)$/.test(last)) return false;
  return styleExact.has(last) || styleSuffix.test(last);
}

// The structural numbers a styling expression may still carry; see the
// contract's `structural` section for why each one is not a styling value.
function structural(expression, start, end, value, property, context = '') {
  const before = expression.slice(0, start).replace(/\s+$/, ''), after = expression.slice(end).replace(/^\s+/, '');
  const last = property.split('.').pop();
  if (value === 0) return true;
  // A count: both sides of a card, the columns of a grid, the rows a list
  // shows, or the gaps between them. A count multiplies a measure or divides
  // one; a number divided by a measure is a length, and is styling.
  const number = '(?:0[xX][0-9a-fA-F_]+|0[oO][0-7_]+|0[bB][01_]+|(?:\\d[\\d_]*(?:\\.[\\d_]*)?|\\.\\d[\\d_]*)(?:[eE][-+]?\\d+)?)';
  const literalBefore = new RegExp(`(?<![\\w.])${number}\\s*[*/]$`), literalAfter = new RegExp(`^\\*\\s*${number}(?![\\w.])`);
  if (Number.isInteger(value) && ((/[*/]$/.test(before) && !literalBefore.test(before)) || (/^\*/.test(after) && !literalAfter.test(after)))) return true;
  if (value === 1 && /[-+]$/.test(before) && (/^\)+\s*[*/]/.test(after) || /\*\s*\([^()]*[-+]$/.test(before))) return true;
  // A half: the middle of a span, the same as dividing it by two.
  if (value === 0.5 && (/\*$/.test(before) || /^\*/.test(after) || /Math\.(?:max|min)\($/.test(before) || last === 'position')) return true;
  // One full turn of a rotation.
  if (value === 360 && /^(?:from|to)$/.test(last) && /\bon\s+rotation\b|RotationAnimation/.test(context)) return true;
  // The two ends of a gradient.
  if (value === 1 && last === 'position') return true;
  // A comparison, an index or a remainder is logic about the data.
  if (/(?:[<>]=?|[!=]==?|%)$/.test(before) || /^(?:[<>]=?|[!=]==?|%)/.test(after) || (/\[$/.test(before) && /^\]/.test(after))) return true;
  // Full strength for a normalised property, or the unit clamp of a guard.
  // `from`/`to` reach full strength only where the animation drives a
  // normalised property: `on opacity`, or `property: "opacity"` in its body.
  const normalised = /\bon\s+(?:opacity|scale|strength)\b|\bproperty:\s*["'](?:opacity|scale|strength)["']|\bproperties:\s*["'][^"']*\b(?:opacity|scale)\b/.test(context);
  if (value === 1 && (/^(?:opacity|scale|globalAlpha)$/.test(last) || (/^(?:from|to)$/.test(last) && normalised) || /Math\.(?:max|min)\($/.test(before) || /^\)/.test(after) && /Math\.(?:max|min)\([^()]*,$/.test(before) || /Math\.(?:max|min)\(\s*1\s*,/.test(expression.slice(Math.max(0, start - 9), end + 1))))
    return true;
  // An empty or full channel of a constructed colour: opaque mask ink, or an
  // interpolation that keeps its alpha. Any other channel value is a colour.
  if (value === 1 && /Qt\.(?:rgba|hsla|hsva)\([^()]*$/.test(before)) return true;
  // Qt's "no preference" sentinel for a layout size.
  if (value === 1 && /-$/.test(before) && /^Layout\./.test(property)) return true;
  return false;
}

// Every numeric literal form QML's JavaScript reads: decimals with or without
// a leading digit, exponents, hexadecimal, octal and binary, and numeric
// separators. `literalValue` reads one the way the engine would.
const numberSource = String.raw`0[xX][0-9a-fA-F](?:_?[0-9a-fA-F])*|0[oO][0-7](?:_?[0-7])*|0[bB][01](?:_?[01])*|(?:\d(?:_?\d)*(?:\.(?:\d(?:_?\d)*)?)?|\.\d(?:_?\d)*)(?:[eE][-+]?\d(?:_?\d)*)?`;
const numberLiteral = () => new RegExp(String.raw`(?<![\w.#])(${numberSource})(?![\w.])`, 'g');
const literalValue = text => Number(text.replace(/_/g, ''));
// Every SVG colour keyword Qt accepts as a colour, so any of them is found.
const namedColours = new RegExp(`^(?:${'aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet brown burlywood cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray grey green greenyellow honeydew hotpink indianred indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen lightskyblue lightslategray lightslategrey lightsteelblue lightyellow lime limegreen linen magenta maroon mediumaquamarine mediumblue mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen mediumturquoise mediumvioletred midnightblue mintcream mistyrose moccasin navajowhite navy oldlace olive olivedrab orange orangered orchid palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue purple red rosybrown royalblue saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue slateblue slategray slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet wheat white whitesmoke yellow yellowgreen'.split(' ').join('|')})$`, 'i');
const isColourLiteral = value => namedColours.test(value) || /^#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(value);
// A property whose value is drawn as a colour, where any string but
// "transparent" is a colour the contract never named.
const colourProperty = /(?:^|\.)(?:color|\w*Color|tint|\w*Tint|\w*Style)$/;
const exemptFiles = new Set(['shared/Theme.qml', 'shared/Palette.js']);
function sources() {
  const files = [];
  for (const root of ['shell', 'shared', 'notes']) {
    const walk = directory => {
      for (const entry of fs.readdirSync(directory, {withFileTypes: true})) {
        const full = path.join(directory, entry.name);
        if (entry.isDirectory()) { if (entry.name !== 'node_modules') walk(full); }
        else if (/\.(qml|js)$/.test(entry.name)) files.push(path.relative(projects, full));
      }
    };
    walk(path.join(projects, root));
  }
  return files.sort().filter(file => !exemptFiles.has(file));
}

const found = new Set(), violations = [], artwork = [];
for (const file of sources()) {
  const text = fs.readFileSync(path.join(projects, file), 'utf8');
  const lines = text.split('\n');
  // Artwork: drawing coordinates of an icon or an illustration, scaled to a
  // contract size and coloured from contract roles. Each region says why.
  const skip = new Set();
  let open = null;
  lines.forEach((line, index) => {
    const begin = /\/\/ theme-contract: artwork\b(.*)$/.exec(line), end = /\/\/ theme-contract: end\b/.test(line);
    if (begin) {
      assert.equal(open, null, `${file}:${index + 1}: nested artwork region`);
      assert.ok(begin[1].trim().replace(/^[-—:]+\s*/, '').length > 10, `${file}:${index + 1}: an artwork region states its reason`);
      open = {file, line: index + 1, reason: begin[1].trim().replace(/^[-—:]+\s*/, '')};
    } else if (end) {
      assert.ok(open, `${file}:${index + 1}: artwork end without a start`);
      open.end = index + 1; artwork.push(open); open = null;
    } else if (open) skip.add(index);
  });
  assert.equal(open, null, `${file}: artwork region left open`);

  const {masked, strings} = mask(text);
  const maskedLines = masked.split('\n');
  // The QML object each line sits in, so a property whose meaning depends on
  // its type -- `position` in a GradientStop, `spring` in a SpringAnimation --
  // is recognised however the object is laid out. Each `{` records the word
  // before it (`GradientStop {`, `NumberAnimation on opacity {`), so the
  // innermost enclosing type is known at the start of every line.
  const within = [];
  {
    // Each object records its type (`GradientStop`, `NumberAnimation on
    // opacity`) and where it starts and ends, so every line knows the chain of
    // objects around it and the innermost object's whole body.
    const stack = [], opened = [];
    let text = '';
    maskedLines.forEach((line, index) => {
      within.push(stack.slice());
      for (let at = 0; at < line.length; ++at) {
        if (line[at] === '{') {
          const opener = /([A-Z]\w*(?:\s+on\s+[\w.]+)?)\s*$/.exec(text + line.slice(0, at));
          const entry = {type: opener ? opener[1] : '', start: index, end: maskedLines.length - 1};
          stack.push(entry); opened.push(entry);
        } else if (line[at] === '}') { const entry = stack.pop(); if (entry) entry.end = index; }
      }
      text = line;
    });
    const bodies = new Map();
    within.forEach((chain, index) => {
      // An animation's own body says what it drives (`property: "opacity"`),
      // read from the source, where that string survives masking. Only
      // animations carry one: they are small, and nothing else needs it.
      const own = chain.filter(entry => entry.type).slice(-1)[0];
      let body = '';
      if (own && /Animation\b|^(?:Binding|PropertyAction)\b/.test(own.type)) {
        if (!bodies.has(own)) bodies.set(own, lines.slice(own.start, own.end + 1).join('\n'));
        body = bodies.get(own);
      }
      within[index] = `${chain.map(entry => entry.type).filter(Boolean).join(' ')} ${body}`;
    });
  }
  const lineStarts = [0];
  for (let index = 0; index < text.length; ++index) if (text[index] === '\n') lineStarts.push(index + 1);
  const lineOf = offset => { let low = 0, high = lineStarts.length - 1; while (low < high) { const mid = (low + high + 1) >> 1; if (lineStarts[mid] <= offset) low = mid; else high = mid - 1; } return low; };
  // An artwork region exempts drawing coordinates only; its colours, enums,
  // colour arithmetic and motion are still the contract's.
  const flag = (index, message, coordinate = false) => {
    const key = `${file}:${index + 1}:${message.replace(/^.*literal /, '')}`;
    if ((coordinate && skip.has(index)) || found.has(key)) return;
    found.add(key);
    violations.push(`${file}:${index + 1}: ${message} -- ${lines[index].trim().slice(0, 140)}`);
  };

  for (const string of strings) {
    const index = lineOf(string.offset);
    if (/^#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(string.value)) flag(index, `colour literal "${string.value}"`);
    else if (namedColours.test(string.value)) {
      // A role name compared or passed by value is data; a named colour bound
      // where a colour is drawn is a colour literal.
      // The statement the string belongs to decides: a binding or assignment
      // of any styling property, such as `fillStyle = "red"`, or a line that
      // continues a colour binding.
      const before = maskedLines[index].slice(0, string.offset - lineStarts[index]);
      const head = /^\s*(?:(?:readonly |required |default )*property\s+([\w<>.]+)\s+)?((?:[A-Za-z_]\w*\.)*[A-Za-z_]\w*)\s*(?::|=(?![=>]))/.exec(before.split(/[;{]/).pop());
      const drawn = head ? head[1] === 'color' || isStyling(head[2], `${maskedLines[index]} ${within[index]}`) : /(?:color|Color|tint|Tint|Style)\b/.test(maskedLines[index]);
      if (drawn && !/(?:[!=]==?|\(|,)\s*$/.test(before)) flag(index, `named colour "${string.value}"`);
    }
  }
  maskedLines.forEach((line, index) => {
    for (const match of line.matchAll(/\b(?:Easing|Font)\.[A-Z]\w*/g)) flag(index, `${match[0]} belongs in the contract`);
    // Colour arithmetic on a literal makes a colour the contract never named.
    for (const match of line.matchAll(/\b(alpha|layeredColor|Qt\.rgba|Qt\.hsla|Qt\.hsva|Qt\.lighter|Qt\.darker|Qt\.tint)\(/g)) {
      let depth = 0, end = match.index + match[0].length - 1;
      for (; end < line.length; ++end) { if (line[end] === '(') depth += 1; else if (line[end] === ')' && --depth === 0) break; }
      const inner = line.slice(match.index + match[0].length, end);
      for (const literal of inner.matchAll(numberLiteral())) {
        const value = literalValue(literal[1]);
        if (value !== 0 && value !== 1) flag(index, `${match[1]} with a literal ${literal[1]}`);
      }
    }
  });
  // Bindings and assignments of styling properties, followed across lines
  // while their brackets are open.
  // Numeric constants declared under names that are not styling, so a styling
  // binding that reads one is judged as if it held the number itself.
  const constants = new Map(), colours = new Map();
  // A declaration whose value starts on the next line, `property int gap:`
  // then `456`, is read as one line by every pass below.
  // It keeps reading while its parentheses or brackets are open, or while it
  // ends on an operator, so `property int gap:` then `(`, `456`, `)` is one
  // initializer. `lastLine` is where it ends.
  const declares = /\bproperty\s+[\w<>.]+\s+\w+\s*:|\b(?:const|let|var)\s+\w+\s*=/;
  const unclosed = text => (text.match(/[([]/g) || []).length - (text.match(/[)\]]/g) || []).length;
  const lastLine = index => {
    const start = declares.exec(maskedLines[index]);
    if (!start) return index;
    let text = maskedLines[index].slice(start.index + start[0].length), at = index;
    while (at + 1 < maskedLines.length && (!text.trim() || unclosed(text) > 0 || /(?:[-+*/%?:<>]|\|\||&&)\s*$/.test(text))) text += ' ' + maskedLines[++at].trim();
    return at;
  };
  const logical = index => maskedLines.slice(index, lastLine(index) + 1).map((part, row) => row ? part.trim() : part).join(' ');
  maskedLines.forEach((physical, index) => {
    const line = logical(index);
    const end = lineStarts[lastLine(index) + 1] ?? masked.length;
    // The string a declaration holds is the first one after its `:` or `=`.
    const stringAfter = at => strings.find(entry => entry.offset > lineStarts[index] + at && entry.offset < end);
    const declared = new RegExp(`^\\s*(?:readonly\\s+)?property\\s+(?:int|real|double|var)\\s+(\\w+)\\s*:\\s*(-?)(${numberSource})\\s*;?\\s*$`).exec(line);
    if (declared && !isStyling(declared[1], line)) constants.set(declared[1], {index, text: declared[2] + declared[3], value: literalValue(declared[2] + declared[3])});
    // The same in JavaScript: `const localWidth = 456`, `let paint = "red"`.
    for (const local of line.matchAll(new RegExp(String.raw`\b(?:const|let|var)\s+(\w+)\s*=\s*(-?)(${numberSource})\s*(?=;|$|,)`, 'g')))
      if (!isStyling(local[1], line)) constants.set(local[1], {index, text: local[2] + local[3], value: literalValue(local[2] + local[3])});
    for (const local of line.matchAll(/\b(?:const|let|var)\s+(\w+)\s*=\s*(["'])_*\2\s*(?=;|$|,)/g)) {
      const string = stringAfter(local.index + local[0].indexOf('='));
      if (string && isColourLiteral(string.value)) colours.set(local[1], {index, text: string.value});
    }
    // A colour name kept in a neutral string, `property string paint: "red"`.
    const held = /^\s*(?:readonly\s+)?property\s+(?:string|var)\s+(\w+)\s*:\s*(["'])_*\2\s*;?\s*$/.exec(line);
    if (held) {
      const string = stringAfter(line.indexOf(':', line.indexOf(held[1])));
      if (string && isColourLiteral(string.value)) colours.set(held[1], {index, text: string.value});
    }
  });
  // A property the file assigns is state that starts at that value, such as a
  // pointer position or a chosen duration, not a constant.
  const dropState = () => {
    for (const [table, name] of [...[...constants.keys()].map(name => [constants, name]), ...[...colours.keys()].map(name => [colours, name])])
      if (new RegExp(String.raw`(?<![\w])(?<!\b(?:const|let|var)\s+)(?:[A-Za-z_]\w*\.)?${name}\s*(?:\*\*|<<|>>>|>>|&&|\|\||\?\?|[-+*/%&|^])?=(?![=>])`).test(masked)) table.delete(name);
  };
  dropState();
  // An alias of a constant, `property int b: a`, holds the same value, and
  // arithmetic on literals and constants alone, `property int m: 100 + 23` or
  // `const m = a * 2`, is a constant too. This runs to a fixed point through
  // any number of renames. Anything that reads a slot or live data is not.
  const pure = text => {
    let rest = text.replace(/\bMath\.(?:round|floor|ceil|max|min|abs|sqrt|pow)\b/g, ' ');
    for (const name of [...constants.keys(), ...colours.keys()]) rest = rest.replace(new RegExp(String.raw`(?<![\w.])(?:[A-Za-z_]\w*\.)?${name}\b`, 'g'), ' ');
    return rest.replace(numberLiteral(), ' ').replace(/[-+*/%(),\s]/g, '') === '';
  };
  for (let grew = true; grew;) {
    grew = false;
    maskedLines.forEach((physical, index) => {
      const line = logical(index);
      const declaration = /^\s*(?:readonly\s+)?property\s+(?:int|real|double|var|string)\s+(\w+)\s*:\s*(.+?)\s*;?\s*$/.exec(line)
        || /\b(?:const|let|var)\s+(\w+)\s*=\s*([^;]+?)\s*(?:;|$)/.exec(line);
      if (!declaration || isStyling(declaration[1], line) || constants.has(declaration[1]) || colours.has(declaration[1])) return;
      const [, name, value] = declaration;
      const alias = /^(?:[A-Za-z_]\w*\.)?([A-Za-z_]\w*)$/.exec(value);
      if (alias && colours.has(alias[1])) { colours.set(name, colours.get(alias[1])); grew = true; return; }
      if (!pure(value)) return;
      const named = [...constants.keys()].filter(other => new RegExp(String.raw`(?<![\w])(?:[A-Za-z_]\w*\.)?${other}\b`).test(value));
      const literal = [...value.matchAll(numberLiteral())].some(match => ![0, 1].includes(literalValue(match[1])));
      if (!named.length && !literal) return;
      if (alias && constants.has(alias[1])) { constants.set(name, constants.get(alias[1])); grew = true; return; }
      let evaluated = value.replace(/\bMath\./g, 'Math.');
      for (const other of named) evaluated = evaluated.replace(new RegExp(String.raw`(?<![\w])(?:[A-Za-z_]\w*\.)?${other}\b`, 'g'), `(${constants.get(other).value})`);
      let result = NaN;
      try { result = Function(`"use strict"; return (${evaluated.replace(/_/g, '')})`)(); } catch (error) { result = NaN; }
      constants.set(name, {index, text: value.trim(), value: Number(result)});
      grew = true;
    });
  }
  dropState();
  for (let index = 0; index < maskedLines.length; ++index) {
    const line = maskedLines[index];
    // A binding or assignment (plain or compound, `width += 1`) starts a line
    // or a statement, follows a control prefix (`if (...) width = 1`, `else`,
    // `=>`, a handler's colon), or is a
    // declaration, whose type is kept: a `color` property is drawn as a colour
    // whatever it is called.
    const heads = [...line.matchAll(/(?:^\s*|[;{(,]\s*|\)\s*|\belse\s+|=>\s*|\b(?:const|let|var)\s+|(?<=:)\s*(?=(?:[A-Za-z_]\w*\.)*[A-Za-z_]\w*\s*(?:\*\*|<<|>>>|>>|&&|\|\||\?\?|[-+*/%&|^])?=(?![=>]))|\bproperty\s+([\w<>.]+)\s+)((?:[A-Za-z_]\w*\.)*[A-Za-z_]\w*)\s*(:|(?:\*\*|<<|>>>|>>|&&|\|\||\?\?|[-+*/%&|^])?=(?![=>]))/g)]
      .map(match => Object.assign(match, {declared: match[1] || '', name: match[2], operator: match[3]}));
    // A member named in brackets, `item["width"] = 1`, is the same assignment.
    for (const match of line.matchAll(/\[\s*(["'])_*\1\s*\]\s*((?:\*\*|<<|>>>|>>|&&|\|\||\?\?|[-+*/%&|^])?=(?![=>]))/g)) {
      const string = strings.find(entry => entry.offset === lineStarts[index] + match.index + match[0].indexOf(match[1]));
      if (string) heads.push(Object.assign(match, {declared: '', name: string.value, operator: match[2]}));
    }
    for (const head of heads) {
      // A Binding or PropertyAction sets the property it names: its `value`
      // is judged as that property, `Binding { property: "width"; value: 1 }`.
      // The object may open on this very line, `Binding { ...; value: 1 }`.
      // The head's own match can include the `{` it follows, so read up to the name.
      const opened = /([A-Z]\w*)\s*\{[^{}]*$/.exec(line.slice(0, head.index + head[0].lastIndexOf(head.name)));
      const setter = opened ? /^(?:Binding|PropertyAction)$/.test(opened[1]) : /\b(?:Binding|PropertyAction)\b/.test(within[index].split('\n')[0]);
      const target = head.name === 'value' && setter
        ? (/\bproperty:\s*["']([\w.]+)["']/.exec(opened ? lines[index] : within[index]) || [])[1] : undefined;
      const property = target || head.name, colourTyped = head.declared === 'color';
      if (/^(?:property|readonly|required|signal|function|case|default|if|else|return|var|let|const)$/.test(property)) continue;
      if (head.operator === ':' && /\?\s*$/.test(line.slice(0, head.index + head[0].lastIndexOf(property)))) continue;
      if (!colourTyped && !isStyling(property, `${line} ${within[index]}`)) continue;
      const drawsColour = colourTyped || colourProperty.test(property);
      let expression = line.slice(head.index + head[0].length), depth = 0, cursor = index;
      // Where each line of the expression starts in the file, to find the
      // strings the expression holds.
      const segments = [lineStarts[index] + head.index + head[0].length];
      const cut = text => {
        let result = '';
        for (const char of text) {
          if ('([{'.includes(char)) depth += 1;
          else if (')]}'.includes(char)) { if (depth === 0) break; depth -= 1; }
          else if (char === ';' && depth === 0) break;
          result += char;
        }
        return result;
      };
      expression = cut(expression);
      // A binding continues while its brackets are open, while nothing has
      // followed its colon yet (`width:` with the value on the next line),
      // while its line ends on an operator (`a +`), and through lines that
      // carry on its expression (`? a`, `: b`, `+ c`).
      const continues = () => {
        if (cursor + 1 >= maskedLines.length) return false;
        if (depth > 0 || !expression.trim()) return true;
        // An operator left at the end of the line carries the expression on.
        if (/(?:[-+*/%?:<>]|\|\||&&|[!=]==?|[<>]=)\s*$/.test(expression)) return true;
        return /^\s*(?:\?|:(?!:)|\|\||&&|[-+*/%]|\.\w)/.test(maskedLines[cursor + 1]) && !/^\s*[A-Za-z_][\w.]*\s*:/.test(maskedLines[cursor + 1]);
      };
      while (continues()) { cursor += 1; segments.push(lineStarts[cursor]); expression += '\n' + cut(maskedLines[cursor]); }
      // Strings the binding draws: in a colour property any value but
      // "transparent" or "", elsewhere any colour name or hex code. A string that
      // is compared, indexed or passed as an argument is data, not a colour.
      expression.split('\n').forEach((part, row) => {
        for (const quoted of part.matchAll(/(["'`])_*\1/g)) {
          const string = strings.find(entry => entry.offset === segments[row] + quoted.index);
          if (!string) continue;
          const lead = part.slice(0, quoted.index);
          if (/(?:[!=]==?|[<>]=?|\[)\s*$/.test(lead) || (!drawsColour && /[(,]\s*$/.test(lead))) continue;
          // An empty string is no colour at all, such as a colour not chosen yet.
          if (drawsColour ? !['', 'transparent'].includes(string.value) : isColourLiteral(string.value))
            flag(index + row, `${property}: colour string "${string.value}"`);
        }
      });
      for (const literal of expression.matchAll(numberLiteral())) {
        const value = literalValue(literal[1]);
        if (structural(expression, literal.index, literal.index + literal[0].length, value, property, `${line} ${within[index]}`)) continue;
        const at = index + expression.slice(0, literal.index).split('\n').length - 1;
        flag(at, `${property}: literal ${literal[1]}`, isCoordinate(property));
      }
      if (drawsColour) for (const [name, held] of colours)
        // Used as an index, `theme[role]`, a role name is data, not a colour.
        if (new RegExp(String.raw`(?<![\w\[]\s*)(?:[A-Za-z_]\w*\.)?${name}\b(?!\s*[:(\]])`).test(expression))
          flag(held.index, `${name}: colour "${held.text}" reaches ${property}`);
      // A literal kept under a name of its own still styles what it reaches:
      // `property int arbitrary: 123` read by `width: arbitrary`.
      // It reaches the value when the value is made of it, contract slots and
      // arithmetic alone; combined with live data (`values.length`, `i`) the
      // constant is an input to a computation, not a style.
      const residue = name => expression
        .replace(/\b(?:[A-Za-z_]\w*\.)*(?:theme|root)\.\w+\b/g, ' ')
        .replace(/\bMath\.(?:round|floor|ceil|max|min|abs)\b/g, ' ')
        .replace(new RegExp(`(?<![\\w])(?:[A-Za-z_]\\w*\\.)?${name}\\b`, 'g'), ' ')
        .replace(numberLiteral(), ' ')
        .replace(/[-+*/%(),?:\s]/g, '');
      for (const [name, held] of constants) {
        if (residue(name) !== '') continue;
        for (const use of expression.matchAll(new RegExp(`(?<![\\w])(?:[A-Za-z_]\\w*\\.)?${name}\\b(?!\\s*[:(])`, 'g'))) {
          if (structural(expression, use.index, use.index + use[0].length, held.value, property, `${line} ${within[index]}`)) continue;
          flag(held.index, `${name}: literal ${held.text} reaches ${property}`, isCoordinate(property));
        }
      }
    }
  }
}

if (report) {
  console.log(violations.join('\n'));
  console.log(`\n${violations.length} styling literals outside the contract`);
  console.log(artwork.map(region => `artwork ${region.file}:${region.line}-${region.end}: ${region.reason}`).join('\n'));
}
if (violations.length) {
  if (!report) console.error(violations.slice(0, 20).join('\n') + (violations.length > 20 ? `\n... and ${violations.length - 20} more (--report lists them)` : ''));
  console.error(`${violations.length} styling literals outside the theme contract`);
  process.exit(1);
}
console.log(`Theme contract: ${slots.size} slots in ${contract.groups.length} groups match Theme.qml; ${published.themes.length} presets validate; no styling literal outside the contract (${artwork.length} artwork regions)`);
