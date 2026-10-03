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


// Execute the production panel's mode table and preview callback against the
// native bridge. QtTest separately clicks the real delegates and checks keys.
const panelSource = fs.readFileSync(process.argv[3], 'utf8');
const {nativeBridge} = require('./native-functions.cjs');
const modesText = panelSource.match(/readonly property var modes: (\[[\s\S]*?\n  \])/)[1];
const modes = vm.runInNewContext(modesText);
assert.equal(new Set(modes.map(mode => mode.id)).size, modes.length);
const preview = panelSource.match(/function preview\(\) \{([\s\S]*?)\n  \}/)[1];
const panelContext = vm.createContext({Native: nativeBridge(), editor: {text: ''}, mode: '', result: null});
for (const [id, input, output] of [
  ['base64url-encode', '\uffff🦀', '77-_8J-mgA'],
  ['base64url-decode', '77-_8J-mgA', '\uffff🦀'],
  ['base64url-decode', '77-_8J-mgA==', '\uffff🦀'],
  ['base64-encode', '\uffff🦀', '77+/8J+mgA=='],
  ['base64-decode', '77+/8J+mgA==', '\uffff🦀'],
]) {
  const mode = modes.find(mode => mode.id === id);
  assert(mode, 'production selector must expose ' + id);
  panelContext.mode = mode.id;
  panelContext.editor.text = input;
  vm.runInContext(preview, panelContext);
  assert.equal(panelContext.result.valid, true);
  assert.equal(panelContext.result.output, output);
}
for (const [mode, input] of [['base64url-decode','Zg='],['base64url-decode','77+/'],['base64-decode','77-_']]) {
  panelContext.mode = mode;
  panelContext.editor.text = input;
  vm.runInContext(preview, panelContext);
  assert.equal(panelContext.result.valid, false);
  assert.equal(panelContext.result.output, '');
}
console.log('text workbench production modes: URL-safe and standard native dispatch passed');
