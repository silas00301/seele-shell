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

const guards = [];
const state = vm.createContext({
  Models: require('./list-models.cjs')(),
  Bridge: nativeBridge(),
  rows,
  catalog: {ok: false, current: '', themes: []},
  panelOpen: false, query: '', selection: '', selectionName: '',
  error: '', actionError: '', reloadPending: '', applying: '',
  list: {running: false},
  set: {running: false, command: []},
  guard: {restart() { guards.push('restart') }, stop() { guards.push('stop') }},
  JSON,
});
Object.defineProperties(state, {
  themes: {get() { return state.catalog.themes || [] }},
  total: {get() { return state.themes.length }},
  current: {get() { return state.selection !== '' ? state.selection : state.catalog.current || '' }},
  busy: {get() { return state.applying !== '' || state.list.running }},
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
  current: 'nord',
  themes: [
    preset('catppuccin-mocha', 'Catppuccin Mocha', 'dark', 10),
    preset('nord', 'Nord', 'dark', 20),
    preset('rose-pine-dawn', 'Rosé Pine Dawn', 'light', 30),
  ],
};
const listed = () => values.map(row => row.entry.id);

// A catalog is read, grouped and marked; nothing is written and nothing runs.
state.accept(reply);
assert.equal(state.error, '');
assert.deepEqual(listed(), ['nord', 'catppuccin-mocha', 'rose-pine-dawn'],
  'the applied theme leads, then the catalog order inside each mode');
assert.deepEqual(values.map(row => row.entry.section), ['CURRENT', 'DARK', 'LIGHT']);
assert.equal(values[0].entry.current, true);
// The header line is the native one the store binds, not a second sentence.
const detail = () => state.Bridge.call('themes.detail',
  [{themes: state.themes, current: state.current}, rows.count]);
assert.equal(detail(), 'Nord · 3 presets');
assert.match(source, /Bridge\.call\("themes\.detail"/);

// The published selection wins over the reply, because it is what the desktop
// is actually wearing: a theme applied from the launcher marks its row here.
state.selection = 'rose-pine-dawn';
state.publish();
assert.deepEqual(listed(), ['rose-pine-dawn', 'catppuccin-mocha', 'nord']);
state.selection = '';
state.publish();

// A search filters the same rows without asking the helper anything.
state.query = 'mocha';
state.publish();
assert.deepEqual(listed(), ['catppuccin-mocha']);
assert.equal(detail(), 'Nord · 1 of 3', 'the header says how much of the catalog is left');
assert.equal(values[0].entry.first, true, 'the first row left in a group keeps its heading');
state.query = '';
state.publish();

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
assert.equal(state.apply('catppuccin-mocha'), true);
assert.deepEqual([...state.set.command], ['seele-theme', 'set', 'catppuccin-mocha']);
assert.equal(state.set.running, true);
assert.equal(state.actionError, '');
assert.deepEqual(guards, ['restart']);
assert.equal(state.apply('nord'), false, 'a second choice cannot overtake the one in flight');
assert.deepEqual([...state.set.command], ['seele-theme', 'set', 'catppuccin-mocha']);

// The reply says what could not be reloaded, never what is selected: that is
// read back from the helper's own published selection.
state.applied({id: 'catppuccin-mocha', pending: ['Ghostty', 'tmux']});
assert.equal(state.reloadPending, 'Ghostty and tmux still show the previous palette.');
assert.equal(state.current, 'nord', 'the request does not mark the row; the selection file does');
state.applied({id: 'catppuccin-mocha', pending: []});
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
assert.match(shell, /height: devicesY \+ smallTileHeight \* 5 \+ gap \* 4/,
  'the grid is as tall as the modules it now holds');

console.log('Theme catalog grouping, selection marking, single-flight applies, reload reporting and production wiring passed');
