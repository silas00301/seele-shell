const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const [storePath, helperPath] = process.argv.slice(2);
const source = fs.readFileSync(storePath, 'utf8');
const helpers = {};
vm.createContext(helpers);
vm.runInContext(fs.readFileSync(helperPath, 'utf8'), helpers);

// The production callbacks are lifted out of the QML component and run against
// the same message shapes the worker emits, so what is checked here is the code
// the application actually ships.
function makeStore() {
  const writes = [];
  const loads = [];
  const store = {
    config: { configured: true, writable: true, legacy: 0 },
    ui: { sidebar: 0, collapsed: false },
    notes: [], trash: [], drafts: [], vaults: [], browse: null, migration: null,
    unreadable: 0, ready: false, started: false, serial: 0, pending: {},
    path: '', openPath: '', draft: '', baseline: '', audio: [],
    trashView: false, trashId: '',
    saveState: 'idle', conflict: null, pendingTrash: false,
    error: '', warning: '', notice: '',
    recording: false, stopping: false, recordingDuration: 0, recordingError: '',
    Notes: helpers,
    worker: { write: value => writes.push(JSON.parse(value)) },
    autosave: { stop() {}, restart() {} },
    loaded: (text, keep) => loads.push({ text, keep }),
    level() {}, recorded() {},
  };
  store.store = store;
  Object.defineProperty(store, 'note', { get: () => store.find(store.path) });
  vm.createContext(store);
  vm.runInContext(
    source.slice(source.indexOf('  function find('), source.indexOf('  Timer { id: autosave;')),
    store,
  );
  store.receive(JSON.stringify({
    ready: true,
    config: store.config,
    notes: [{ path: 'Inbox/A.md', name: 'A', title: 'A', excerpt: 'old', audio: 0, updated: 1, hash: 'h1' }],
    trash: [],
  }));
  return { store, writes, loads };
}

// A launch opens an empty document rather than the last note that was read.
{
  const { store, loads } = makeStore();
  assert.equal(store.path, '');
  assert.equal(store.saveState, 'idle');
  assert.deepEqual(loads.at(-1), { text: '', keep: false });
}

// A capture with nothing in it is never written, so an abandoned draft leaves
// no empty file in the vault.
{
  const { store, writes } = makeStore();
  store.edit('   \n  ');
  store.flush();
  assert.equal(writes.length, 0, 'an empty draft is not written');
  assert.equal(store.saveState, 'idle');
  store.edit('First thought');
  store.flush();
  const created = writes.pop();
  assert.equal(created.action, 'save');
  assert.equal(created.path, '', 'a new capture has no filename until it is written');
  assert.equal(created.baseline, '');
}

// Text typed while a save is in flight is not acknowledged by that save.
{
  const { store, writes } = makeStore();
  store.select('Inbox/A.md');
  store.receive(JSON.stringify({
    request: writes.pop().request, ok: true,
    note: { path: 'Inbox/A.md', name: 'A', title: 'A', excerpt: 'old', audio: 0, updated: 1, hash: 'h1' },
    text: 'old', hash: 'h1', audio: [],
  }));
  assert.equal(store.draft, 'old');
  assert.equal(store.baseline, 'h1');
  store.edit('first');
  store.flush();
  const inFlight = writes.pop();
  store.edit('second');
  store.receive(JSON.stringify({
    request: inFlight.request, ok: true,
    note: { path: 'Inbox/A.md', name: 'A', title: 'A', excerpt: 'first', audio: 0, updated: 2, hash: 'h2' },
    hash: 'h2',
    notes: [{ path: 'Inbox/A.md', name: 'A', title: 'A', excerpt: 'first', audio: 0, updated: 2, hash: 'h2' }],
    trash: [],
  }));
  assert.equal(store.draft, 'second');
  assert.equal(store.saveState, 'dirty', 'the newer text is still unsaved');
  assert.equal(store.baseline, 'h2');
}

// A failed write keeps the text, says so, and is not followed by a trash.
{
  const { store, writes } = makeStore();
  store.select('Inbox/A.md');
  store.receive(JSON.stringify({
    request: writes.pop().request, ok: true,
    note: { path: 'Inbox/A.md', audio: 0, updated: 1, hash: 'h1', name: 'A', title: 'A', excerpt: '' },
    text: 'old', hash: 'h1', audio: [],
  }));
  store.edit('do not lose me');
  store.trashNote();
  const attempted = writes.pop();
  assert.equal(attempted.action, 'save', 'the note is saved before it is moved');
  store.receive(JSON.stringify({ request: attempted.request, ok: false, error: 'Disk full' }));
  assert.equal(writes.length, 0, 'a failed write is never followed by a trash');
  assert.equal(store.draft, 'do not lose me');
  assert.equal(store.saveState, 'failed');
  assert.equal(store.error, 'Disk full');
  assert.equal(store.pendingTrash, false);
}

// A save that succeeds after a deferred trash lets the trash through.
{
  const { store, writes } = makeStore();
  store.select('Inbox/A.md');
  store.receive(JSON.stringify({
    request: writes.pop().request, ok: true,
    note: { path: 'Inbox/A.md', audio: 0, updated: 1, hash: 'h1', name: 'A', title: 'A', excerpt: '' },
    text: 'old', hash: 'h1', audio: [],
  }));
  store.edit('keep');
  store.trashNote();
  const saved = writes.pop();
  store.receive(JSON.stringify({
    request: saved.request, ok: true,
    note: { path: 'Inbox/A.md', audio: 0, updated: 2, hash: 'h2', name: 'A', title: 'A', excerpt: 'keep' },
    hash: 'h2', notes: [], trash: [],
  }));
  assert.equal(writes.pop().action, 'trash');
}

// Both versions survive a conflict, and neither is written over on its own.
{
  const { store, writes } = makeStore();
  store.select('Inbox/A.md');
  store.receive(JSON.stringify({
    request: writes.pop().request, ok: true,
    note: { path: 'Inbox/A.md', audio: 0, updated: 1, hash: 'h1', name: 'A', title: 'A', excerpt: '' },
    text: 'old', hash: 'h1', audio: [],
  }));
  store.edit('mine');
  store.flush();
  store.receive(JSON.stringify({
    request: writes.pop().request, ok: true,
    conflict: { path: 'Inbox/A.md', text: 'theirs', hash: 'h9' },
  }));
  assert.equal(store.saveState, 'conflict');
  assert.equal(store.draft, 'mine', 'the local text is untouched');
  assert.equal(store.conflict.text, 'theirs');
  store.resolve('copy');
  const resolution = writes.pop();
  assert.equal(resolution.action, 'resolve');
  assert.equal(resolution.mode, 'copy');
  assert.equal(resolution.text, 'mine');
}

// An external edit refreshes a clean note and leaves a dirty one alone.
{
  const { store, writes } = makeStore();
  store.select('Inbox/A.md');
  store.receive(JSON.stringify({
    request: writes.pop().request, ok: true,
    note: { path: 'Inbox/A.md', audio: 0, updated: 1, hash: 'h1', name: 'A', title: 'A', excerpt: '' },
    text: 'old', hash: 'h1', audio: [],
  }));
  store.receive(JSON.stringify({
    changed: true, trash: [],
    notes: [{ path: 'Inbox/A.md', name: 'A', title: 'A', excerpt: 'x', audio: 0, updated: 5, hash: 'h5' }],
  }));
  assert.equal(writes.pop().action, 'read', 'a clean note follows the file');
  store.edit('typing');
  store.receive(JSON.stringify({
    changed: true, trash: [],
    notes: [{ path: 'Inbox/A.md', name: 'A', title: 'A', excerpt: 'y', audio: 0, updated: 6, hash: 'h6' }],
  }));
  assert.equal(writes.length, 0, 'a note being typed into is never reloaded underneath the caret');
  assert.equal(store.draft, 'typing');
}

// Re-reading the note already on screen keeps the caret; a different note does not.
{
  const { store, writes, loads } = makeStore();
  store.select('Inbox/A.md');
  const document = {
    request: writes.pop().request, ok: true,
    note: { path: 'Inbox/A.md', audio: 0, updated: 1, hash: 'h1', name: 'A', title: 'A', excerpt: '' },
    text: 'old', hash: 'h1', audio: [],
  };
  store.receive(JSON.stringify(document));
  assert.equal(loads.at(-1).keep, false, 'arriving at a note starts at the top');
  store.receive(JSON.stringify({
    changed: true, trash: [],
    notes: [{ path: 'Inbox/A.md', name: 'A', title: 'A', excerpt: 'x', audio: 0, updated: 5, hash: 'h5' }],
  }));
  store.receive(JSON.stringify(Object.assign({}, document, {
    request: writes.pop().request, text: 'refreshed', hash: 'h5',
  })));
  assert.equal(loads.at(-1).keep, true, 'a refresh of the same note keeps the caret');
  assert.equal(loads.at(-1).text, 'refreshed');
}

// A file that vanished is not recreated by the autosave already in flight.
{
  const { store, writes } = makeStore();
  store.select('Inbox/A.md');
  store.receive(JSON.stringify({
    request: writes.pop().request, ok: true,
    note: { path: 'Inbox/A.md', audio: 0, updated: 1, hash: 'h1', name: 'A', title: 'A', excerpt: '' },
    text: 'old', hash: 'h1', audio: [],
  }));
  store.edit('still typing');
  store.flush();
  store.receive(JSON.stringify({ request: writes.pop().request, ok: true, gone: true, path: 'Inbox/A.md' }));
  assert.equal(store.saveState, 'gone');
  assert.equal(store.draft, 'still typing');
  assert.equal(writes.length, 0);
}

// A worker that disconnects keeps the draft and stops claiming it was saved.
{
  const { store, writes } = makeStore();
  store.select('Inbox/A.md');
  store.receive(JSON.stringify({
    request: writes.pop().request, ok: true,
    note: { path: 'Inbox/A.md', audio: 0, updated: 1, hash: 'h1', name: 'A', title: 'A', excerpt: '' },
    text: 'old', hash: 'h1', audio: [],
  }));
  store.edit('unsent');
  store.flush();
  writes.pop();
  store.ready = false;
  store.saveState = 'dirty';
  store.started = true;
  store.receive(JSON.stringify({ ready: true, config: store.config, notes: [], trash: [] }));
  assert.equal(store.draft, 'unsent', 'a reconnect is not a launch and does not clear the editor');
  assert.equal(store.path, 'Inbox/A.md');
}

// A recovery draft is only kept for text the vault could not take.
{
  const { store, writes } = makeStore();
  store.select('Inbox/A.md');
  store.receive(JSON.stringify({
    request: writes.pop().request, ok: true,
    note: { path: 'Inbox/A.md', audio: 0, updated: 1, hash: 'h1', name: 'A', title: 'A', excerpt: '' },
    text: 'old', hash: 'h1', audio: [],
  }));
  store.edit('saved fine');
  store.flush();
  store.receive(JSON.stringify({
    request: writes.pop().request, ok: true,
    note: { path: 'Inbox/A.md', audio: 0, updated: 2, hash: 'h2', name: 'A', title: 'A', excerpt: '' },
    hash: 'h2', notes: [], trash: [],
  }));
  store.keepDraft();
  assert.equal(writes.length, 0, 'a note that saved cleanly has nothing to recover');
  store.saveState = 'failed';
  store.keepDraft();
  assert.equal(writes.pop().action, 'draft', 'text the vault refused is kept');
}

// A new recording becomes an embed rather than a note attribute.
{
  const { store } = makeStore();
  let embedded = '';
  store.recorded = name => { embedded = name; };
  store.receive(JSON.stringify({ audio: [{ index: 0, name: 'Voice memo 1.wav', path: '/v/A/Voice memo 1.wav', duration: 3000 }] }));
  assert.equal(store.audio.length, 1);
  assert.equal(store.audio[0].path, '/v/A/Voice memo 1.wav');
  assert.equal(embedded, '');
}

console.log('Notes store launch, save race, failure, conflict, external refresh and reconnect checks passed');
