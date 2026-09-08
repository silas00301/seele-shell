const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const helper = vm.createContext({});
vm.runInContext(fs.readFileSync(process.argv[2], 'utf8'), helper);
const title = '--primary $(do not run) `literal` <b>title</b>\n第二行';
const entry = {id:42, summary:title, body:'<b>Hello</b><br><a href="https://hidden.invalid/secret">World</a> &amp; &#x1f680;'};
assert.equal(helper.payload(entry, 'title').value, title, 'title is preserved verbatim');
assert.equal(helper.payload(entry, 'body').value, 'Hello\nWorld & 🚀', 'message copies visible text and entities');
assert.equal(helper.payload({}, 'body').ok, false);
assert.equal(helper.payload(entry, 'actions').ok, false);
assert.equal(helper.payload(null, 'title').ok, false);
assert.equal(helper.payload({body:'x'.repeat(262145)}, 'body').ok, false, 'oversized text is rejected instead of truncated');
assert.equal(helper.payload({body:'x'.repeat(262144)}, 'body').value.length, 262144);
const source = fs.readFileSync(process.argv[3], 'utf8');
function method(name) {
  const start = source.indexOf('function ' + name + '(');
  assert.ok(start >= 0);
  const brace = source.indexOf('{', start);
  let depth = 1, end = brace + 1;
  while (depth && end < source.length) { if (source[end] === '{') depth++; if (source[end] === '}') depth--; end++; }
  return source.slice(start, end);
}
const store = vm.createContext({
  NotificationCopy: helper, status:'idle', key:'', message:'', requestId:0, worker:null,
  feedback:{stop(){},restart(){}}, watchdog:{stop(){},restart(){}},
  workerFactory:{createObject(parent, properties) { return {...properties, running:false, destroyed:false, destroy(){this.destroyed = true;}}; }}
});
store.clipboard = store;
Object.defineProperty(store, 'pending', {get(){return store.status === 'pending';}});
for (const name of ['copy','complete']) vm.runInContext(method(name), store);
assert.equal(store.copy(entry, 'title'), true);
const first = store.worker;
assert.equal(first.payload, title);
assert.equal(store.key, '42:title');
assert.equal(store.copy(entry, 'body'), false, 'only one clipboard transfer at a time');
first.running = false;
assert.equal(store.copy(entry, 'body'), false, 'deferred exit still holds pending state');
store.complete(first.token, true, '');
assert.equal(store.status, 'success');
assert.equal(store.message, 'Copied');
assert.equal(first.payload, '', 'payload is cleared on completion');
assert.equal(first.destroyed, true);
assert.equal(store.copy(entry, 'body'), true);
const second = store.worker;
store.complete(first.token, false, 'late');
assert.equal(store.status, 'pending', 'old exit cannot finish a new request');
store.complete(second.token, false, 'Clipboard did not respond. Try again.');
assert.equal(store.status, 'error', 'startup or hung process recovers via watchdog');
assert.equal(second.destroyed, true);
assert.equal(second.payload, '');
assert.equal(store.copy({}, 'body'), false);
assert.equal(store.message, 'Nothing to copy.');
store.workerFactory.createObject = () => null;
assert.equal(store.copy(entry, 'body'), false);
assert.equal(store.status, 'error');
const writes = [];
const clipboardProcess = vm.createContext({payload:title, stdinEnabled:true, write(value){writes.push(value);}});
vm.runInContext(method('send'), clipboardProcess);
clipboardProcess.send();
assert.deepEqual(writes, [title], 'untrusted payload is written exactly once to stdin');
assert.equal(clipboardProcess.payload, '');
assert.equal(clipboardProcess.stdinEnabled, false, 'stdin closes after exact text, without adding a newline');
assert.match(source, /command: \["wl-copy", "--type", "text\/plain;charset=utf-8"\]/);
assert.match(source, /interval: 5000/);
assert.doesNotMatch(source, /execDetached|dismiss|invoke|console\./);
const shell = fs.readFileSync(process.argv[4], 'utf8');
assert.match(shell, /notificationClipboard.copy\(notificationEntry.entry, modelData\)/);
assert.match(shell, /localComplete: selected && notificationClipboard.status === "success"/);
assert.match(shell, /localFailed: selected && notificationClipboard.status === "error"/);
console.log('notification copy payload, stdin, async completion, failure, watchdog and stale request checks passed');
