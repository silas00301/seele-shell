const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const path = require('node:path');
const {nativeBridge} = require('./native-functions.cjs');
const source = fs.readFileSync(process.argv[2], 'utf8');
const methods = [...source.matchAll(/^  function \w+\([^\n]*\) \{\n[\s\S]*?^  \}/gm)].map(m => m[0]).join('\n');
const writes = [];
let completed = 0;
const state = vm.createContext({
  Bridge: nativeBridge(), healthSuccess: 0, healthPublished(){},
  connected: false, configured: false, ready: true, entities: [], preferences: [], catalog: [], error: '',
  pending: {}, requests: {}, serial: 0, settingsPending: false, url: '', summary: '', summaryText: '',
  worker: {running: true, write(text) { writes.push(JSON.parse(text)); }},
  restart: {start(){}}, setupComplete(){ completed++; }, Date,
});
vm.runInContext(methods, state);
const projection = source.match(/readonly property var projection: ([^\n]+)/)[1];
Object.defineProperty(state,"projection",{get(){return vm.runInContext(projection,state)}});
const entity = {entity_id:'light.desk',state:'off',controllable:true,available:true,room:'Office'};
const other = {...entity,entity_id:'light.ceiling'};
const snapshot = {ready:true,configured:true,connected:true,entities:[entity,other],preferences:[{entity_id:entity.entity_id,name:'',room:'',favorite:false}],pending:{},error:''};
state.receive(JSON.stringify(snapshot));
const previousEntities = state.entities;
state.receive(JSON.stringify(snapshot));
assert.equal(state.entities, previousEntities, 'unchanged snapshots retain model identity');
state.setState(entity, 'on');
state.setState(other, 'on');
assert.equal(writes.length, 2, 'different devices can change concurrently');
state.setState(entity, 'off');
assert.equal(writes.length, 2, 'a pending device cannot enqueue opposite intent');
state.receive(JSON.stringify({...snapshot,connected:false,pending:{}}));
state.setState(entity, 'on');
assert.equal(writes.length, 2, 'stale devices cannot be controlled');
assert.equal(state.entities[0].entity_id,entity.entity_id);
state.receive(JSON.stringify(snapshot));
state.setState(entity, 'toggle');
state.setState({...entity,controllable:false}, 'on');
assert.equal(writes.length, 2);
state.setup('https://home.test', 'never-retain-this-token');
assert.equal(state.settingsPending,true);
assert.ok(!JSON.stringify(state.requests).includes('never-retain-this-token'));
state.receive(JSON.stringify({request:writes.at(-1).request,ok:true}));
assert.equal(completed,1);
assert.equal(state.settingsPending,false);
state.edit(entity.entity_id,'favorite',true);
assert.equal(writes.at(-1).entities[0].favorite,true);
state.receive(JSON.stringify({request:writes.at(-1).request,ok:false,error:'Save failed'}));
assert.equal(state.settingsPending,false);
assert.equal(state.preferences[0].favorite,false,'failed save keeps persisted preferences');
state.entities = [{...entity,favorite:true},{...other,favorite:false}, {entity_id:'sensor.temperature',name:'Temperature',room:'Office',state:'21',unit:'°C',available:true}];
assert.deepEqual(Array.from(state.rows(),row=>row.heading||row.entity_id), ['Favorites','light.desk','Office','light.ceiling']);
assert.equal(state.rows().find(row=>row.heading==='Office').detail,'21°C');
state.entities = [{entity_id:'sensor.temp',name:'Temperature',room:'Office',state:'unavailable',unit:'°C',available:false,favorite:true}, {entity_id:'sensor.humidity',name:'Humidity',room:'Office',state:'50',unit:'%',device_class:'humidity',available:true}];
assert.deepEqual(Array.from(state.rows(),row=>row.heading||row.entity_id), ['Office'], 'environment readings only appear in room headers, including favorite and unavailable sensors');
assert.ok(state.rows()[0].detail.includes('unavailable'));
assert.ok(state.rows()[0].detail.includes('50%'));
state.entities = [{...entity,favorite:true},{...other,favorite:false},{...entity,entity_id:'light.third',favorite:true}];
state.preferences = state.entities.map(item=>({entity_id:item.entity_id}));
assert.equal(state.moveTarget('light.desk',1),2,'reordering skips entries from other groups');
assert.equal(state.moveTarget('light.ceiling',1),-1);
state.move('light.desk',1);
assert.deepEqual(Array.from(writes.at(-1).entities,item=>item.entity_id),['light.third','light.ceiling','light.desk']);
state.entities = [{...entity,favorite:false}, {entity_id:'sensor.temp',room:'Office',unit:'°C',favorite:false}, {...other,favorite:false}];
state.preferences = state.entities.map(item=>({entity_id:item.entity_id}));
assert.equal(state.moveTarget('light.desk',1),2,'device ordering skips readings that appear only in the header');
state.stopped();
assert.equal(state.connected,false);
assert.equal(state.ready,false);
assert.equal(Object.keys(state.pending).length,0);

const qml = fs.readFileSync(path.join(path.dirname(process.argv[2]), 'HomeAssistantPanel.qml'), 'utf8');
const row = qml.split('id: homeRow')[1];
const keyBody = row.match(/^(\s*)Keys\.onPressed: event => \{\n([\s\S]*?)^\1\}/m)[2];
let requests = 0;
const keys = vm.createContext({changeState(){requests++},Qt:{Key_Return:13,Key_Enter:14,Key_Space:32,ControlModifier:1,AltModifier:2,MetaModifier:4}});
vm.runInContext('function press(event) {' + keyBody + '}',keys);
keys.press({key:32,modifiers:0,isAutoRepeat:false});
keys.press({key:32,modifiers:0,isAutoRepeat:true});
keys.press({key:13,modifiers:1,isAutoRepeat:false});
keys.press({key:14,modifiers:0,isAutoRepeat:false});
assert.equal(requests,2);
console.log('Home Assistant concurrent controls, stale state, credential lifetime, preferences and keyboard intent passed');

assert.equal(state.moveTarget("light.desk",0),-1,"zero offset cannot enter a nonterminating search");

const Models=require('./list-models.cjs')();
const rows={values:[],get count(){return this.values.length},get(i){return this.values[i]},insert(i,v){this.values.splice(i,0,v)},move(i,j){this.values.splice(j,0,this.values.splice(i,1)[0])},remove(i,n){this.values.splice(i,n)},setProperty(i,k,v){this.values[i][k]=v}};
const panelMethods=vm.createContext({Models});
const reconcile=qml.match(/^  function reconcile\([^\n]*\) \{\n[\s\S]*?^  \}/m)[0];vm.runInContext(reconcile,panelMethods);
const entries=[{heading:'Office',detail:'21°C'},entity,other];panelMethods.reconcile(rows,entries);const kept=rows.get(1);
panelMethods.reconcile(rows,[other,entries[0],entity]);assert.equal(rows.get(2),kept,'shared keyed-role adapter preserves Home Assistant delegate identity');
panelMethods.reconcile(rows,[{...entity,state:'on'}]);assert.equal(rows.count,1);assert.equal(rows.get(0).payload.state,'on');
console.log('Home Assistant shared header/entity model reconciliation passed');
