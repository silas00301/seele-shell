#!/usr/bin/env node
// Pixel baselines of every existing theme on the shell's main surfaces.
//
//   node tests/theme-snapshots.js <projects/shell> <Qt QML imports> <Seele.Core QML imports> \
//     <font directory> <grain.png> [--update] [--keep <directory>] [--catalog <seele-theme list JSON>]
//     [--baseline <directory>]
//
// The production shell.qml is lifted rather than copied by hand: every root
// property, function and inline component, and each surface's whole layer
// window, are taken from the source being checked. Only what Quickshell owns
// -- layer placement, processes, files, Hyprland, Mpris, the tray, icon
// lookup -- is replaced by the fixtures beside this file, so a theme change
// anywhere in the shell's own QML or in the native presentation policy reaches
// the pixels. Every theme is rendered on every surface and compared with the
// committed PNGs pixel for pixel; any difference fails. `--update` rewrites the
// baselines instead. A drop-in theme folder's document (folders.json) is
// rendered on the two Themes surfaces, the places a registered theme has to
// appear. See theme-snapshots/README.md.
'use strict';
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const zlib = require('node:zlib');
const {spawnSync} = require('node:child_process');

const assets = path.join(__dirname, 'theme-snapshots');

function usage(message) {
  console.error(message);
  console.error('usage: node tests/theme-snapshots.js <projects/shell> <qt-qml-imports> <seele-core-qml-imports> <font-dir> <grain.png> [--update] [--keep <dir>] [--catalog <file>] [--baseline <dir>]');
  process.exit(2);
}
const positional = [], options = {update: false, keep: '', catalog: '', baseline: ''};
for (let index = 2; index < process.argv.length; ++index) {
  const argument = process.argv[index];
  if (argument === '--update') options.update = true;
  else if (argument === '--keep' || argument === '--catalog' || argument === '--baseline') {
    if (index + 1 >= process.argv.length) usage(`${argument} needs a path`);
    options[argument.slice(2)] = path.resolve(process.argv[++index]);
  } else if (argument.startsWith('--')) usage(`unknown option ${argument}`);
  else positional.push(argument);
}
if (positional.length !== 5) usage('expected five paths');
const [source, qtImport, coreImport, fontDir, grain] = positional.map(item => path.resolve(item));
// The committed baselines, or a directory recorded elsewhere: comparing a
// change under another renderer means recording its base there first.
const baselineDir = options.baseline || path.join(assets, 'baseline');
const manifestPath = path.join(options.baseline || assets, 'manifest.json');
// --update replaces a directory of earlier snapshots, never an arbitrary one.
if (options.update && options.baseline && fs.existsSync(baselineDir) && fs.readdirSync(baselineDir).length > 0 && !fs.existsSync(manifestPath))
  usage(`${baselineDir} is not empty and holds no snapshot manifest`);

// The surfaces, each one production layer window. `state` is the root state
// the shell is in while that window is shown; `target` names the item inside
// the window that is captured when it is not the whole window.
const surfaces = [
  {name: 'bar', namespace: 'seele-shell-bar', target: 'barSurface'},
  {name: 'control-center', namespace: 'seele-shell-control-center', state: {controlPanel: 'control-center'}},
  {name: 'notifications', namespace: 'seele-shell-notifications', id: 'notificationPopupWindow'},
  {name: 'notification-center', namespace: 'seele-shell-notifications', id: 'notificationWindow', state: {controlPanel: 'notifications'}},
  {name: 'media', namespace: 'seele-shell-media', state: {controlPanel: 'media'}},
  {name: 'audio', namespace: 'seele-shell-audio', state: {controlPanel: 'audio'}},
  {name: 'session', namespace: 'seele-shell-session', state: {controlPanel: 'system'}},
  {name: 'osd', namespace: 'seele-shell-osd', state: {osdOpen: true, osdKind: 'volume'}},
  {name: 'theme-switcher', namespace: 'seele-shell-themes', state: {themesOpen: true}},
  {name: 'theme-settings', namespace: 'seele-shell-theme-settings', state: {controlPanel: 'themes'}},
];
// Where a registered theme folder is drawn: in the switcher and in the
// Control Center's Themes panel, beside every preset. A folder the shell
// ships (registered.json) is a theme like a preset, and drawn everywhere.
const folderSurfaces = ['theme-switcher', 'theme-settings'];
const surfacesOf = theme => theme.folder && !theme.shipped ? surfaces.filter(surface => folderSurfaces.includes(surface.name)) : surfaces;
function counted() {
  const folders = themes.filter(theme => theme.folder && !theme.shipped).length;
  return `${themes.length - folders} themes on ${surfaces.length} surfaces and ${folders} theme folder${folders === 1 ? '' : 's'} on ${folderSurfaces.length}`;
}

// ---------------------------------------------------------------- PNG codec
// Qt writes the captures and Node has no image library, so this reads the
// non-interlaced 8-bit greyscale, RGB and RGBA files Qt produces and writes
// RGBA. Pixels, not file bytes, are compared: a different zlib may compress
// the same image differently.
const pngSignature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
function decodePng(file) {
  const data = fs.readFileSync(file);
  assert.ok(data.subarray(0, 8).equals(pngSignature), `${file} is not a PNG`);
  let offset = 8, header = null;
  const idat = [];
  while (offset < data.length) {
    const length = data.readUInt32BE(offset), type = data.toString('latin1', offset + 4, offset + 8);
    const body = data.subarray(offset + 8, offset + 8 + length);
    if (type === 'IHDR') header = {width: body.readUInt32BE(0), height: body.readUInt32BE(4), depth: body[8], color: body[9], interlace: body[12]};
    else if (type === 'IDAT') idat.push(body);
    else if (type === 'IEND') break;
    offset += 12 + length;
  }
  assert.ok(header, `${file} has no header`);
  const channels = {0: 1, 2: 3, 4: 2, 6: 4}[header.color];
  assert.ok(channels && header.depth === 8 && header.interlace === 0, `${file}: unsupported PNG layout`);
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = header.width * channels, pixels = Buffer.alloc(header.width * header.height * 4);
  let previous = Buffer.alloc(stride);
  for (let y = 0; y < header.height; ++y) {
    const filter = raw[y * (stride + 1)], line = Buffer.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    for (let x = 0; x < stride; ++x) {
      const left = x >= channels ? line[x - channels] : 0, up = previous[x], corner = x >= channels ? previous[x - channels] : 0;
      let predictor = 0;
      if (filter === 1) predictor = left;
      else if (filter === 2) predictor = up;
      else if (filter === 3) predictor = (left + up) >> 1;
      else if (filter === 4) {
        const estimate = left + up - corner, a = Math.abs(estimate - left), b = Math.abs(estimate - up), c = Math.abs(estimate - corner);
        predictor = a <= b && a <= c ? left : b <= c ? up : corner;
      } else assert.equal(filter, 0, `${file}: unknown PNG filter`);
      line[x] = (line[x] + predictor) & 0xff;
    }
    for (let x = 0; x < header.width; ++x) {
      const from = x * channels, to = (y * header.width + x) * 4;
      if (channels >= 3) line.copy(pixels, to, from, from + 3);
      else pixels.fill(line[from], to, to + 3);
      pixels[to + 3] = channels === 4 ? line[from + 3] : channels === 2 ? line[from + 1] : 0xff;
    }
    previous = line;
  }
  return {width: header.width, height: header.height, pixels};
}
function crc32(buffer) {
  let crc = ~0;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; ++bit) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return ~crc >>> 0;
}
function encodePng(image) {
  const chunk = (type, body) => {
    const length = Buffer.alloc(4), crc = Buffer.alloc(4), named = Buffer.concat([Buffer.from(type, 'latin1'), body]);
    length.writeUInt32BE(body.length);
    crc.writeUInt32BE(crc32(named));
    return Buffer.concat([length, named, crc]);
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(image.width, 0);
  header.writeUInt32BE(image.height, 4);
  header[8] = 8; header[9] = 6;
  const rows = [];
  for (let y = 0; y < image.height; ++y) rows.push(Buffer.from([0]), image.pixels.subarray(y * image.width * 4, (y + 1) * image.width * 4));
  return Buffer.concat([pngSignature, chunk('IHDR', header), chunk('IDAT', zlib.deflateSync(Buffer.concat(rows))), chunk('IEND', Buffer.alloc(0))]);
}
// Changed pixels in magenta over a faded copy of the baseline.
function difference(expected, actual) {
  const width = Math.max(expected.width, actual.width), height = Math.max(expected.height, actual.height);
  const pixels = Buffer.alloc(width * height * 4);
  let changed = 0, left = width, top = height, right = -1, bottom = -1;
  const at = (image, x, y) => x < image.width && y < image.height ? image.pixels.subarray((y * image.width + x) * 4, (y * image.width + x) * 4 + 4) : null;
  for (let y = 0; y < height; ++y) for (let x = 0; x < width; ++x) {
    const a = at(expected, x, y), b = at(actual, x, y), to = (y * width + x) * 4;
    if (a && b && a.equals(b)) {
      pixels[to] = pixels[to + 1] = pixels[to + 2] = Math.round((a[0] + a[1] + a[2]) / 3);
      pixels[to + 3] = Math.round(a[3] * 0.3);
    } else {
      pixels[to] = 0xff; pixels[to + 1] = 0; pixels[to + 2] = 0xff; pixels[to + 3] = 0xff;
      changed += 1; left = Math.min(left, x); top = Math.min(top, y); right = Math.max(right, x); bottom = Math.max(bottom, y);
    }
  }
  return {image: {width, height, pixels}, changed, box: changed ? `${left},${top} to ${right},${bottom}` : ''};
}

// ------------------------------------------------------- lifting shell.qml
// shell.qml keeps a strict layout: the root's members start two spaces in, a
// window's members six. A member runs until the next line at its own depth,
// and comment lines directly above a member belong to it.
function members(lines, from, to, indent) {
  const opener = new RegExp(`^ {${indent}}\\S`), closer = new RegExp(`^ {${indent}}[}\\])]`), comment = new RegExp(`^ {${indent}}//`);
  const result = [];
  let current = null, pending = [];
  for (let index = from; index < to; ++index) {
    const line = lines[index];
    if (opener.test(line) && !closer.test(line)) {
      if (comment.test(line)) { pending.push(line); continue; }
      current = {head: line.trim(), lines: [...pending, line]};
      pending = [];
      result.push(current);
    } else if (current) {
      if (pending.length) pending.push(line); else current.lines.push(line);
    }
  }
  return result;
}
function nameOf(member) {
  const match = /^(?:readonly |required |default )*property\s+[\w<>.]+\s+(\w+)|^function\s+(\w+)|^component\s+(\w+)|^id:\s*(\w+)/.exec(member.head);
  return match ? match.slice(1).find(Boolean) : '';
}
function kindOf(member) {
  if (/^(?:readonly |required |default )*property\s/.test(member.head)) return 'property';
  if (member.head.startsWith('function ')) return 'function';
  if (member.head.startsWith('component ')) return 'component';
  if (member.head.startsWith('signal ')) return 'signal';
  if (/^on[A-Z]\w*\s*:|^Component\.on/.test(member.head)) return 'handler';
  if (member.head.startsWith('id:')) return 'id';
  return 'object';
}
function objectId(member) {
  const match = /^\s+id:\s*(\w+)\s*$/m.exec(member.lines.join('\n'));
  return match ? match[1] : '';
}

// Members of the production root the harness replaces. HoverTip is a
// Quickshell popup window; `now` is pinned so every clock and age reads the
// same on every run, and `agentData` is what CodexBar would have reported.
const replacedMembers = new Set(['HoverTip', 'now', 'agentData']);
// Production objects the harness keeps: they read only fixtures.
const keptObjects = new Set(['spotifyMediaSlot', 'deviceMediaSlot', 'micTest', 'themeSounds']);
// Window members that place a Quickshell layer surface. An Item has no such
// properties, and the harness shows each surface itself.
const placement = /^(screen|visible|anchors|margins(\.\w+)?|exclusionMode|exclusiveZone|color|mask|focusable|aboveWindows|implicitWidth|implicitHeight|onVisibleChanged|WlrLayershell\.\w+)\s*[:{]/;

function liftWindow(shellLines, surface) {
  const marker = `WlrLayershell.namespace: "${surface.namespace}"`;
  const candidates = [];
  shellLines.forEach((line, index) => { if (line.trim() === marker) candidates.push(index); });
  const windows = candidates.map(index => {
    let start = index;
    while (start >= 0 && shellLines[start] !== '    PanelWindow {') --start;
    let end = start;
    while (shellLines[end] !== '    }') ++end;
    return {start, end};
  }).filter(window => window.start >= 0);
  const chosen = windows.filter(window => !surface.id || shellLines.slice(window.start, window.end).some(line => line.trim() === `id: ${surface.id}`));
  assert.equal(chosen.length, 1, `exactly one production window for ${surface.name}`);
  const {start, end} = chosen[0];
  const parts = members(shellLines, start + 1, end, 6);
  const body = [], sizes = {};
  for (const member of parts) {
    const head = member.head;
    if (/^implicit(Width|Height)\s*:/.test(head)) sizes[head.split(':')[0].trim()] = member.lines.join('\n');
    if (head === 'required property var modelData') body.push('      property var modelData: Quickshell.screens[0]');
    else if (!placement.test(head)) body.push(...member.lines);
  }
  // A window sizes to its content or to its output, as Quickshell sizes it.
  const width = sizes.implicitWidth ? sizes.implicitWidth : '      implicitWidth: modelData.width';
  const height = sizes.implicitHeight ? sizes.implicitHeight : '      implicitHeight: modelData.height';
  return [
    `  Component {`,
    `    id: ${surface.name.replace(/-(\w)/g, (_, letter) => letter.toUpperCase())}Surface`,
    `    Item {`,
    width, height,
    `      width: implicitWidth`,
    `      height: implicitHeight`,
    `      readonly property Item snapshotTarget: ${surface.target || 'null'}`,
    ...body,
    `    }`,
    `  }`,
  ].join('\n');
}

// Quickshell imports become the fixtures of the same name; everything else
// in the production import block is kept.
const fixtureModules = ['Quickshell', 'Hyprland', 'SystemTray', 'Mpris', 'MprisLoopState', 'MprisPlaybackState', 'DesktopEntries'];
function rewriteImports(text) {
  return text
    .replace(/^import Quickshell\s*$/m, 'import "fixture/Quickshell.js" as Quickshell')
    .replace(/^import Quickshell\..*\n/gm, '')
    .replace(/import "\.\.\/shared"/g, 'import "shared"')
    .replace(/"\.\.\/shared\//g, '"shared/');
}

function harness(shellText, scene, themes, output, contract) {
  const lines = shellText.split('\n');
  const rootStart = lines.indexOf('Shared.Theme {'), rootEnd = lines.lastIndexOf('}');
  assert.ok(rootStart > 0 && rootEnd > rootStart, 'shell.qml roots the shell in Shared.Theme');
  const imports = lines.slice(0, rootStart).filter(line => line.startsWith('import ') && !line.startsWith('import Quickshell'));
  const lifted = [];
  for (const member of members(lines, rootStart + 1, rootEnd, 2)) {
    const kind = kindOf(member), name = nameOf(member);
    if (kind === 'object' ? keptObjects.has(objectId(member)) : ['property', 'function', 'component'].includes(kind) && !replacedMembers.has(name))
      lifted.push(member.lines.join('\n'));
  }
  for (const name of keptObjects) assert.ok(lifted.some(text => text.includes(`id: ${name}`)), `production object ${name}`);
  const windows = surfaces.map(surface => liftWindow(lines, surface)).join('\n\n');
  const fixtures = fixtureModules.map(name => `import "fixture/${name}.js" as ${name}`);
  const replace = (text, marker, value) => {
    assert.ok(text.includes(marker), `scene marker ${marker}`);
    return text.replace(marker, () => value);
  };
  let result = scene;
  result = replace(result, '// @imports@', [...imports.map(rewriteImports), ...fixtures].join('\n'));
  result = replace(result, '// @production@', lifted.join('\n\n'));
  result = replace(result, '// @surfaces@', windows);
  result = replace(result, '"@surfaces@"', JSON.stringify(surfaces.map(surface => ({name: surface.name, state: surface.state || {}}))));
  result = replace(result, '"@themes@"', JSON.stringify(themes));
  result = replace(result, '"@folderSurfaces@"', JSON.stringify(folderSurfaces));
  result = replace(result, '"@contract@"', JSON.stringify(contract));
  result = replace(result, '"@output@"', JSON.stringify(output));
  return result;
}

// ---------------------------------------------------------------- the run
function sha256(file) { return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex'); }
const fonts = Object.fromEntries(fs.readdirSync(fontDir).filter(file => /\.(ttf|otf)$/i.test(file)).sort().map(file => [file, sha256(path.join(fontDir, file))]));
assert.ok(Object.keys(fonts).length > 0, `no fonts in ${fontDir}`);
const manifest = fs.existsSync(manifestPath) ? JSON.parse(fs.readFileSync(manifestPath, 'utf8')) : null;
if (!options.update) {
  assert.ok(manifest, 'no baseline manifest: run with --update to record one');
  // Text is most of every surface. A different font build moves every glyph,
  // which would bury a real change under noise, so it is refused outright.
  assert.deepEqual(fonts, manifest.fonts, `${fontDir} is not the font build the baselines were rendered with (see theme-snapshots/README.md)`);
}

const themeFile = JSON.parse(fs.readFileSync(path.join(assets, 'themes.json'), 'utf8'));
if (options.catalog) {
  // `seele-theme list` on the desktop carries every preset as the theme
  // document the shell receives, so it can prove the fixture still matches
  // what is installed, document for document. Only a colour's hex digits may
  // differ in case; names, the font family and every other string match
  // exactly.
  const listed = JSON.parse(fs.readFileSync(options.catalog, 'utf8')).themes;
  const color = /^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
  const shape = theme => JSON.parse(JSON.stringify(theme), (key, value) => typeof value === 'string' && color.test(value) ? value.toLowerCase() : value);
  assert.deepEqual(listed.map(shape), themeFile.themes.map(shape), 'themes.json no longer matches the installed catalog');
}
// Each preset is rendered from its contract document exactly as listed, and
// so are the folders the shell ships and the example theme folder, as the
// helper lists them once registered. A folder joins the catalog only while it
// is the one drawn, so the presets keep the catalog they were recorded with.
const folderFile = JSON.parse(fs.readFileSync(path.join(assets, 'folders.json'), 'utf8'));
const registeredFile = JSON.parse(fs.readFileSync(path.join(assets, 'registered.json'), 'utf8'));
// A file a shipped folder names -- its typeface, its texture, its wallpaper
// -- is listed in registered.json where it lies in the repository,
// `themes/<id>/<name>`, rather than at the store path it is registered to;
// the scene reads it there.
const repository = path.resolve(source, '../..');
const sharedSource = fs.existsSync(path.join(source, 'shared')) ? path.join(source, 'shared') : path.resolve(source, '../shared');
const fileSlots = new Set(JSON.parse(fs.readFileSync(path.join(sharedSource, 'theme-contract.json'), 'utf8')).groups
  .flatMap(group => group.slots).filter(slot => slot.type === 'path').map(slot => slot.name));
const inRepository = theme => Object.fromEntries(Object.entries(theme).map(([key, value]) =>
  [key, fileSlots.has(key) && typeof value === 'string' && /^themes\//.test(value) ? path.join(repository, value) : value]));
for (const theme of registeredFile.themes) for (const [key, value] of Object.entries(inRepository(theme)))
  if (fileSlots.has(key) && value !== '') assert.ok(fs.existsSync(value), `${theme.id}: ${key} names ${value}, which is not in the repository`);
const themes = [{id: 'fallback', fallback: true}, ...themeFile.themes,
  ...registeredFile.themes.map(theme => ({...inRepository(theme), folder: true, shipped: true})),
  ...folderFile.themes.map(theme => ({...theme, folder: true}))];

const work = fs.mkdtempSync(path.join(os.tmpdir(), 'seele-theme-snapshots-'));
try {
  const stage = path.join(work, 'shell'), output = path.join(work, 'output');
  fs.mkdirSync(stage, {recursive: true});
  fs.mkdirSync(output);
  // The installed layout: shell files beside `shared/`, as the package and
  // tests/qml-fixture.sh arrange them, with the generated grain film in it.
  for (const file of fs.readdirSync(source)) {
    if (!/\.(qml|js|svg)$/.test(file)) continue;
    const text = fs.readFileSync(path.join(source, file), 'utf8');
    fs.writeFileSync(path.join(stage, file), file.endsWith('.svg') ? text : rewriteImports(text));
  }
  const shared = fs.existsSync(path.join(source, 'shared')) ? path.join(source, 'shared') : path.resolve(source, '../shared');
  fs.cpSync(shared, path.join(stage, 'shared'), {recursive: true});
  fs.copyFileSync(grain, path.join(stage, 'shared/grain.png'));
  // The package installs the launcher's mark beside shell.qml for the bar.
  fs.copyFileSync(path.resolve(source, '../vicinae/seele.svg'), path.join(stage, 'seele.svg'));
  // Theme.qml without Quickshell's config IO, exactly as the other Qt
  // fixtures strip it: each theme reaches its applyTheme directly instead,
  // with the slots Theme.qml would read from the contract beside it.
  const themePath = path.join(stage, 'shared/Theme.qml');
  const theme = fs.readFileSync(themePath, 'utf8').split('  FileView {')[0]
    .replace(/import Quickshell.*\n/g, '').replace('ShellRoot {', 'Item {')
    .replace(/Quickshell.env\("SEELE_SHELL_WALLPAPER"\) \|\| /, '');
  fs.writeFileSync(themePath, theme + '}\n');
  for (const file of fs.readdirSync(path.join(assets, 'fixture'))) {
    const into = file.endsWith('.qml') ? stage : path.join(stage, 'fixture');
    fs.mkdirSync(into, {recursive: true});
    fs.copyFileSync(path.join(assets, 'fixture', file), path.join(into, file));
  }
  const shellText = fs.readFileSync(path.join(source, 'shell.qml'), 'utf8');
  const scene = fs.readFileSync(path.join(assets, 'scene.qml'), 'utf8');
  const contract = JSON.parse(fs.readFileSync(path.join(shared, 'theme-contract.json'), 'utf8'));
  fs.writeFileSync(path.join(stage, 'tst_themesnapshots.qml'), harness(shellText, scene, themes, output, contract));
  // Qt saves into an existing directory only.
  for (const theme of themes) fs.mkdirSync(path.join(output, theme.id));

  // Only the pinned fonts, so no fallback from the rendering machine can
  // supply a glyph; and no user configuration of any kind. Text is rasterized
  // unhinted in greyscale: hinting is where FreeType releases differ (2.13.2
  // and 2.14.3 move up to a few hundred pixels per surface), while the plain
  // outline rasterizer gives the same pixels on both, so nixpkgs' FreeType
  // and a distribution's agree with the baselines.
  const home = path.join(work, 'home');
  fs.mkdirSync(home);
  const assign = (name, value) => `<edit name="${name}" mode="assign">${value}</edit>`;
  fs.writeFileSync(path.join(work, 'fonts.conf'), `<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig>
  <dir>${fontDir}</dir>
  <cachedir>${path.join(work, 'fontconfig')}</cachedir>
  <match target="font">
    ${assign('hinting', '<bool>false</bool>')}${assign('hintstyle', '<const>hintnone</const>')}${assign('autohint', '<bool>false</bool>')}
    ${assign('antialias', '<bool>true</bool>')}${assign('rgba', '<const>none</const>')}${assign('lcdfilter', '<const>lcdnone</const>')}
    ${assign('embeddedbitmap', '<bool>false</bool>')}
  </match>
</fontconfig>
`);
  const env = {
    PATH: process.env.PATH, HOME: home, XDG_CONFIG_HOME: path.join(home, '.config'), XDG_CACHE_HOME: path.join(home, '.cache'),
    XDG_RUNTIME_DIR: work, TZ: 'UTC', LANG: 'C.UTF-8', LC_ALL: 'C.UTF-8', FONTCONFIG_FILE: path.join(work, 'fonts.conf'),
    QT_QPA_PLATFORM: 'offscreen', QT_QUICK_BACKEND: 'software', QT_QUICK_CONTROLS_STYLE: 'Fusion',
    QT_ENABLE_HIGHDPI_SCALING: '0', QT_SCALE_FACTOR: '1', QML_IMPORT_PATH: coreImport, QML2_IMPORT_PATH: coreImport,
  };
  if (process.env.QT_PLUGIN_PATH) env.QT_PLUGIN_PATH = process.env.QT_PLUGIN_PATH;
  const run = spawnSync('qmltestrunner', ['-import', qtImport, '-import', coreImport, '-input', path.join(stage, 'tst_themesnapshots.qml')], {
    encoding: 'utf8', env, maxBuffer: 64 * 1024 * 1024, timeout: 15 * 60 * 1000,
  });
  if (run.error) throw run.error;
  const log = (run.stdout || '') + (run.stderr || '');
  // A warning means a binding the harness failed to satisfy, and the surface
  // it belongs to would be drawn from a default instead of from the theme.
  const warnings = log.split('\n').filter(line => /^(QWARN|QCRITICAL|QFATAL)|\bqml: .*(Error|warn)|ReferenceError|TypeError|is not a type|Cannot assign/.test(line));
  if (run.status !== 0 || warnings.length) {
    process.stdout.write(log);
    if (options.keep) {
      fs.cpSync(stage, path.join(options.keep, 'harness'), {recursive: true});
      fs.cpSync(output, path.join(options.keep, 'rendered'), {recursive: true});
      console.error(`Generated harness: ${path.join(options.keep, 'harness/tst_themesnapshots.qml')}`);
    }
    throw Error(warnings.length ? `the harness rendered with ${warnings.length} warnings` : `qmltestrunner exited with ${run.status}`);
  }
  const config = (/^Config: (.*)$/m.exec(log) || [, 'unknown'])[1];
  const renderer = {qt: config.replace(/, by .*/, '').replace(/^Using QtTest library [^,]+, /, ''), platform: 'offscreen', backend: 'software', style: 'Fusion', text: 'unhinted greyscale'};

  const expected = themes.flatMap(theme => surfacesOf(theme).map(surface => path.join(theme.id, `${surface.name}.png`))).sort();
  const rendered = expected.filter(file => fs.existsSync(path.join(output, file)));
  assert.deepEqual(rendered, expected, 'every theme rendered every surface');

  if (options.update) {
    fs.rmSync(baselineDir, {recursive: true, force: true});
    for (const file of expected) {
      fs.mkdirSync(path.dirname(path.join(baselineDir, file)), {recursive: true});
      fs.copyFileSync(path.join(output, file), path.join(baselineDir, file));
    }
    const record = {renderer, fonts, themes: themes.filter(theme => !theme.folder).map(theme => theme.id), surfaces: surfaces.map(surface => surface.name),
      folders: Object.fromEntries(themes.filter(theme => theme.folder).map(theme => [theme.id, surfacesOf(theme).map(surface => surface.name)]))};
    fs.writeFileSync(manifestPath, JSON.stringify(record, null, 2) + '\n');
    console.log(`Recorded ${expected.length} baselines: ${counted()} (${renderer.qt})`);
  } else {
    const failures = [];
    const committed = fs.existsSync(baselineDir) ? fs.readdirSync(baselineDir, {recursive: true}).filter(file => file.endsWith('.png')).sort() : [];
    for (const file of committed.filter(file => !expected.includes(file))) failures.push(`${file}: baseline for a theme or surface that is no longer rendered`);
    for (const file of expected) {
      const baseline = path.join(baselineDir, file);
      if (!fs.existsSync(baseline)) { failures.push(`${file}: no baseline`); continue; }
      const want = decodePng(baseline), got = decodePng(path.join(output, file));
      if (want.width === got.width && want.height === got.height && want.pixels.equals(got.pixels)) continue;
      const diff = difference(want, got);
      failures.push(`${file}: ${want.width}x${want.height} -> ${got.width}x${got.height}, ${diff.changed} pixels changed (${diff.box})`);
      if (options.keep) {
        const into = path.join(options.keep, path.dirname(file)), stem = path.basename(file, '.png');
        fs.mkdirSync(into, {recursive: true});
        fs.copyFileSync(path.join(output, file), path.join(into, `${stem}.actual.png`));
        fs.copyFileSync(baseline, path.join(into, `${stem}.baseline.png`));
        fs.writeFileSync(path.join(into, `${stem}.diff.png`), encodePng(diff.image));
      }
    }
    if (failures.length) {
      console.error(failures.join('\n'));
      if (JSON.stringify(renderer) !== JSON.stringify(manifest.renderer))
        console.error(`\nThe baselines were rendered by ${JSON.stringify(manifest.renderer)}; this run used ${JSON.stringify(renderer)}.\nRe-record them at the unchanged base revision with --update in this environment before comparing a change.`);
      if (options.keep) console.error(`\nActual, baseline and diff images: ${options.keep}`);
      process.exitCode = 1;
    } else {
      console.log(`${expected.length} snapshots match: ${counted()} are pixel-identical`);
    }
  }
} finally {
  fs.rmSync(work, {recursive: true, force: true});
}
