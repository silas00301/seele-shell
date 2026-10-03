// Presentation mode (SIL-55): the production setPresenting() from shell.qml,
// run against the real native policy, must borrow Caffeinate's one session
// without ever taking over or ending one the user started.
const {nativeBridge} = require('./native-functions.cjs');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

const shell = fs.readFileSync(process.argv[2], 'utf8');
const start = shell.indexOf('  function setPresenting(on) {');
assert.ok(start > 0, 'setPresenting must exist');
const body = shell.slice(start, shell.indexOf('\n  }\n', start) + 4);

function harness(session, now) {
  const sent = [];
  const context = vm.createContext({
    Bridge: nativeBridge(),
    presentingRetained: {manual: false, caffeinateStarted: 0},
    caffeinateStore: {session, send(request) { sent.push(request); }, stop() { sent.push({op: 'stop'}); }},
    Date: {now: () => context.clock * 1000},
    clock: now,
  });
  vm.runInContext(body.replace('function setPresenting', 'function set'), context);
  return {context, get sent() { return JSON.parse(JSON.stringify(sent)); }, set: (on) => context.set(on)};
}

{
  // No session running: the mode starts one and ends that one again.
  const h = harness({active: false}, 1000);
  h.set(true);
  assert.deepEqual(h.sent, [{op: 'start', mode: 'manual'}]);
  assert.equal(h.context.presentingRetained.caffeinateStarted, 1000);
  h.set(true);
  assert.equal(h.sent.length, 1, 'turning it on twice starts one session');
  h.context.caffeinateStore.session = {active: true, mode: 'manual', elapsed: 1790};
  h.context.clock = 2800;
  h.set(false);
  assert.deepEqual(h.sent.at(-1), {op: 'stop'}, 'a heartbeat-stale snapshot of its own session is still its own');
  assert.equal(h.context.presentingRetained.caffeinateStarted, 0);
}
{
  // The user already keeps the session awake: the mode leaves it alone both ways.
  const h = harness({active: true, mode: 'duration', elapsed: 60, remaining: 3000}, 1000);
  h.set(true);
  h.context.clock = 2000;
  h.set(false);
  assert.deepEqual(h.sent, [], 'a session the user started is neither replaced nor ended');
}
{
  // The user replaced the mode's session while presenting: that one stays.
  const h = harness({active: false}, 1000);
  h.set(true);
  h.context.caffeinateStore.session = {active: true, mode: 'manual', elapsed: 120};
  h.context.clock = 4000;
  h.set(false);
  assert.deepEqual(h.sent, [{op: 'start', mode: 'manual'}], 'a replaced session is not stopped');
}

// Toasts and the bar's personal text follow the mode wherever it is drawn.
for (const [pattern, message] of [
  [/!root\.systemData\.dnd\s*\n\s*&& !root\.presenting/, 'toasts are held back while presenting'],
  [/root\.presenting \? root\.windowAppName\(/, 'the window title leaves the bar'],
  [/summaryText !== "" && !root\.presenting/, 'Home Assistant readings leave the bar'],
  [/root\.presenting \? "Event" : root\.calendarIndicator\.title/, 'the event title leaves the bar'],
  [/hasArt: barMediaArtImage\.status === Image\.Ready && !root\.presenting/, 'artwork leaves the bar'],
  [/"presenting\.state",\s*\n\s*\[presentingRetained\.manual, !!root\.systemData\.screenRecording\]/, 'a screen share implies the mode'],
]) assert.match(shell, pattern, message);
assert.equal((shell.match(/visible: !root\.presenting\n\s*text: root\.mediaLabel/g) || []).length, 2, 'both media entries drop their text');
console.log('presentation mode keep-awake ownership and bar/toast concealment passed');
