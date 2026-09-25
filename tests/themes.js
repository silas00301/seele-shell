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
const settle = {running: false, restart() { this.running = true }, stop() { this.running = false }};
const state = vm.createContext({
  Bridge: nativeBridge(),
  catalog: {ok: false, current: '', themes: []},
  panelOpen: false, query: '', mode: 'all', columns: 4, highlighted: '',
  desired: '', sent: '', opened: '',
  selection: '', selectionName: '', error: '', actionError: '', reloadPending: '', applying: '',
  list: {running: false},
  set: {running: false, command: []},
  settle,
  guard: {restart() { guards.push('restart') }, stop() { guards.push('stop') }},
  JSON,
});
// The store's bindings, evaluated the way Qt would on every read.
Object.defineProperties(state, {
  themes: {get() { return state.catalog.themes || [] }},
  total: {get() { return state.themes.length }},
  current: {get() { return state.selection !== '' ? state.selection : state.catalog.current || '' }},
  busy: {get() { return state.applying !== '' || state.list.running }},
  switching: {get() { return state.applying !== '' || state.settle.running }},
  filtered: {get() { return state.query !== '' || state.mode !== 'all' }},
  layout: {get() { return state.Bridge.call('themes.layout', [state.themes, state.current, state.query, state.mode, state.columns]) }},
  focusedId: {get() { return state.Bridge.call('themes.focus', [state.layout, state.highlighted, state.current]) }},
  openedName: {get() { return state.nameOf(state.opened) }},
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
const sent = () => [...state.set.command].slice(2);
// The helper answers the switch in flight, and the desktop publishes it.
function helperFinishes(code = 0) {
  if (code === 0) state.selection = state.applying;
  state.set.running = false;
  state.finished(code);
  state.list.running = false;
}

// A catalog is read and laid out; nothing is written and nothing runs.
state.accept(reply);
assert.equal(state.error, '');
assert.deepEqual(shape(), ['Catppuccin: Mocha, Latte', 'Rosé Pine: Rosé Pine, Dawn', 'More: Nord']);
assert.equal(state.focusedId, 'catppuccin-mocha', 'the ring opens on the applied theme');

// Opening remembers where the desktop started, so it is one step away later.
state.panelOpen = true;
state.openChanged();
assert.equal(state.opened, 'catppuccin-mocha');
assert.equal(state.openedName, 'Catppuccin Mocha');
state.list.running = false;

// A burst of arrow presses moves the ring at once but sends only its end.
state.step('right');
state.step('down');
assert.equal(state.focusedId, 'rose-pine-dawn', 'the ring follows every press');
assert.equal(state.settle.running, true, 'a keyboard move waits for the burst to settle');
assert.equal(state.set.running, false, 'nothing is sent mid-burst');
assert.equal(state.switching, true);
state.settle.running = false;
state.flush();
assert.deepEqual(sent(), ['rose-pine-dawn'], 'the burst sends its last move only');

// The reader keeps moving while that switch runs; the latest choice follows
// the moment it finishes, and the one in between is never sent.
state.step('left');
assert.equal(state.desired, 'rose-pine');
state.settle.running = false;
state.flush();
assert.deepEqual(sent(), ['rose-pine-dawn'], 'one switch runs at a time');
helperFinishes();
assert.equal(state.current, 'rose-pine-dawn');
assert.deepEqual(sent(), ['rose-pine'], 'the latest choice follows at once');
helperFinishes();
assert.equal(state.current, 'rose-pine');
assert.equal(state.switching, false);
state.flush();
assert.equal(state.set.running, false, 'what is applied is not sent again');

// Listing again after a switch never blocks the next one.
state.list.running = true;
state.choose('nord', true);
assert.deepEqual(sent(), ['nord'], 'a click switches at once, even while the catalog is re-read');
helperFinishes();

// A switch that failed is not taken for applied: choosing it again resends it.
state.choose('catppuccin-latte', true);
helperFinishes(1);
assert.match(state.actionError, /could not be applied/);
assert.equal(state.current, 'nord');
state.choose('catppuccin-latte', true);
assert.deepEqual(sent(), ['catppuccin-latte'], 'the failed choice can be sent again');
helperFinishes();

// Back returns to where the panel opened, at once.
state.revert();
assert.deepEqual(sent(), ['catppuccin-mocha']);
helperFinishes();
assert.equal(state.current, 'catppuccin-mocha');

// Closing in the middle of a burst keeps its last move rather than dropping it.
state.step('right');
assert.equal(state.set.running, false);
state.panelOpen = false;
state.openChanged();
assert.deepEqual(sent(), ['catppuccin-latte'], 'closing sends the settled-on choice');
assert.equal(state.highlighted, '', 'reopening starts from the applied theme');
helperFinishes();

// Unknown or empty choices are refused, and nothing is sent for them.
state.panelOpen = true;
state.openChanged();
state.list.running = false;
state.set.command = [];
state.choose('not-a-theme', true);
state.choose('', true);
assert.deepEqual([...state.set.command], [], 'a theme the catalog does not hold is never sent');
assert.equal(state.apply('not-a-theme'), false);
assert.match(state.actionError, /no longer in the catalog/);
state.actionError = '';

// The mode and the search take tiles out and move the ring without
// switching anything: only a move the reader made does.
state.mode = 'light';
assert.deepEqual(shape(), ['Catppuccin: Latte', 'Rosé Pine: Dawn']);
assert.equal(state.focusedId, 'catppuccin-latte');
state.query = 'solarized';
assert.deepEqual(shape(), []);
assert.equal(state.focusedId, '');
state.step('down');
assert.equal(state.settle.running, false, 'an empty grid has nowhere to move');
state.resetFilters();
assert.deepEqual([state.query, state.mode, state.filtered], ['', 'all', false]);
assert.deepEqual([...state.set.command], [], 'filtering never switched the desktop');

// The published selection, not the request, marks the applied tile.
assert.equal(state.layout.rows[0].members[1].current, true);
state.selection = 'nord';
assert.equal(state.layout.rows[2].members[0].current, true);

// A malformed catalog keeps the one that was already read.
const kept = state.catalog;
for (const broken of [{current: 'nord', themes: []}, {current: 'nord', themes: [{id: 'x'}]}, {}]) {
  state.accept(broken);
  assert.equal(state.catalog, kept, 'an unusable reply replaces nothing');
  assert.match(state.error, /catalog could not be read|unavailable/);
  state.error = '';
}

// The reply says what could not be reloaded, never what is selected.
state.applied({id: 'nord', pending: ['Ghostty', 'tmux']});
assert.equal(state.reloadPending, 'Ghostty and tmux still show the previous palette.');
state.applied({id: 'nord', pending: []});
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
for (const policy of ['layout', 'focus', 'step', 'name'])
  assert.match(source, new RegExp(`Bridge\\.call\\("themes\\.${policy}"`), `${policy} is decided natively`);
assert.doesNotMatch(source, /ListModel|Models\.reconcile/, 'the layout is read whole, not mirrored into a model');
assert.match(source, /watchChanges: true/, 'the applied theme is watched rather than polled');
assert.doesNotMatch(panel, /Process\s*\{/, 'the panel starts no process of its own');
assert.doesNotMatch(panel, /#[0-9a-fA-F]{6}/, 'every colour the panel draws comes from a palette');

// Production wiring: the picker floats on its own and closes nothing.
assert.match(shell, /ThemeStore \{\s*\n\s*id: themeStore/, 'the store is instantiated');
assert.match(shell, /panelOpen: root\.themesOpen/, 'listing follows the floating picker');
assert.match(shell, /ThemePanel \{\s*\n\s*id: themesPanel\n[\s\S]{0,200}?store: themeStore\n/, 'tile and picker share one store');
assert.match(shell, /onCloseRequested: root\.themesOpen = false/);
assert.match(shell, /namespace: "seele-shell-themes"/);
assert.match(shell, /label: "Themes"/, 'the Control Center carries the module');
assert.match(shell, /onActivated: root\.toggleThemes\(\)/, 'the tile opens the picker');
assert.match(shell, /function toggleThemes\(\): void \{ root\.toggleThemes\(\) \}/, 'shellctl can open it');
assert.match(shell, /detail: themeStore\.currentName/, 'the tile names the applied theme');
const body = name => {
  const start = shell.indexOf(`  function ${name}(`);
  return shell.slice(start, shell.indexOf('\n  }\n', start));
};
assert.doesNotMatch(body('toggleThemes'), /closeOverlays|controlPanel/, 'opening the picker closes no panel');
assert.doesNotMatch(body('closeOverlays'), /themesOpen/, 'closing the panels leaves the picker');
const floating = shell.slice(shell.indexOf('id: themesWindow'), shell.indexOf('ThemePanel {', shell.indexOf('id: themesWindow')));
assert.doesNotMatch(floating, /anchors \{|margins \{/, 'an unanchored layer surface is centred by the compositor');
assert.match(floating, /WlrLayershell\.layer: WlrLayer\.Overlay/, 'above the click-away catcher');
assert.doesNotMatch(panel, /store\.apply\(|component Scene|\"Apply\"/, 'the desktop is the preview; there is no apply step');

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

console.log('Theme families, instant coalesced switching, going back, filtering, selection marking and floating production wiring passed');
