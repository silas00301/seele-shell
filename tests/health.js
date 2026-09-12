const {nativeBridge,source:nativeSource}=require("./native-functions.cjs");
const fs=require('node:fs'), vm=require('node:vm'), assert=require('node:assert/strict');
const api=vm.createContext({Bridge:nativeBridge()}); vm.runInContext(nativeSource(fs.readFileSync(process.argv[2],'utf8')),api);
const reg=api.registration({id:'fixture',name:'Fixture',deadline:5000,actions:['retry','settings','restart'],service:'fixture.service',setup:'github'});
let values={fixture:api.publication(reg,{state:'healthy',summary:'Connected',lastSuccess:10000,actions:['settings'],rawPrivateContent:'MUST NOT COPY'},10000)};
assert.equal(api.rows({fixture:reg},values,14999)[0].state,'healthy');
assert.equal(api.rows({fixture:reg},values,15001)[0].state,'stale');
assert.equal(JSON.stringify(values).includes('MUST NOT COPY'),false);
assert.equal(api.rows({},values,16000).length,0,'unregister discards visibility');
assert.equal(api.rows({fixture:reg},{},10000)[0].state,'stale','configured but absent provider stays visible');
for(const value of ['token=abc','Bearer secret','sk-abcdef']) assert.throws(()=>api.publication(reg,{state:'healthy',summary:value},10000));
assert.throws(()=>api.publication(reg,{state:'other',summary:'x'},10000));
assert.throws(()=>api.publication(reg,{state:'healthy',summary:'x',actions:['execute']},10000));
assert.throws(()=>api.registration({id:'fixture',name:'Fixture',service:'bad; touch /tmp/bad'}));
assert.throws(()=>api.registration({id:'__proto__',name:'Fixture'}));
const bad=api.registration({id:'attention',name:'Z attention'});
const healthy=api.publication(reg,{state:'healthy',summary:'Connected'},10000);
const failed=api.publication(bad,{state:'disconnected',summary:'Reconnect'},10000);
assert.deepEqual(Array.from(api.rows({fixture:reg,attention:bad},{fixture:healthy,attention:failed},10000),x=>x.id),['attention','fixture']);
assert.equal(api.publication(reg,{state:'healthy',summary:'Connected',detail:'',lastSuccess:20000},10000).lastSuccess,10000);
console.log('Health registration, bounded payloads, privacy, action types, ordering and stale deadlines passed');
// Execute production store methods: confirmation, invalid actions, replacement,
// unregister and stale asynchronous completion, rather than mirror the code.
const path=require('node:path');
const source=fs.readFileSync(path.join(path.dirname(process.argv[2]),'IntegrationHealthStore.qml'),'utf8');
function method(name) {
 const begin=source.indexOf('  function '+name+'('), brace=source.indexOf('{',begin);
 let end=brace+1, depth=1;
 while(depth){if(source[end]==='{')depth++;if(source[end]==='}')depth--;end++;}
 return source.slice(begin,end);
}
const core=vm.createContext({Health:api,Date,registrations:{},values:{},errors:{},pending:{},handlers:{},serial:0,now:Date.now(),configured(){},openSettings(){},restartFactory:{createObject(){throw Error('not expected')}}});
for(const name of ['configure','publish','complete','act'])vm.runInContext(method(name),core);
Object.defineProperty(core,'rows',{get(){return api.rows(core.registrations,core.values,core.now)}});
core.configure([{id:'fixture',name:'Fixture',actions:['retry'],disruptive:['retry']}]);
core.publish('fixture',{state:'degraded',summary:'Try again',actions:['retry']});
assert.equal(core.act('fixture','retry',false),false,'disruptive action requires confirmation');
assert.equal(core.act('fixture','shell-command',true),false);
let invoked=0;core.handlers.fixture=()=>invoked++;
assert.equal(core.act('fixture','retry',true),true);assert.equal(invoked,1);
assert.equal(core.act('fixture','retry',true),false,'duplicate action blocked');
const token=core.pending.fixture.token;
core.configure([]);core.complete('fixture',token,false);
assert.equal(core.rows.length,0);assert.equal(Object.keys(core.values).length,0);assert.equal(Object.keys(core.errors).length,0);
assert.equal(core.publish('fixture',{state:'healthy',summary:'Late update'}),false);
console.log('Production Health store confirmation, action deduplication, unregister and stale callbacks passed');

// Valid provider names can overlap Object.prototype; inherited properties are
// never registrations, pending actions, handlers or published health metadata.
const constructorReg=api.registration({id:'constructor',name:'Constructor',actions:['retry']});
assert.equal(api.rows({constructor:constructorReg},{},10000)[0].state,'stale');
core.configure([]);
assert.equal(core.publish('constructor',{state:'healthy',summary:'Unregistered'}),false);
core.configure([{id:'constructor',name:'Constructor',actions:['retry']}]);
core.publish('constructor',{state:'degraded',summary:'Retry',actions:['retry']});
assert.equal(core.act('constructor','retry',false),true);
assert.equal(core.errors.constructor,'Action failed. Try again.');
assert.equal(Object.keys(core.pending).length,0);
console.log('Health provider identity collisions do not resolve inherited object properties');

let restartTask;
core.store=core;
core.restartFactory.createObject=(_,task)=>{ restartTask=task;return {running:false}; };
core.configure([{id:'fixture',name:'Fixture',actions:['restart'],service:'fixture.service'}]);
core.publish('fixture',{state:'degraded',summary:'Restart',actions:['restart']});
assert.equal(core.act('fixture','restart',false),true);
assert.deepEqual(Array.from(restartTask.command),['seele-control','restart-user-service','fixture.service']);
console.log('Health restarts use the bounded native service action');

// Equal display names keep provider registration order across Rust serialization.
{
 const b=api.registration({id:'zeta',name:'Same'}),a=api.registration({id:'alpha',name:'Same'});
 assert.deepEqual(Array.from(api.rows({zeta:b,alpha:a},{},0),x=>x.id),['zeta','alpha']);
 for (const summary of ['🦀'.repeat(121),'x\u0000']) assert.throws(()=>api.publication(reg,{state:'healthy',summary},0));
}
