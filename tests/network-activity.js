const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const source = fs.readFileSync(process.argv[2], 'utf8');
function methods(text) {
  const out = [];
  const starts = /^  function \w+\(/gm;
  let match;
  while ((match = starts.exec(text))) {
    let index = text.indexOf('{', match.index), depth = 0;
    for (; index < text.length; index++) {
      if (text[index] === '{') depth++;
      else if (text[index] === '}' && --depth === 0) break;
    }
    out.push(text.slice(match.index, index + 1));
  }
  return out.join('\n');
}
const processes = [];
const state = vm.createContext({panelOpen:false, snapshot:{rows:[]}, error:'', selectedId:'', received:false, generation:0, worker:null,
  workerComponent:{createObject(owner, properties) {
    const p = {...properties, running:false, destroyed:false, sent:[], write(value) { this.sent.push(JSON.parse(value)) }, destroy() {this.destroyed=true}};
    processes.push(p); return p;
  }}
});
state.store = state;
Object.defineProperty(state, 'rows', {get() {return state.snapshot.rows}});
vm.runInContext(methods(source),state);
const sample = {version:1,rows:[{id:'2:1',name:'eth0',state:'Up'}],elapsed:0};
state.panelOpen=true; state.clear(); state.start();
const old = state.worker;
state.accept(sample, old.session);
assert.equal(state.selectedId,'2:1');
state.reset(); assert.equal(old.sent[0].op,'reset');
// Close destroys and disables the actual process instance; no counters survive.
state.panelOpen=false; state.clear();
assert.equal(old.running,false); assert.equal(old.destroyed,true); assert.equal(state.rows.length,0);
state.accept(sample,old.session); assert.equal(state.rows.length,0);
// Reopening creates a distinct worker, and neither late stdout nor onExited
// from the old instance can change the freshly opened session.
state.panelOpen=true; state.clear(); state.start();
const fresh=state.worker;
assert.notEqual(fresh,old);
state.accept(sample,old.session); state.failed(old.session);
assert.equal(state.rows.length,0); assert.equal(state.error,'');
state.accept(sample,fresh.session);
assert.equal(state.rows.length,1);
state.accept({version:1, rows:[{id:'3:1',name:'wifi0',state:'Down'}]},fresh.session);
assert.equal(state.selectedId,'2:1');
state.select(0); assert.equal(state.selectedId,'3:1');
state.failed(fresh.session); assert.equal(state.rows.length,0); assert.ok(state.error.includes('stopped'));
state.retry(); assert.equal(fresh.destroyed,true); assert.notEqual(state.worker,fresh); assert.equal(state.error,'');
console.log('network activity store: process ownership, late data/exit, selection and Reset passed');
