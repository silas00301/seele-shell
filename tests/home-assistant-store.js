const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const source = fs.readFileSync(process.argv[2], 'utf8');
const methods = [...source.matchAll(/^  function \w+\([^\n]*\) \{\n[\s\S]*?^  \}/gm)].map(m => m[0]).join('\n');
const callbacks = [];
const state = vm.createContext({
  connected: false, configured: false, entities: [], error: '', received: false,
  pendingEntity: '', pendingValue: '', generation: 0, requestPending:false, timedOut:false,
  worker: {running:false,command:[]}, watchdog:{restart(){},stop(){}},
  Qt:{callLater(callback){callbacks.push(callback)}}
});
state.store = state;
Object.defineProperty(state, 'busy', {get(){return state.requestPending || state.worker.running}});
vm.runInContext(methods, state);
const entity = {entity_id:'light.desk',state:'off',controllable:true,available:true};
const snapshot = JSON.stringify({configured:true,connected:true,entities:[entity],error:''});
const exit = () => {state.worker.running=false;state.finish()};
const flush = () => {while(callbacks.length) callbacks.shift()()};
state.refresh();
assert.deepEqual(Array.from(state.worker.command), ['seele-home-assistant', 'status']);
state.accept(snapshot);
exit();
state.setState(entity, 'on');
assert.equal(state.worker.command[1], 'status', 'exit keeps busy until streams are processed');
flush();
state.setState(entity, 'on');
assert.deepEqual(Array.from(state.worker.command), ['seele-home-assistant', 'set', 'light.desk', 'on']);
state.setState(entity, 'off');
assert.equal(state.pendingValue, 'on', 'rapid clicks cannot enqueue opposite intent');
state.accept(JSON.stringify({configured:true,connected:false,error:'Disconnected'}));
exit();flush();
assert.equal(state.entities[0].entity_id, 'light.desk', 'failed refresh preserves explicitly stale display');
state.setState(entity, 'off');
assert.equal(state.pendingValue, '', 'disconnected entities cannot be controlled');
state.connected = true;
state.setState({...entity,controllable:false}, 'off');
assert.equal(state.pendingValue, '', 'read-only entries cannot be controlled');
state.setState(entity, 'toggle');
assert.equal(state.pendingValue, '', 'only explicit on/off is accepted');
state.setState(entity, 'on');
state.worker.running=false; // failed startup without an exit signal
state.timeout();
assert.equal(state.busy,false);
assert.equal(state.pendingEntity,'');
state.accept(snapshot);
assert.equal(state.connected,false,'late stream cannot overwrite watchdog failure');
state.finish();
state.refresh(); // new request before the old deferred cleanup
assert.equal(state.requestPending,true);
flush();
assert.equal(state.requestPending,true,'old cleanup cannot clear new operation');
state.accept('bad json');
assert.equal(state.connected, false);
exit();flush();
state.refresh();
state.accept(JSON.stringify({configured:false,connected:false,entities:[],error:''}));
exit();flush();
assert.equal(state.configured, false);
assert.equal(state.entities.length, 0);
console.log('Home Assistant store guards, stale state and process lifecycle passed');

const qml = fs.readFileSync(require("node:path").join(require("node:path").dirname(process.argv[2]), "shell.qml"), "utf8");
const row = qml.split("id: homeAssistantRow")[1];
// Match the handler's closing brace by its own indent, so restyling the row
// cannot silently drop this check by moving the delegate a level deeper.
const keyBody = row.match(/^(\s*)Keys\.onPressed: event => \{\n([\s\S]*?)^\1\}/m)[2];
let requests = 0;
const keys = vm.createContext({changeState(){requests++},Qt:{Key_Return:13,Key_Enter:14,Key_Space:32,ControlModifier:1,AltModifier:2,MetaModifier:4}});
vm.runInContext("function press(event) {" + keyBody + "}",keys);
keys.press({key:32,modifiers:0,isAutoRepeat:false});
keys.press({key:32,modifiers:0,isAutoRepeat:true});
keys.press({key:13,modifiers:1,isAutoRepeat:false});
keys.press({key:14,modifiers:0,isAutoRepeat:false});
assert.equal(requests,2,"device controls ignore held-key repeats and modified shortcuts");
console.log("Home Assistant deliberate keyboard intent passed");
