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
// The worker is started only by opening the panel; no other entry exists.
assert.equal((source.match(/createObject\(/g) || []).length, 1);
assert.match(source, /command: \["seele-sensors"\]/);
const processes = [];
const state = vm.createContext({panelOpen:false, snapshot:{rows:[]}, error:'', received:false, generation:0, worker:null,
  workerComponent:{createObject(owner, properties) {
    const p = {...properties, running:false, destroyed:false, sent:[], write(value) { this.sent.push(JSON.parse(value)) }, destroy() {this.destroyed=true}};
    processes.push(p); return p;
  }}
});
state.store = state;
Object.defineProperty(state, 'rows', {get() {return state.snapshot.rows}});
vm.runInContext(methods(source), state);
const sample = {version:1, rows:[{id:'k10temp@/pci', title:'CPU', detail:'k10temp', readings:[]}], summary:'Hottest 48 °C · CPU Tctl', elapsed:0};
// Nothing starts while the panel is closed.
state.start(); assert.equal(processes.length, 0);
state.panelOpen=true; state.clear(); state.start();
const old = state.worker;
assert.equal(old.running, true);
state.accept(sample, old.session);
assert.equal(state.rows.length, 1); assert.equal(state.received, true);
state.accept({version:2, rows:[]}, old.session); assert.equal(state.rows.length, 1);
state.accept({version:1, rows:'x'}, old.session); assert.equal(state.rows.length, 1);
state.reset(); assert.deepEqual(old.sent, [{op:'reset'}]);
// Closing destroys the process and forgets every reading and peak.
state.panelOpen=false; state.clear();
assert.equal(old.running,false); assert.equal(old.destroyed,true); assert.equal(state.rows.length,0); assert.equal(state.received,false);
state.accept(sample, old.session); assert.equal(state.rows.length,0);
state.reset(); assert.equal(old.sent.length, 1);
// A reopened panel owns a new worker; the old one's late output and exit are refused.
state.panelOpen=true; state.clear(); state.start();
const fresh=state.worker;
assert.notEqual(fresh, old);
state.accept(sample, old.session); state.failed(old.session);
assert.equal(state.rows.length,0); assert.equal(state.error,'');
state.accept(sample, fresh.session); assert.equal(state.rows.length,1);
state.accept({...sample, error:"The kernel's sensor interface is unavailable", rows:[]}, fresh.session);
assert.equal(state.error, "The kernel's sensor interface is unavailable");
state.failed(fresh.session); assert.equal(state.rows.length,0); assert.ok(state.error.includes('stopped'));
state.retry(); assert.equal(fresh.destroyed,true); assert.notEqual(state.worker,fresh); assert.equal(state.error,'');
console.log('sensors store: open-only worker, late data/exit, reset and retry passed');
