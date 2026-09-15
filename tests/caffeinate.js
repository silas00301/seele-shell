// Run the production Caffeinate store methods and the shared native projection,
// then prove the shell wires them into a conditional bar item and one panel.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const {nativeBridge} = require('./native-functions.cjs');

const source = fs.readFileSync(process.argv[2], 'utf8');
const panel = fs.readFileSync(process.argv[3], 'utf8');
const shell = fs.readFileSync(process.argv[4], 'utf8');
const methods = [...source.matchAll(/^  function \w+\([^\n]*\) \{\n[\s\S]*?^  \}/gm)].map(m => m[0]).join('\n');
const projection = source.match(/readonly property var projection: ([^\n]+)/)[1];
const idle = {version: 1, active: false, mode: '', elapsed: 0, remaining: 0, task: null};
const state = vm.createContext({
  Bridge: nativeBridge(), idle, session: idle, error: '', actionError: '', payload: '',
  action: {running: false}, JSON,
});
vm.runInContext(methods, state);
const project = () => vm.runInContext(projection, state);

// A snapshot from the service is the only thing that makes a session real.
assert.equal(project().active, false);
assert.equal(project().barText, '');
state.accept({version: 1, active: true, mode: 'duration', remaining: 4320, elapsed: 60, task: null});
assert.equal(project().active, true);
assert.equal(project().barText, '\u{f0176} 1h12', 'the bar states the remaining time it was given');
assert.equal(project().detail, '1 h 12 min remaining');
assert.equal(project().headline, 'For a set time');

// The countdown is the service's, never the panel's: nothing here recomputes it.
assert.doesNotMatch(source, /Date\.now|new Date|setInterval/, 'remaining time is computed natively');
assert.doesNotMatch(panel, /Date\.now|new Date|Timer\s*\{/, 'the panel draws time, it does not keep it');

// A malformed or unversioned line cannot replace a live session.
const live = state.session;
state.accept(null);
state.accept({active: false});
state.accept({version: 2, active: false});
assert.equal(state.session, live, 'only version 1 snapshots are accepted');

state.accept({version: 1, active: true, mode: 'task', elapsed: 30, remaining: 0,
  task: {kind: 'build', label: 'nix build', project: 'seele', pid: 4242}});
assert.equal(project().barText, '\u{f0176}', 'an untimed session shows the cup alone');
assert.equal(project().detail, 'nix build');
assert.equal(project().task.pid, 4242);
assert.match(project().hoverText, /Until a task ends/);

state.accept({version: 1, active: true, mode: 'manual', elapsed: 780, remaining: 0, task: null});
assert.equal(project().detail, 'Active for 13 min');

// Stop is one request at a time, and it is the only thing this surface sends.
state.stop();
assert.deepEqual(JSON.parse(state.payload), {op: 'stop'});
assert.equal(state.action.running, true);
state.payload = '';
state.stop();
assert.equal(state.payload, '', 'a second Stop cannot overtake the one in flight');
const ops = [...source.matchAll(/op: *"([a-z-]+)"/g)].map(m => m[1]);
assert.deepEqual(ops, ['stop'], 'the shell surface sends Stop and nothing else');

// Failure codes are named by the shared native map, not by the QML.
assert.equal(state.failure(''), '');
assert.match(state.failure('service-unavailable'), /Caffeinate is unavailable/);
assert.match(state.failure('task-unavailable'), /already ended/);
assert.match(state.failure('duration-out-of-range'), /1 minute and 24 hours/);
assert.match(state.failure('nonsense-code'), /Try again/);

// Production wiring.
assert.match(shell, /CaffeinateStore \{\s*\n\s*id: caffeinateStore/, 'the store is instantiated');
assert.match(shell, /visible: caffeinateStore\.active/, 'the bar item is conditional on an active session');
assert.match(shell, /CaffeinatePanel \{ theme: root; store: caffeinateStore/, 'bar and panel share one store');
assert.match(shell, /namespace: "seele-shell-caffeinate"/);
assert.match(shell, /root\.toggleControl\("caffeinate"/, 'the bar item opens the panel');
assert.match(panel, /text: "Stop"/, 'the panel offers Stop');
assert.match(panel, /onClicked: panel\.store\.stop\(\)/);
assert.match(panel, /visible: !panel\.store\.active/, 'an ended session leaves an empty panel, not a stale one');

// Closing either surface must not end the session: the store owns no lifecycle
// and the window's visibility drives nothing but its own rendering.
const window = shell.slice(shell.indexOf('id: caffeinateWindow'), shell.indexOf('id: caffeinateWindow') + 1400);
assert.doesNotMatch(window, /caffeinateStore\.(stop|send)\(/, 'hiding the panel cannot release the inhibitor');

console.log('Caffeinate projection, snapshot guards, single-flight Stop and production wiring passed');
