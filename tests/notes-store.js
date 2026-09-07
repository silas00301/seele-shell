const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const [storePath, helperPath] = process.argv.slice(2);
const source = fs.readFileSync(storePath, 'utf8');
const helpers = {};
vm.createContext(helpers);
vm.runInContext(fs.readFileSync(helperPath, 'utf8'), helpers);
const writes = [];
const store = {
  notes: [], selected: '', error: '', warning: '', ready: false, serial: 0, pending: {}, pendingTrash: null, recording: false, recordingNote: '',
  Notes: helpers, worker: { write: value => writes.push(JSON.parse(value)) },
  autosave: { stop() {}, restart() {} }, created() {},
};
store.store = store;
Object.defineProperty(store, 'current', {get: () => store.find(store.selected)});
vm.createContext(store);
vm.runInContext(source.slice(source.indexOf('  function find('), source.indexOf('  Timer { id: autosave;')), store);
store.receive(JSON.stringify({ready:true,notes:[{id:'a',title:'A',body:'old',memos:[],updated:1}]}));
store.edit('body', 'first'); store.flush();
const first = writes.pop();
store.edit('body', 'second');
store.receive(JSON.stringify({ok:true,request:first.request,note:{id:'a',title:'A',body:'first',memos:[],updated:2}}));
assert.equal(store.current.body, 'second');
assert.equal(store.current._dirty, true);
const second = writes.pop();
assert.equal(second.body, 'second', 'new edits are flushed after the old save acknowledgement');
store.receive(JSON.stringify({ok:false,request:second.request,error:'Disk full'}));
assert.equal(store.current.body, 'second');
assert.equal(store.current._dirty, true);
assert.equal(store.error, 'Disk full');
store.pending = {}; store.ready = false;
store.receive(JSON.stringify({ready:true,notes:[{id:'a',title:'A',body:'first',memos:['memo'],updated:2}]}));
assert.equal(store.current.body, 'second', 'reconnect cannot overwrite the draft');
assert.equal(store.current.memos.length, 1);
const retried = writes.pop();
store.receive(JSON.stringify({ok:true,request:retried.request,note:{id:'a',title:'A',body:'second',memos:['memo'],updated:3}}));
assert.equal(store.current._dirty, false);

store.edit('body', 'do not lose me');
store.trash(false);
const beforeTrash = writes.pop();
assert.equal(beforeTrash.action, 'save');
store.receive(JSON.stringify({ok:false,request:beforeTrash.request,error:'Disk full'}));
assert.equal(writes.length, 0, 'failed writes must not be followed by trash');
assert.equal(store.current.body, 'do not lose me');

console.log('Notes QML store save, failure, reconnect, and trash checks passed');
