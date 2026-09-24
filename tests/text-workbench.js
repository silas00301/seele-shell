// Execute production callbacks with independently owned fake Process objects.
const fs = require('fs');
const vm = require('vm');
const assert = require('assert/strict');
const source = fs.readFileSync(process.argv[2], 'utf8');
function body(marker) {
  let start=source.indexOf('{',source.indexOf(marker)),level=1,i=start+1;
  for (;level && i<source.length;i++) {
    if(source[i]==='{')level++;
    if(source[i]==='}')level--;
  }
  return source.slice(start+1,i-1);
}
const writes=[], replies=[];
const session={activeTransport:null,
  receivePaste(...args){replies.push(['paste',...args]);},
  receiveCopy(...args){replies.push(['copy',...args]);}};
function create(action,token,payload) {
  const task={action,token,payload,completed:false,running:true,stdinEnabled:action==='copy',
    deadline:{stop(){},restart(){}},write(text){assert.equal(this.stdinEnabled,true);writes.push(text);},destroy(){this.destroyed=true;}};
  const context=vm.createContext({session,task});
  for(const key of Object.keys(task)) Object.defineProperty(context,key,{get:()=>task[key],set:value=>task[key]=value,configurable:true});
  task.finish=response=>{context.response=response;vm.runInContext('(function(){'+body('function finish(')+'})()',context);};
  task.started=()=>vm.runInContext(body('onStarted:'),context);
  session.activeTransport=task;
  return task;
}
let paste=create('paste',1,'');
paste.finish({ok:true,text:'original'});
for(const [token,text] of [[2,'one\n'],[3,'Grüße 🦀']]) {
  let task=create('copy',token,text);task.started();
  assert.equal(task.stdinEnabled,false);assert.equal(task.payload,'');
  task.finish({ok:true});assert.equal(task.destroyed,true);
}
assert.deepEqual(writes,['one\n','Grüße 🦀']);
let old=create('paste',4,'');
old.finish({ok:false,error:'Clipboard handoff timed out.'});
let fresh=create('paste',5,'');
const count=replies.length;
old.finish({ok:true,text:'stale private text'});
assert.equal(replies.length,count);
assert.equal(session.activeTransport,fresh);
fresh.finish({ok:true,text:'new text'});
assert.deepEqual(replies.at(-1),['paste',5,true,'new text','']);
const closing=create('copy',6,'private');
vm.runInNewContext(body('Component.onDestruction:'),{session});
assert.equal(closing.payload,'');assert.equal(closing.completed,true);assert.equal(closing.running,false);
console.log('text workbench production callbacks: repeated copies and timeout/stale-result isolation passed');
