const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const input = process.argv[2];
const sourcePath = input.endsWith('.qml') ? input : path.join(input, 'MaintenanceStore.qml');
const source = fs.readFileSync(sourcePath, 'utf8');
const panel = fs.readFileSync(path.join(path.dirname(sourcePath), 'MaintenancePanel.qml'), 'utf8');
function method(name) {
  const start = source.indexOf('  function ' + name + '(');
  assert.ok(start >= 0, name);
  let end = source.indexOf('{', start) + 1, depth = 1;
  while (depth && end < source.length) {
    if (source[end] === '{') depth++;
    if (source[end] === '}') depth--;
    end++;
  }
  return source.slice(start, end);
}
function model() {
  return {
    rows: [], replacements: 0,
    get count() { return this.rows.length; },
    get(i) { return this.rows[i]; },
    insert(i, value) { this.rows.splice(i, 0, value); },
    move(from, to) { this.rows.splice(to, 0, this.rows.splice(from, 1)[0]); },
    remove(i, count = 1) { this.rows.splice(i, count); },
    setProperty(i, key, value) { this.rows[i][key] = value; this.replacements++; },
  };
}
const ctx = vm.createContext({
  snapshot: {active: [], snoozed: [], history: [], count: 0},
  activeRows: model(), snoozedRows: model(), historyRows: model(),
  expandedIds: {}, error: '', pendingId: '', actionErrorId: '', actionError: '', confirmation: null,
  command: {running: false, payload: '', targetId: '', targetRevision: 0}, poll: {running: false},
});
ctx.store = ctx;
for (const name of ['reconcile', 'row', 'unavailable', 'accept', 'refresh', 'request', 'repair', 'confirm', 'acceptAction'])
  vm.runInContext(method(name), ctx);
function finding(id = 'fixture', extra = {}) {
  return {id, revision: 1, title: 'Fixture', source: 'fixture', urgency: 'soon', busy: '', resolved: 0,
    lifecycle: 'ongoing', actions: [{id: 'retry', label: 'Retry', disruptive: false}, {id: 'restart', label: 'Restart', disruptive: true}],
    canAnalyze: true, analysis: {revision: 1, actions: ['retry']}, analysisStale: false, ...extra};
}
function snapshot(active, extra = {}) { return {ok: true, active, snoozed: [], history: [], count: active.length, checkErrors: {}, ...extra}; }
function finish() { ctx.command.running = false; ctx.pendingId = ''; }
const a = finding(), b = finding('second');
assert.equal(ctx.accept(snapshot([a, b])), true);
const original = ctx.activeRows.get(0), second = ctx.activeRows.get(1);
ctx.expandedIds.fixture = true;
ctx.accept(JSON.parse(JSON.stringify(snapshot([a, b]))));
assert.equal(ctx.activeRows.get(0), original, 'polling retains delegate identity');
assert.equal(ctx.activeRows.replacements, 0, 'unchanged rows do not reassign their role');
ctx.accept(snapshot([b, a]));
assert.equal(ctx.activeRows.get(0), second, 'priority changes move the existing row');
assert.equal(ctx.activeRows.get(1), original);
assert.equal(ctx.expandedIds.fixture, true);
assert.equal(ctx.repair(a, a.actions[1], false), true);
assert.equal(ctx.command.running, false, 'disruptive action only stages confirmation');
assert.equal(ctx.confirmation.action, 'restart');
ctx.accept(snapshot([finding('fixture', {revision: 2})]));
assert.equal(ctx.confirmation, null, 'new finding revision invalidates confirmation');
assert.equal(ctx.confirm(), false);
assert.equal(ctx.request(a, 'done'), false, 'old revision cannot mark a finding done');
ctx.accept(snapshot([a]));
assert.equal(ctx.request(a, 'done'), false, 'ongoing findings cannot be user-resolved');
assert.equal(ctx.request(a, 'shell'), false, 'unregistered operation rejected');
assert.equal(ctx.request(a, 'action', {action: 'restart'}), false, 'disruptive actions cannot bypass confirmation');
assert.equal(ctx.repair(a, {id: 'invented', label: 'Run command'}, false), false);
assert.equal(ctx.repair(a, a.actions[0], true), true);
assert.equal(ctx.command.running, false, 'AI proposals need explicit confirmation even for safe action');
assert.equal(ctx.confirm(), true);
assert.deepEqual(JSON.parse(ctx.command.payload), {op: 'action', id: 'fixture', revision: 1, action: 'retry', confirmed: true});
assert.equal(ctx.request(a, 'analyze'), false, 'overlapping click rejected');
finish();
ctx.accept(snapshot([finding('fixture', {analysisStale: true})]));
assert.equal(ctx.repair(ctx.row('fixture'), a.actions[0], true), false, 'stale AI analysis cannot propose a repair');
ctx.accept(snapshot([a]));
assert.equal(ctx.request(a, 'snooze', {seconds: 0}), false);
assert.equal(ctx.request(a, 'snooze', {seconds: 3.5}), false);
assert.equal(ctx.request(a, 'snooze', {seconds: 2592001}), false);
for (const seconds of [60, 3600, 86400, 604800, 2592000]) {
  assert.equal(ctx.request(a, 'snooze', {seconds, op: 'shell', id: 'other', revision: 99}), true);
  assert.deepEqual(JSON.parse(ctx.command.payload), {op: 'snooze', id: 'fixture', revision: 1, seconds});
  finish();
}
assert.equal(ctx.request(a, 'analyze'), true);
ctx.acceptAction({ok: false});
assert.equal(ctx.actionErrorId, 'fixture', 'failure stays on affected row');
finish();
ctx.accept(snapshot([finding('fixture', {revision: 2})]));
assert.equal(ctx.actionErrorId, '', 'old error does not attach to changed finding');
ctx.acceptAction({ok: false});
assert.equal(ctx.actionErrorId, '', 'late failure ignored after revision change');
ctx.accept(snapshot([], {snoozed: [a], history: [finding('old', {resolved: 10})]}));
assert.equal(ctx.snoozedRows.count, 1);
assert.equal(ctx.historyRows.count, 1);
assert.equal(ctx.request(ctx.row('old'), 'unsnooze'), false, 'history is read-only');
ctx.repair(a, a.actions[1], false);
ctx.accept({ok: false});
assert.notEqual(ctx.error, '');
assert.equal(ctx.confirmation, null, 'source failure invalidates pending consent');
assert.equal(ctx.snoozedRows.count + ctx.activeRows.count + ctx.historyRows.count, 0, 'unavailable source never presents old rows as current');
assert.equal(ctx.request(a, 'analyze'), false);
assert.equal(ctx.accept(null), false);
assert.match(panel, /component FindingCard: Rectangle/);
assert.equal((panel.match(/FindingCard \{\s*required property var modelData;\s*finding:\s*modelData\s*\}/g) || []).length, 3);
assert.doesNotMatch(panel, /onLoaded|sourceComponent:\s*cardComponent/);
assert.match(panel, /validator:\s*IntValidator \{\s*bottom:\s*1;\s*top:\s*43200\s*\}/);
assert.match(source, /code!==0 \|\| !received/, 'empty replies fail visibly');
console.log('Maintenance production QML methods: stable rows, revision-bound confirmation, typed actions, snooze, stale replies and unavailable source passed');
