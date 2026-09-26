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

const values = [];
const rows = {
  get count() { return values.length },
  get(i) { return values[i] },
  insert(i, v) { values.splice(i, 0, v) },
  move(i, j) { values.splice(j, 0, values.splice(i, 1)[0]) },
  setProperty(i, k, v) { values[i][k] = v },
  remove(i, n) { values.splice(i, n === undefined ? 1 : n) },
};
const commands = [];
const settle = {running: false, restart() { this.running = true }, stop() { this.running = false }};
const state = vm.createContext({
  Bridge: nativeBridge(),
  Models: require('./list-models.cjs')(),
  rows,
  catalog: {ok: false, current: '', themes: []},
  appearance: {mode: 'dark', dark: '', light: '', auto: {source: 'off', lightAt: '07:00', darkAt: '19:00'}, next: null},
  panelOpen: false, query: '', mode: 'dark', slots: {dark: '', light: ''}, showAll: false, highlighted: '',
  opened: null, queue: [], running: null, settling: null,
  selection: '', selectionName: '', error: '', actionError: '', reloadPending: '',
  list: {running: false},
  set: {running: false, command: []},
  settle,
  guard: {restart() {}, stop() {}},
  JSON, Object,
});
// The store's bindings, evaluated the way Qt would on every read.
Object.defineProperties(state, {
  themes: {get() { return state.catalog.themes || [] }},
  total: {get() { return state.themes.length }},
  current: {get() { return state.selection !== '' ? state.selection : state.catalog.current || '' }},
  busy: {get() { return state.running !== null || state.list.running }},
  switching: {get() { return state.running !== null || state.queue.length > 0 || state.settling !== null }},
  slot: {get() { return state.slots[state.mode] || '' }},
  carousel: {get() { return state.Bridge.call('themes.carousel', [state.themes, state.slot, state.query, state.showAll ? 'all' : state.mode]) }},
  focusedId: {get() { return state.Bridge.call('themes.focus', [state.carousel, state.highlighted, state.slot]) }},
  scheduleText: {get() { return state.Bridge.call('themes.schedule', [state.appearance]) }},
  autoSource: {get() { return (state.appearance.auto || {}).source || 'off' }},
  store: {get() { return state }},
});
vm.runInContext(methods(source), state);
// The Process: every start records its argument vector.
Object.defineProperty(state.set, 'running', {
  get() { return this._running || false },
  set(value) { if (value && !this._running) commands.push([...this.command].slice(1).join(' ')); this._running = value },
});

const palette = (seed) => {
  const theme = {};
  for (const [index, role] of ['base', 'mantle', 'crust', 'surface', 'overlay', 'text',
    'subtext', 'accent', 'red', 'green', 'yellow'].entries())
    theme[role] = `#${String(seed + index).padStart(2, '0').repeat(3)}`;
  return theme;
};
const preset = (id, name, mode, seed) => ({id, name, mode, ...palette(seed)});
const appearance = (over = {}) => ({mode: 'dark', dark: 'catppuccin-mocha', light: 'catppuccin-latte',
  auto: {source: 'off', lightAt: '07:00', darkAt: '19:00'}, place: 'Berlin', next: null, ...over});
const reply = {
  current: 'catppuccin-mocha',
  themes: [
    preset('catppuccin-mocha', 'Catppuccin Mocha', 'dark', 10),
    preset('catppuccin-latte', 'Catppuccin Latte', 'light', 20),
    preset('rose-pine', 'Rosé Pine', 'dark', 30),
    preset('rose-pine-dawn', 'Rosé Pine Dawn', 'light', 40),
    preset('nord', 'Nord', 'dark', 50),
  ],
  appearance: appearance(),
};
const names = () => state.carousel.items.map(item => item.name);
// The helper answers the request in flight.
function helperAnswers(over = {}, code = 0) {
  if (code === 0) state.answered({pending: [], appearance: appearance(over)});
  state.set._running = false;
  state.finished(code);
}

// Opening reads the catalog and the helper's slots and mode; what it first
// reads is what a cancel puts back.
state.panelOpen = true;
state.openChanged();
assert.equal(state.list.running, true, 'opening reads the catalog');
state.list.running = false;
state.accept(reply);
assert.equal(state.mode, 'dark');
assert.deepEqual({...state.slots}, {dark: 'catppuccin-mocha', light: 'catppuccin-latte'});
assert.deepEqual({...state.opened}, {mode: 'dark', dark: 'catppuccin-mocha', light: 'catppuccin-latte'});

// The carousel offers the edited mode's presets, centred on the slot's own.
assert.deepEqual(names(), ['Catppuccin Mocha', 'Rosé Pine', 'Nord']);
assert.equal(state.focusedId, 'catppuccin-mocha');
state.toggleAll();
assert.equal(names().length, 5, 'any preset may fill the slot');
state.toggleAll();

// Arrows move the slot at once and tell the helper once the burst settles.
state.step('right');
state.step('right');
assert.equal(state.slots.dark, 'nord', 'the picker is ahead of the helper');
assert.equal(state.focusedId, 'nord');
assert.deepEqual(commands, [], 'nothing is sent mid-burst');
assert.equal(state.switching, true);
state.flushSettling();
assert.deepEqual(commands, ['slot dark nord'], 'the burst sends its last move only, for its own slot');

// While that runs, the reader switches mode and picks a light theme: each
// waits its turn, and a newer choice for a slot replaces the waiting one.
state.setMode('light');
assert.equal(state.mode, 'light');
assert.deepEqual(names(), ['Catppuccin Latte', 'Rosé Pine Dawn'], 'the carousel now edits the light slot');
state.choose('rose-pine-dawn', true);
state.choose('catppuccin-latte', true);
state.choose('rose-pine-dawn', true);
assert.deepEqual([...state.queue].map(item => item.op + ' ' + (item.id || item.mode)), ['mode light', 'slot rose-pine-dawn']);
// A reply while the reader is ahead is kept, not adopted.
state.answered({pending: [], appearance: appearance({dark: 'nord'})});
assert.equal(state.mode, 'light', 'an older reply does not pull the picker back');
helperAnswers({dark: 'nord'});
assert.deepEqual(commands, ['slot dark nord', 'mode light']);
helperAnswers({dark: 'nord', mode: 'light'});
assert.deepEqual(commands, ['slot dark nord', 'mode light', 'slot light rose-pine-dawn']);
helperAnswers({dark: 'nord', light: 'rose-pine-dawn', mode: 'light'});
assert.equal(state.switching, false);
assert.deepEqual([state.mode, state.slots.light, state.slots.dark], ['light', 'rose-pine-dawn', 'nord'],
  'once everything is answered, the helper\'s word stands');

// A mode switch mid-burst sends the burst first, for the mode it belonged to.
state.step('left');
assert.equal(state.slots.light, 'catppuccin-latte');
state.setMode('dark');
assert.deepEqual(commands.slice(-1), ['slot light catppuccin-latte'], 'the light slot is told before the mode moves');
helperAnswers({dark: 'nord', light: 'catppuccin-latte', mode: 'light'});
assert.deepEqual(commands.slice(-1), ['mode dark']);
helperAnswers({dark: 'nord', light: 'catppuccin-latte', mode: 'dark'});

// The schedule: sun, fixed times keeping the saved ones, and off.
state.setAuto('sun');
assert.deepEqual(commands.slice(-1), ['auto sun']);
helperAnswers({dark: 'nord', light: 'catppuccin-latte', auto: {source: 'sun', lightAt: '07:00', darkAt: '19:00'}, next: {mode: 'light', clock: '07:12'}});
assert.equal(state.autoSource, 'sun');
assert.equal(state.scheduleText, 'Light at 07:12 · sunrise in Berlin');
state.setAuto('schedule');
assert.deepEqual(commands.slice(-1), ['auto schedule 07:00 19:00'], 'the saved times are kept');
helperAnswers({dark: 'nord', light: 'catppuccin-latte', auto: {source: 'schedule', lightAt: '07:00', darkAt: '19:00'}});
state.setAuto('schedule', '06:30', '21:15');
assert.deepEqual(commands.slice(-1), ['auto schedule 06:30 21:15']);
helperAnswers({dark: 'nord', light: 'catppuccin-latte', auto: {source: 'schedule', lightAt: '06:30', darkAt: '21:15'}});
state.setAuto('whenever');
assert.deepEqual(commands.slice(-1), ['auto schedule 06:30 21:15'], 'an unknown source sends nothing');

// Cancel puts back mode and both slots as the picker opened, superseding
// whatever still waited.
state.choose('rose-pine', true);
state.setMode('light');
state.cancel();
assert.deepEqual([state.mode, state.slots.dark, state.slots.light], ['dark', 'catppuccin-mocha', 'catppuccin-latte']);
assert.deepEqual([...state.queue].map(item => item.op), ['restore'], 'nothing waiting survives a restore');
helperAnswers({dark: 'nord'});
assert.deepEqual(commands.slice(-1), ['restore dark catppuccin-mocha catppuccin-latte']);
helperAnswers();

// A refusal is read back from the helper rather than assumed.
state.choose('nord', true);
helperAnswers({}, 1);
assert.match(state.actionError, /could not be applied/);
assert.equal(state.list.running, true, 'after a refusal the slots are read again');
state.list.running = false;

// Something else switching — the schedule at a boundary, the launcher — makes
// an idle open picker read the slots and mode again.
state.selectionMoved();
assert.equal(state.list.running, true);
state.list.running = false;
state.choose('rose-pine', false);
state.selectionMoved();
assert.equal(state.list.running, false, 'a switch of its own does not');
state.flushSettling();
helperAnswers({dark: 'rose-pine'});

// Filtering narrows the carousel and switches nothing.
const sent = commands.length;
state.query = 'nord';
assert.deepEqual(names(), ['Nord']);
state.query = 'zzz';
assert.equal(state.carousel.count, 0);
assert.equal(state.focusedId, '');
state.step('right');
state.resetFilters();
assert.equal(commands.length, sent, 'filtering never switched the desktop');

// Closing mid-burst keeps the last move, and the next opening starts afresh.
state.step('right');
state.panelOpen = false;
state.openChanged();
assert.match(commands.at(-1), /^slot dark /, 'closing sends the settled-on choice');
assert.equal(state.opened, null);
assert.equal(state.highlighted, '');

// The reply names what did not reload.
state.answered({pending: ['Ghostty', 'tmux'], appearance: appearance()});
assert.equal(state.reloadPending, 'Ghostty and tmux still show the previous palette.');

// Failure wording is the shared native map's, and says what happened to the
// selection rather than reporting a success.
assert.equal(state.failure(''), '');
assert.match(state.failure('unavailable'), /Nothing was changed/);
assert.match(state.failure('timeout'), /Refresh to see what is selected/);
assert.match(state.failure('whatever'), /Try again/);

// This surface reads the catalog and asks the helper for five things: a
// slot, the mode, a restore, the schedule, and the catalog itself. It writes
// no file, keeps no palette of its own and starts nothing else.
const verbs = [...source.matchAll(/\["seele-theme", "([a-z]+)"/g)].map(match => match[1]);
assert.deepEqual([...new Set(verbs)].sort(), ['auto', 'list', 'mode', 'restore', 'slot']);
assert.doesNotMatch(source, /"seele-theme", "set"/, 'the picker names the slot it edits rather than the mode on screen');
assert.doesNotMatch(source, /atomicWrite|writeFile|\.write\(/, 'the store publishes nothing itself');
assert.match(source, /Bridge\.call\("themes\.selected"/, 'the published selection is parsed natively');
for (const policy of ['carousel', 'focus', 'step', 'schedule', 'name'])
  assert.match(source, new RegExp(`Bridge\\.call\\("themes\\.${policy}"`), `${policy} is decided natively`);
assert.match(source, /Models\.reconcile\(rows, carousel\.items/, 'cards are kept by ID so they slide rather than rebuild');
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
assert.doesNotMatch(panel, /store\.apply\(|text: "Apply"/, 'moving switches; there is no apply step');

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

console.log('Theme slots, mode, schedule, coalesced switching, cancel, refusals, filtering and floating production wiring passed');
