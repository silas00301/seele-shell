// The port inspector's store: what it sends, what it refuses to send, and what
// it says about a result. Discovery and privilege belong to seele-ports; these
// are the guards that keep a confirmation from turning into a request the
// reader never made.
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

const sent = [];
const opened = [];
const state = vm.createContext({
  Models: require('./list-models.cjs')(),
  Bridge: nativeBridge(),
  rows,
  snapshot: {version: 1, rows: [], total: 0, limited: false, query: {}},
  panelOpen: false, query: '', bindingScope: 'all', reviewValid: false, plan: null, outcome: null, expanded: '',
  error: '', actionError: '', schemes: {}, queue: [], pending: '', copied: '',
  worker: {running: true, write(text) { sent.push(JSON.parse(text)) }},
  guard: {restart() {}, stop() {}},
  search: {restart() {}},
  clipboard: {payload: '', stdinEnabled: false, running: false},
  Qt: {callLater(fn) { fn() }, openUrlExternally(url) { opened.push(url) }},
});
Object.defineProperty(state, 'busy', {get() { return state.pending !== '' || state.queue.length > 0 }});
vm.runInContext(methods(source), state);

function listener(over) {
  return Object.assign({
    id: '4:41001', inode: 41001, family: 'ipv4', address: '127.0.0.1',
    binding: '127.0.0.1:3000', port: 3000, scope: 'loopback',
    scopeLabel: 'This machine only', uid: 1000, user: 'silash',
    destination: '127.0.0.1', selected: 101, identified: false, identifiable: false,
    owners: [{pid: 101, start: 1101, name: 'node', uid: 1000, user: 'silash', unit: '', service: '', project: 'seele', projectPath: '/home/silash/seele', proxy: ''}],
    target: {kind: 'process', unit: '', scope: '', pid: 101, start: 1101, uid: 1000, user: 'silash', name: 'node', privileged: false, reason: ''},
    token: '1|41001|127.0.0.1:3000|3000|process|||101|1101|1000',
  }, over || {});
}
function snapshot(rowList, over) {
  return Object.assign({version: 1, type: 'snapshot', rows: rowList, total: rowList.length, limited: false, query: {text: '', scheme: 'http', explicitScheme: false}}, over || {});
}

// --- listing is inert -------------------------------------------------------
const plain = listener();
state.accept(snapshot([plain]));
assert.equal(rows.count, 1);
assert.deepEqual(sent, [], 'a snapshot alone sends nothing back');
assert.deepEqual(opened, [], 'listing ports opens no browser tab');
assert.equal(state.url(plain), 'http://127.0.0.1:3000');

// A row keeps its delegate across a refresh that did not change it.
const identity = rows.get(0).entry;
state.accept(snapshot([listener()]));
assert.equal(rows.get(0).entry, identity, 'an unchanged listener keeps its row');
const six = listener({id: '6:41004', binding: '[::1]:9000', port: 9000, destination: '[::1]', family: 'ipv6'});
state.accept(snapshot([six, listener()]));
assert.equal(rows.get(1).entry, identity, 'reordering keeps existing rows');

// --- addresses --------------------------------------------------------------
assert.equal(state.url(six), 'http://[::1]:9000', 'an IPv6 URL keeps its brackets');
const wildcard = listener({id: '4:41002', binding: '0.0.0.0:8080', port: 8080, destination: '127.0.0.1', scope: 'wildcard'});
assert.equal(state.url(wildcard), 'http://127.0.0.1:8080',
  'a wildcard binding opens a local destination while the row keeps the binding');
assert.equal(wildcard.binding, '0.0.0.0:8080');
state.setScheme(plain.id, 'https');
assert.equal(state.url(plain), 'https://127.0.0.1:3000', 'the scheme is the reader\'s to change');
state.setScheme(plain.id, 'http');
state.query = 'https://localhost:3000';
state.accept(snapshot([plain], {query: {text: 'https://localhost:3000', scheme: 'https', explicitScheme: true}}));
assert.equal(state.scheme('4:99999'), 'https', 'a typed scheme is preserved as the proposal');
state.query = '';
state.accept(snapshot([plain]));
assert.equal(state.scheme('4:99999'), 'http', 'a bare port is never read as proof of a scheme');

state.copy(plain);
assert.equal(state.clipboard.payload, 'http://127.0.0.1:3000');
assert.deepEqual(opened, [], 'copying contacts nothing');
state.openUrl(plain);
assert.deepEqual(opened, ['http://127.0.0.1:3000'], 'opening is explicit');

// --- stopping is confirmed, once, for the row it was raised on --------------
sent.length = 0;
state.expand(plain.id);
assert.equal(state.expanded, plain.id);
assert.equal(state.review(plain.id, 'graceful'), true);
assert.deepEqual(sent, [{op: 'plan', id: plain.id, mode: 'graceful'}], 'Stop asks for a plan first');
assert.equal(state.plan, null, 'nothing is confirmable until the worker describes it');

const reviewValue = {
  type: 'plan', id: plain.id, mode: 'graceful', ok: true,
  token: plain.token, binding: plain.binding, port: 3000,
  target: {kind: 'process', name: 'node', pid: 101, user: 'silash', privileged: false, unit: '', reason: ''},
  disclosure: ['Only process 101 (node) is asked to stop.'], canForce: false,
};
sent.length = 0;
state.reviewValid = true;
state.accept(reviewValue);
assert.ok(state.plan, 'the described target becomes the confirmation');
state.dismiss();
assert.equal(state.plan, null);
assert.deepEqual(sent, [], 'cancelling a confirmation sends no stop request');

// A plan that arrives for a row the reader folded away is not a confirmation.
state.expanded = '';
state.reviewValid = true;
state.accept(reviewValue);
assert.equal(state.plan, null, 'a plan for a folded row is discarded');
state.expand(plain.id);
state.reviewValid = true;
state.accept(reviewValue);
assert.ok(state.plan);

// A listener that changed while the confirmation was open cannot be confirmed.
state.accept(snapshot([listener({token: '1|41001|127.0.0.1:3000|3000|process|||777|9999|1000'})]));
assert.ok(state.plan, 'the confirmation is still on screen');
sent.length = 0;
assert.equal(state.confirm(), false);
assert.deepEqual(sent, [], 'a stale confirmation sends nothing');
assert.match(state.actionError, /changed|no longer/i);

// A vanished listener takes its confirmation with it.
state.reviewValid = true;
state.accept(reviewValue);
state.accept(snapshot([]));
assert.equal(state.plan, null, 'a confirmation for a gone listener is withdrawn');

// The ordinary path: confirm exactly what was described.
state.accept(snapshot([plain]));
state.expand(plain.id);
state.reviewValid = true;
state.accept(reviewValue);
sent.length = 0;
assert.equal(state.confirm(), true);
assert.deepEqual(sent, [{op: 'stop', token: plain.token, mode: 'graceful'}]);
assert.equal(state.plan, null, 'a confirmation is spent when it is used');

// --- results are reported honestly ------------------------------------------
state.pending = '';
state.stopped({type: 'stop', token: plain.token, mode: 'graceful', ok: false, error: '', remaining: true, canForce: true});
assert.match(state.actionError, /still there/i);
assert.ok(!/stopped/i.test(state.actionError) || /did not/.test(state.actionError),
  'a listener that is still bound is never reported as removed');
assert.equal(state.outcome.canForce, true);

// Force is a second, separate decision and only after a graceful attempt.
sent.length = 0;
state.expand(plain.id);
state.outcome = {token: plain.token, canForce: false};
assert.equal(state.review(plain.id, 'force'), false, 'force is not reachable on its own');
assert.deepEqual(sent, []);
state.outcome = {token: plain.token, canForce: true};
assert.equal(state.review(plain.id, 'force'), true);
assert.deepEqual(sent, [{op: 'plan', id: plain.id, mode: 'force'}],
  'force is reviewed again before it is confirmed');

// A cancelled authentication leaves the target running and says so.
state.stopped({type: 'stop', token: plain.token, ok: false, error: 'not-authorized', remaining: true});
assert.match(state.actionError, /cancelled|refused/i);
assert.match(state.failure('gone'), /gone/i);
assert.equal(state.failure(''), '');

// --- one request at a time --------------------------------------------------
state.pending = 'stop';
sent.length = 0;
assert.equal(state.review(plain.id, 'graceful'), false, 'a second action cannot overlap the first');
assert.deepEqual(sent, []);
state.pending = '';
state.queue = Array.from({length: 8}, (_, n) => ({op: 'refresh', n}));
state.send({op: 'refresh', n: 99});
assert.equal(state.queue.length, 8, 'the pending queue is bounded');
assert.match(state.actionError, /Too many/);
state.queue = [];
state.actionError = '';
state.send({op: 'refresh'});
state.send({op: 'refresh'});
assert.equal(sent.filter(m => m.op === 'refresh').length, 1, 'a repeated request collapses');

console.log('Ports addresses, confirmation, staleness, escalation and bounded requests passed');

// Changing scope clears review immediately, even before its delayed reply.
state.pending = '';
state.queue = [];
state.query = 'https://localhost:3000';
state.bindingScope = 'all';
state.accept(snapshot([plain], {query: {text: state.query, scope: 'all', scheme: 'https', explicitScheme: true}}));
state.expanded = plain.id;
state.reviewValid = true;
state.accept(reviewValue);
state.bindingScope = 'network';
state.filtersChanged();
assert.equal(state.plan, null);
assert.equal(state.expanded, '');
assert.equal(rows.count, 0);
assert.equal(state.find(plain.id), null);
state.expanded = plain.id; // A late reply still cannot revive the review.
state.accept(reviewValue);
assert.equal(state.plan, null);
sent.length = 0;
assert.equal(state.confirm(), false);
assert.deepEqual(sent, []);
state.accept(snapshot([plain], {query: {text: state.query, scope: 'all'}}));
assert.equal(rows.count, 0, 'an old query reply cannot restore a hidden row');
state.accept(snapshot([wildcard], {query: {text: state.query, scope: 'network', scheme: 'https', explicitScheme: true}}));
assert.equal(rows.count, 1);
assert.equal(state.scheme(wildcard.id), 'https', 'switching scope preserves the explicit URL scheme');
state.resetFilters();
assert.equal(state.query, '');
assert.equal(state.bindingScope, 'all');
assert.match(source, /onBindingScopeChanged: filtersChanged\(\)/);
assert.match(source, /op: "query", text: store.query, scope: store.bindingScope/);
console.log('Ports composed filters, reset, late snapshots and late confirmations passed');

// --- production wiring ------------------------------------------------------
assert.match(source, /command: \["seele-ports"\]/, 'the store speaks to the native worker');
assert.doesNotMatch(source, /\bss\b|netstat|lsof|nmap/, 'discovery is never a shelled-out probe');
assert.doesNotMatch(source, /XMLHttpRequest|fetch\(/, 'the panel never contacts a listener');
assert.doesNotMatch(panel, /XMLHttpRequest|fetch\(/, 'the panel never contacts a listener');
assert.doesNotMatch(panel, /"http:\/\/" \+|"https:\/\/" \+/, 'URLs are built in Rust, not concatenated in QML');
assert.doesNotMatch(panel, /\budp\b/i, 'UDP is out of scope and is not implied');
assert.match(panel, /ports\.url|store\.url\(/, 'the proposed address comes from the native policy');
assert.match(panel, /panel\.store\.review\(row\.entry\.id, "graceful"\)/, 'Stop opens a confirmation');
assert.match(panel, /panel\.store\.review\(row\.entry\.id, "force"\)/, 'force stop is separately confirmed');
assert.match(panel, /text: "Cancel"/, 'a confirmation can be cancelled');
assert.match(panel, /visible: !!\(row\.outcome && row\.outcome\.canForce\)/,
  'force stop appears only after a graceful attempt reported the listener remaining');
assert.match(panel, /Identify owner/, 'another user\'s listener can be identified deliberately');
assert.match(panel, /visible: !!row\.target\.reason/, 'an unavailable Stop states its reason');
assert.match(panel, /maximumHeight: theme\.portsMaximumHeight/, 'the list is bounded by a theme token');
assert.match(shell, /PortsStore \{\n\s+id: portsStore\n\s+panelOpen: root\.controlPanel === "ports"/,
  'the worker only scans while the panel is open');
assert.match(shell, /label: "Ports"/, 'the inspector opens from the Control Center');
assert.match(shell, /PortsPanel \{ id: portsPanel; theme: root; store: portsStore/);
console.log('Ports production wiring passed');
