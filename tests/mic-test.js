// The Audio panel's microphone test: device resolution, the microphone-use
// gate, meter rest, device loss and panel-close discard, executed against the
// same Rust policy the QML plugin calls.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {nativeBridge, source} = require('./native-functions.cjs');

const adapterSource = fs.readFileSync(process.argv[2], 'utf8');
const storeSource = fs.readFileSync(process.argv[3], 'utf8');
const card = fs.readFileSync(process.argv[4], 'utf8');
const shell = fs.readFileSync(process.argv[5], 'utf8');

// The production adapter, running against the production bridge.
const adapter = vm.createContext({Bridge: nativeBridge()});
vm.runInContext(source(adapterSource), adapter);

const sent = [];
const worker = {running: true, write(text) { sent.push(JSON.parse(text)) }};
const idleStatus = () => ({mode: 'idle', sample: false, clipped: false, input: '', output: '', muted: false, error: ''});
const restMeter = () => ({level: 0, peak: 0, clipped: false, remaining: null});
// The store's own literals are created inside its context, so they are
// compared by value rather than by realm.
const plain = value => JSON.parse(JSON.stringify(value));

const devices = [
  {id: 1, kind: 'output', name: 'Speakers', node: 'sink.speakers', default: true, selected: true},
  {id: 2, kind: 'output', name: 'Headphones', node: 'sink.headphones'},
  {id: 3, kind: 'output', name: 'HDMI card · Stereo', node: '', profile: 2},
  {id: 4, kind: 'input', name: 'Webcam', node: 'source.webcam', default: true},
  {id: 5, kind: 'input', name: 'Headset', node: 'source.headset'},
];

const context = vm.createContext({MicTest: adapter, worker, JSON, String, Qt: {callLater(fn) { fn() }}});
const store = {
  panelOpen: true, devices, chosenOutput: '', status: idleStatus(), meter: restMeter(),
  users: [], detection: true, confirming: '', notice: '',
};
// The store's own declared bindings, read out of the production QML so the
// fixture cannot drift from what Qt evaluates.
for (const name of ['resolved', 'presentation', 'usage', 'outputs', 'output', 'outputName', 'input', 'inputName', 'active']) {
  const binding = storeSource.match(new RegExp(`readonly property \\w+ ${name}: ([^\\n]+)`));
  assert.ok(binding, `${name} is not a declared binding of MicTestStore`);
  const expression = vm.compileFunction(`return (${binding[1]})`, [], {parsingContext: context});
  Object.defineProperty(store, name, {get: expression, enumerable: true});
}
context.store = store;
const methods = [...storeSource.matchAll(/^  function \w+\([^\n]*\) \{(?:[^\n]*\}|\n[\s\S]*?^  \})/gm)].map(match => match[0]);
assert.ok(methods.length >= 8, 'MicTestStore lost its methods');
vm.runInContext(methods.join('\n'), context);
for (const name of ['accept', 'send', 'begin', 'confirm', 'cancel', 'replay', 'stop', 'chooseOutput', 'idle', 'reconcile']) {
  assert.equal(typeof context[name], 'function', `${name} is missing from MicTestStore`);
  store[name] = context[name];
}

// Playback defaults to the system output and follows an explicit choice only.
assert.equal(store.output, 'sink.speakers');
assert.equal(store.outputName, 'Speakers');
assert.equal(store.input, 'source.webcam');
assert.equal(store.inputName, 'Webcam');
assert.deepEqual(store.outputs.map(option => option.node), ['sink.speakers', 'sink.headphones']);
store.chooseOutput('sink.headphones');
assert.equal(store.output, 'sink.headphones');
assert.deepEqual(sent, [], 'choosing a test output while idle starts and stops nothing');

// A sample test starts with both resolved devices named explicitly.
store.begin('sample');
assert.deepEqual(sent.pop(), {command: 'sample', input: 'source.webcam', output: 'sink.headphones'});
assert.equal(store.confirming, '');

// Another application's microphone use gates both modes.
store.users = ['Zoom'];
for (const mode of ['sample', 'live']) {
  store.begin(mode);
  assert.deepEqual(sent, [], `${mode} started before the warning was answered`);
  assert.equal(store.confirming, mode);
  assert.equal(store.usage.busy, true);
  assert.match(store.usage.title, /Zoom is using the microphone/);
  store.cancel();
  assert.equal(store.confirming, '');
  assert.deepEqual(sent, [], 'cancelling the warning starts no capture or playback');
  store.begin(mode);
  store.confirm();
  assert.equal(sent.pop().command, mode);
  assert.equal(store.confirming, '');
}
// Confirming asks the worker for nothing but the test: no mute, stop or move.
store.begin('live');
store.confirm();
assert.deepEqual(Object.keys(sent.pop()).sort(), ['command', 'input', 'output']);

// Unavailable detection reports its limitation instead of claiming quiet, and
// is not turned into a prompt no evidence could answer.
store.users = [];
store.detection = false;
assert.equal(store.usage.known, false);
assert.equal(store.usage.busy, false);
assert.match(store.usage.title, /cannot be checked/);
store.begin('live');
assert.equal(sent.pop().command, 'live');
store.detection = true;
assert.equal(store.usage.title, '');

// Worker events reach the three properties they belong to.
store.accept(JSON.stringify({mode: 'recording', sample: false, clipped: false, input: 'source.webcam', output: 'sink.headphones', muted: false, error: ''}));
assert.equal(store.presentation.recording, true);
store.accept(JSON.stringify({level: 0.8, peak: 0.95, clipped: true, remaining: 2500}));
assert.equal(store.presentation.level, 0.8);
assert.equal(store.presentation.clipped, true);
assert.equal(store.presentation.remaining, '2.5 s left');
store.accept(JSON.stringify({users: ['Firefox'], detection: true}));
assert.deepEqual(store.users, ['Firefox']);
store.users = [];

// Sample playback is not capture: the meter rests rather than replaying the
// recording's last frame as if the microphone were still delivering it.
store.accept(JSON.stringify({mode: 'playing', sample: true, clipped: true, input: 'source.webcam', output: 'sink.headphones', muted: false, error: ''}));
assert.equal(store.presentation.level, 0);
assert.equal(store.presentation.clipped, false);
assert.equal(store.presentation.canStop, true);
assert.equal(store.presentation.canReplay, false);
store.accept(JSON.stringify({mode: 'idle', sample: true, clipped: true, input: 'source.webcam', output: 'sink.headphones', muted: false, error: ''}));
assert.equal(store.presentation.canReplay, true);
assert.equal(store.presentation.sampleClipped, true);
store.replay();
assert.deepEqual(sent.pop(), {command: 'replay', input: 'source.webcam', output: 'sink.headphones'});

// A muted microphone is reported, never unmuted or relevelled.
store.accept(JSON.stringify({mode: 'live', sample: false, clipped: false, input: 'source.webcam', output: 'sink.headphones', muted: true, error: ''}));
assert.equal(store.presentation.muted, true);
assert.deepEqual(sent, [], 'a muted microphone is described, not changed');

// A test output that disappears ends the test instead of falling back to
// whatever else the machine happens to have.
store.devices = devices.filter(device => device.node !== 'sink.headphones');
store.reconcile();
assert.deepEqual(sent.pop(), {command: 'stop'});
assert.match(store.notice, /test output is no longer available/);
store.devices = devices;
store.chosenOutput = 'sink.headphones';

// A microphone that disappears, or one the panel no longer has selected, ends
// the running test before anything starts on the new selection.
store.accept(JSON.stringify({mode: 'live', sample: false, clipped: false, input: 'source.webcam', output: 'sink.headphones', muted: false, error: ''}));
store.devices = devices.filter(device => device.node !== 'source.webcam');
store.reconcile();
assert.deepEqual(sent.pop(), {command: 'stop'});
assert.match(store.notice, /microphone being tested is no longer available/);
store.devices = devices.map(device => device.id === 4 ? {...device, default: false} : device.id === 5 ? {...device, default: true} : device);
store.accept(JSON.stringify({mode: 'live', sample: false, clipped: false, input: 'source.webcam', output: 'sink.headphones', muted: false, error: ''}));
store.reconcile();
assert.deepEqual(sent.pop(), {command: 'stop'});
assert.match(store.notice, /microphone changed/);
store.devices = devices;

// Closing the panel discards the sample and every visible remnant of the test.
store.accept(JSON.stringify({mode: 'idle', sample: true, clipped: true, input: 'source.webcam', output: 'sink.headphones', muted: true, error: 'something failed'}));
store.users = ['Zoom'];
store.confirming = 'live';
store.panelOpen = false;
store.idle();
assert.deepEqual(plain(store.status), idleStatus());
assert.deepEqual(plain(store.meter), restMeter());
assert.equal(store.presentation.sample, false);
assert.equal(store.presentation.canReplay, false);
assert.equal(store.confirming, '');
assert.equal(store.notice, '');
assert.deepEqual(plain(store.users), []);

// A stopped worker cannot be written to, so no request outlives its process.
worker.running = false;
store.send({command: 'live'});
assert.deepEqual(sent, [], 'requests are not queued past the worker that would serve them');
worker.running = true;

// The worker's lifetime is the panel's, which is what discards the sample and
// releases the capture and playback streams.
assert.match(storeSource, /running: store\.panelOpen/, 'the worker must live exactly as long as the panel');
assert.match(storeSource, /command: \["seele-mic-test"\]/);
assert.doesNotMatch(storeSource, /FileView|writeFile|StandardPaths|\.wav|\.raw/, 'the test keeps no audio on disk');
assert.doesNotMatch(card, /FileDialog|Export|Upload|Transcri/i, 'the card offers no export, upload or transcription');

// Every primary control is a focusable button rather than a pointer-only area,
// and Stop is one of them in every active state.
for (const label of ['Record 5 s', 'Listen live', 'Replay', 'Stop']) {
  assert.ok(card.includes(`text: "${label}"`), `${label} is not offered by the card`);
}
assert.match(card, /Shared\.ActionButton\s*\{[^}]*text: "Stop"[^}]*enabled: card\.view\.canStop/s);
assert.doesNotMatch(card, /MouseArea/, 'the test controls stay keyboard-reachable buttons');
assert.match(card, /Flow \{/, 'the controls wrap rather than leaving a narrow panel');
assert.match(card, /headphones/i, 'the feedback hint must precede live mode');
assert.match(card, /CLIPPING/);

// Production wiring.
assert.match(shell, /MicTestStore \{\s*\n\s*id: micTest/);
assert.match(shell, /panelOpen: root\.controlPanel === "audio"/);
assert.match(shell, /MicTestCard \{ theme: root; store: micTest/);
assert.match(shell, /namespace: "seele-shell-audio"\s+WlrLayershell\.keyboardFocus: visible \? WlrKeyboardFocus\.OnDemand/);

console.log('microphone test device resolution, the microphone-use gate, meter rest, device loss and panel-close discard passed');
