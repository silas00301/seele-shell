// The theme panel's store: what it publishes from a catalog, what it refuses
// to send, and what it says about a switch. Publication, reloading and every
// file a theme is made of belong to `seele-theme`; these are the guards that
// keep one open panel honest about which theme is actually applied.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const {nativeBridge} = require('./native-functions.cjs');

const source = fs.readFileSync(process.argv[2], 'utf8');
const panel = fs.readFileSync(process.argv[3], 'utf8');
const shell = fs.readFileSync(process.argv[4], 'utf8');

// The store's functions, braces counted rather than guessed, so a one-line
// method is extracted exactly like a block.
function methods(text) {
  const out = [];
  const starts = /^  function \w+\(/gm;
  let match;
  while ((match = starts.exec(text))) {
    let index = text.indexOf('{', match.index);
    let depth = 0;
    for (; index < text.length; index++) {
      if (text[index] === '{') depth++;
      else if (text[index] === '}' && --depth === 0) break;
    }
    out.push(text.slice(match.index, index + 1));
  }
  return out.join('\n');
}

const guards = [];
const state = vm.createContext({
  Bridge: nativeBridge(),
  catalog: {ok: false, current: '', themes: []},
  panelOpen: false, query: '', mode: 'all', columns: 4, highlighted: '',
  selection: '', selectionName: '', error: '', actionError: '', reloadPending: '', applying: '',
  list: {running: false},
  set: {running: false, command: []},
  guard: {restart() { guards.push('restart') }, stop() { guards.push('stop') }},
  JSON,
});
// The store's bindings, evaluated the way Qt would on every read.
Object.defineProperties(state, {
  themes: {get() { return state.catalog.themes || [] }},
  total: {get() { return state.themes.length }},
  current: {get() { return state.selection !== '' ? state.selection : state.catalog.current || '' }},
  busy: {get() { return state.applying !== '' || state.list.running }},
  filtered: {get() { return state.query !== '' || state.mode !== 'all' }},
  layout: {get() { return state.Bridge.call('themes.layout', [state.themes, state.current, state.query, state.mode, state.columns]) }},
  focusedId: {get() { return state.Bridge.call('themes.focus', [state.layout, state.highlighted, state.current]) }},
  preview: {get() { return state.Bridge.call('themes.find', [state.themes, state.focusedId]) }},
  store: {get() { return state }},
});
vm.runInContext(methods(source), state);

const palette = (seed) => {
  const theme = {};
  for (const [index, role] of ['base', 'mantle', 'crust', 'surface', 'overlay', 'text',
    'subtext', 'accent', 'red', 'green', 'yellow'].entries())
    theme[role] = `#${String(seed + index).padStart(2, '0').repeat(3)}`;
  return theme;
};
const preset = (id, name, mode, seed) => ({id, name, mode, ...palette(seed)});
const reply = {
  current: 'catppuccin-mocha',
  themes: [
    preset('catppuccin-mocha', 'Catppuccin Mocha', 'dark', 10),
    preset('catppuccin-latte', 'Catppuccin Latte', 'light', 20),
    preset('rose-pine', 'Rosé Pine', 'dark', 30),
    preset('rose-pine-dawn', 'Rosé Pine Dawn', 'light', 40),
    preset('nord', 'Nord', 'dark', 50),
  ],
};
// Each row as "Family: variant, variant", so a whole layout reads at once.
const shape = () => state.layout.rows.map(row =>
  `${row.family}: ${row.members.map(member => member.variant).join(', ')}`);

// A catalog is read and laid out; nothing is written and nothing runs.
state.accept(reply);
assert.equal(state.error, '');
assert.deepEqual(shape(), ['Catppuccin: Mocha, Latte', 'Rosé Pine: Rosé Pine, Dawn', 'More: Nord']);
assert.equal(state.layout.rows[0].members[0].current, true);
assert.equal(state.focusedId, 'catppuccin-mocha', 'the preview opens on the applied theme');
assert.equal(state.preview.name, 'Catppuccin Mocha');

// Moving is native: the store only remembers where the reader went.
state.step('right');
assert.equal(state.focusedId, 'catppuccin-latte');
state.step('down');
assert.equal(state.focusedId, 'rose-pine-dawn', 'down keeps the column');
state.step('down');
assert.equal(state.focusedId, 'nord', 'a shorter row takes its last tile');
state.step('down');
assert.equal(state.focusedId, 'nord', 'the last row holds');
state.highlight('rose-pine');
assert.equal(state.preview.name, 'Rosé Pine', 'pointing at a tile previews it');

// The mode and the search take tiles out; a highlight they hide falls back.
state.mode = 'light';
assert.deepEqual(shape(), ['Catppuccin: Latte', 'Rosé Pine: Dawn']);
assert.equal(state.focusedId, 'catppuccin-latte', 'a hidden highlight falls back to the first tile shown');
assert.equal(state.filtered, true);
state.mode = 'all';
state.query = 'nord';
assert.deepEqual(shape(), ['More: Nord']);
assert.equal(state.focusedId, 'nord');
state.query = 'solarized';
assert.deepEqual(shape(), []);
assert.equal(state.focusedId, '', 'nothing shown previews nothing');
assert.equal(state.preview, null);
state.resetFilters();
assert.equal(state.query, '');
assert.equal(state.mode, 'all');
assert.equal(state.filtered, false);
assert.deepEqual(shape(), ['Catppuccin: Mocha, Latte', 'Rosé Pine: Rosé Pine, Dawn', 'More: Nord']);

// The published selection wins over the reply, because it is what the desktop
// is actually wearing: a theme applied from the launcher marks its tile here.
state.highlighted = '';
state.selection = 'rose-pine-dawn';
assert.equal(state.focusedId, 'rose-pine-dawn');
assert.equal(state.layout.rows[1].members[1].current, true);
assert.equal(state.layout.rows[0].members[0].current, false);
state.selection = '';

// A malformed catalog keeps the one that was already read.
const kept = state.catalog;
for (const broken of [{current: 'nord', themes: []}, {current: 'nord', themes: [{id: 'x'}]}, {}]) {
  state.accept(broken);
  assert.equal(state.catalog, kept, 'an unusable reply replaces nothing');
  assert.match(state.error, /catalog could not be read|unavailable/);
  state.error = '';
}

// Applying is one reviewed ID at a time, and only an ID the catalog holds.
assert.equal(state.apply('not-a-theme'), false);
assert.match(state.actionError, /no longer in the catalog/);
assert.deepEqual([...state.set.command], [], 'a theme the catalog does not hold is never sent');
assert.equal(state.apply(''), false);
state.highlight('catppuccin-latte');
assert.equal(state.applyFocused(), true, 'Enter applies what the preview shows');
assert.deepEqual([...state.set.command], ['seele-theme', 'set', 'catppuccin-latte']);
assert.equal(state.set.running, true);
assert.equal(state.actionError, '');
assert.deepEqual(guards, ['restart']);
assert.equal(state.apply('nord'), false, 'a second choice cannot overtake the one in flight');
assert.deepEqual([...state.set.command], ['seele-theme', 'set', 'catppuccin-latte']);

// The reply says what could not be reloaded, never what is selected: that is
// read back from the helper's own published selection.
state.applied({id: 'catppuccin-latte', pending: ['Ghostty', 'tmux']});
assert.equal(state.reloadPending, 'Ghostty and tmux still show the previous palette.');
assert.equal(state.current, 'catppuccin-mocha', 'the request does not mark the tile; the selection file does');
state.applied({id: 'catppuccin-latte', pending: []});
assert.equal(state.reloadPending, '');

// Failure wording is the shared native map's, and says what happened to the
// selection rather than reporting a success.
assert.equal(state.failure(''), '');
assert.match(state.failure('unavailable'), /Nothing was changed/);
assert.match(state.failure('timeout'), /Refresh to see what is selected/);
assert.match(state.failure('whatever'), /Try again/);

// This surface reads a catalog and applies one theme. It writes no file, keeps
// no palette of its own and starts nothing else.
const commands = [...source.matchAll(/command = \[([^\]]*)\]|command: \[([^\]]*)\]/g)]
  .map(match => (match[1] ?? match[2]).replace(/\s+/g, ' ').trim());
assert.deepEqual(commands.sort(), ['"seele-theme", "list"', '"seele-theme", "set", id']);
assert.doesNotMatch(source, /atomicWrite|writeFile|\.write\(/, 'the store publishes nothing itself');
assert.match(source, /Bridge\.call\("themes\.selected"/, 'the published selection is parsed natively');
for (const policy of ['layout', 'focus', 'find', 'step'])
  assert.match(source, new RegExp(`Bridge\\.call\\("themes\\.${policy}"`), `${policy} is decided natively`);
assert.doesNotMatch(source, /ListModel|Models\.reconcile/, 'the layout is read whole, not mirrored into a model');
assert.match(source, /watchChanges: true/, 'the applied theme is watched rather than polled');
assert.doesNotMatch(panel, /Process\s*\{/, 'the panel starts no process of its own');
assert.doesNotMatch(panel, /#[0-9a-fA-F]{6}/, 'every colour the panel draws comes from a palette');

// Production wiring.
assert.match(shell, /ThemeStore \{\s*\n\s*id: themeStore/, 'the store is instantiated');
assert.match(shell, /panelOpen: root\.controlPanel === "themes"/, 'listing follows the open panel');
assert.match(shell, /ThemePanel \{ id: themesPanel; theme: root; store: themeStore/, 'tile and panel share one store');
assert.match(shell, /namespace: "seele-shell-themes"/);
assert.match(shell, /label: "Themes"/, 'the Control Center carries the module');
assert.match(shell, /root\.toggleControl\("themes"/, 'the tile opens the panel');
assert.match(shell, /detail: themeStore\.currentName/, 'the tile names the applied theme');
// The tile has a row of its own and the grid is tall enough to hold it. The
// row count is read from the grid rather than fixed here, so a module added
// later moves this assertion along with it instead of breaking it.
const grid = shell.slice(shell.indexOf('component ControlCenterGrid:'),
  shell.indexOf('component IconButton:', shell.indexOf('component ControlCenterGrid:')));
const [, gridRows, gridGaps] = grid.match(/height: devicesY \+ smallTileHeight \* (\d+) \+ gap \* (\d+)/).map(Number);
assert.equal(gridGaps, gridRows - 1, 'the grid spaces its own rows');
const tileRows = [...grid.matchAll(/y: controlGrid\.devicesY \+ controlGrid\.smallTileHeight \* (\d+) \+ controlGrid\.gap \* \1\b[\s\S]*?label: "([^"]+)"/g)]
  .map(match => ({row: Number(match[1]), label: match[2]}));
const themesRow = tileRows.find(tile => tile.label === 'Themes');
assert.ok(themesRow, 'the Themes tile sits on a grid row');
assert.deepEqual(tileRows.filter(tile => tile.row === themesRow.row).map(tile => tile.label), ['Themes'],
  'no other module shares the Themes row');
assert.ok(themesRow.row < gridRows, 'the grid is tall enough to show the Themes row');
assert.doesNotMatch(grid.slice(grid.indexOf('label: "Themes"') - 400, grid.indexOf('label: "Themes"') + 400),
  /\u{f03d8}/u, 'the palette mark belongs to Colour Lab; Themes carries its own');

console.log('Theme families, native movement and focus, filtering, selection marking, single-flight applies, reload reporting and production wiring passed');
