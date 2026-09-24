const fs = require('node:fs');
const vm = require('node:vm');
const {nativeBridge} = require('./native-functions.cjs');
const assert = require('node:assert/strict');
const source = fs.readFileSync(process.argv[2], 'utf8');
const panel = fs.readFileSync(process.argv[3], 'utf8');
const shell = fs.readFileSync(process.argv[4], 'utf8');
const methods = [...source.matchAll(/^  function \w+\([^\n]*\) \{\n[\s\S]*?^  \}/gm)].map(m=>m[0]).join('\n');
const values = [];
let reveals = 0;
const rows = {get count(){return values.length}, get(i){return values[i]}, insert(i,v){values.splice(i,0,v)}, move(i,j){values.splice(j,0,values.splice(i,1)[0])}, setProperty(i,k,v){values[i][k]=v},remove(i,count=1){values.splice(i,count)}};
const state = vm.createContext({Models:require("./list-models.cjs")(),Bridge:nativeBridge(),rows, groups:[],targets:[],selection:[],capabilities:{},error:'',actionError:'',lastFocus:'',lastFocusRevision:0,expanded:'',query:'',direction:'all',status:'all',history:{},panelOpen:false,queue:[],payload:'',action:{running:false},revealRequested(){reveals++}, busy:false});
vm.runInContext(methods,state);
state.selectUrls(['https://example.test/file']);
assert.equal(state.queue.length,0);
assert.ok(state.actionError.includes('local'));
state.selectUrls(['file:///tmp/a%20file%0A.txt']);
assert.deepEqual(JSON.parse(state.payload),{op:'select',paths:['/tmp/a file\n.txt']});
state.enqueue({op:'send',target:'phone'});
state.enqueue({op:'send',target:'phone'});
assert.equal(state.queue.length,1,'duplicate queued activations collapse');
const group={id:'one',state:'sending',seen:true,files:[]};
state.accept({version:1,groups:[group],targets:[],selection:[]});
const identity=rows.get(0).entry;
state.accept({version:1,groups:[{...group}],targets:[],selection:[]});
assert.equal(rows.get(0).entry,identity,'unchanged snapshots preserve delegate data');
state.accept({version:1,groups:[{...group,id:'two'},group],focus:'one'});
assert.equal(rows.get(1).entry,identity,'reordering keeps existing data');
assert.equal(state.expanded,'one');
assert.equal(reveals,1);
state.accept({version:1,groups:[group],focus:'one'});
assert.equal(reveals,1,'notification focus is not repeated by polling');
state.expanded = '';
state.accept({version:1,groups:[group],focus:'one',focusRevision:1});
assert.equal(reveals,2,'another explicit notification activation focuses the same group again');
assert.equal(state.expanded,'one');
assert.match(shell,/function openTransfers\(\): void \{ if \(root.controlPanel !== "transfers"\)/, 'external open is idempotent');
state.accept({version:1,groups:[],error:'service-unavailable'});
assert.equal(rows.count,0,'a stopped service clears stale controls');
assert.match(panel,/fileMode: FileDialog.OpenFiles/);
assert.match(panel,/DropArea/);
assert.match(panel,/"open", "reveal", "move", "trash"/);
assert.match(shell,/visible: transfersStore.attention/);
assert.match(shell,/TransfersPanel \{ theme: root; store: transfersStore/);
assert.match(shell,/label: "Transfers"/);
assert.doesNotMatch(panel,/tailscale|file-put|file-targets/,'UI stays provider-neutral');
console.log('Transfers selection, duplicate actions, stable rows, notification focus and production wiring passed');

const openBody = shell.match(/function openTransfers\(\): void \{([^\n]+)\}/)[1];
let toggles = 0;
const desktop = {controlPanel:'',toggleControl(name){toggles++;this.controlPanel=this.controlPanel===name?'':name}};
const ipc = vm.createContext({root:desktop});
vm.runInContext('function open(){'+openBody+'}',ipc);
ipc.open(); // QML notification focus can open first.
ipc.open(); // CLI notification action can arrive second.
assert.equal(desktop.controlPanel,'transfers');
assert.equal(toggles,1,'notification/picker external open cannot close an already-open panel');

state.action.running=true;state.queue=[];state.groups=[{id:"one",seen:false},{id:"two",seen:false},{id:"seen",seen:true}];
state.markSeen();assert.deepEqual(JSON.parse(JSON.stringify(state.queue)),[{op:"seen",ids:["one","two"]}],"opening a panel marks all unseen groups in one durable request");
for (const url of ["file:///tmp/%", "file:///tmp/%GG", "file:///tmp/%FF"]) {state.queue=[];state.selectUrls([url]);assert.equal(state.queue.length,0);assert.equal(state.actionError,"Invalid file.");}
state.queue=Array.from({length:128},(_,id)=>({op:"seen",id:String(id)}));state.enqueue({op:"new"});assert.equal(state.queue.length,128);assert.match(state.actionError,/Too many/);
const projection=source.match(/readonly property var projection: ([^\n]+)/)[1];
state.groups=[{state:"sending",size:200,bytes:51,seen:true},{state:"receiving",size:100,bytes:51,seen:false}];
assert.equal(vm.runInContext(projection,state).barText,"󰇚 34%");
console.log("Transfers bounded native URL/action policy and batched seen checks passed");

// Execute native matching through production store methods, retaining original entries.
const historyGroups = [
  {id:'done',direction:'incoming',device:'Phone',state:'completed',files:[{name:'Summer Photo.JPG'}]},
  {id:'failure',direction:'outgoing',device:'Tablet',state:'failed',files:[{name:'Other.txt'},{name:'Budget.pdf'}]},
  {id:'live',direction:'outgoing',device:'Laptop',state:'sending',files:[{name:'Unrelated.zip'}]},
  {id:'cancel',direction:'incoming',device:'Tablet',state:'cancelled',files:[{name:'note.md'}]},
];
state.panelOpen=false;
state.accept({version:1,groups:historyGroups});
assert.deepEqual(values.map(row=>row.entry.id),['live','done','failure','cancel']);
state.query='  BUDGET  ';state.refreshRows();
assert.deepEqual(values.map(row=>row.entry.id),['live','failure']);
assert.equal(rows.get(1).entry.files[1].name,'Budget.pdf','matched multi-file group retains original file order');
assert.equal(state.history.matched,1);
state.direction='incoming';state.refreshRows();
assert.deepEqual(values.map(row=>row.entry.id),['live'],'active work survives every filter');
state.query='tablet';state.status='cancelled';state.refreshRows();
assert.deepEqual(values.map(row=>row.entry.id),['live','cancel']);
state.accept({version:1,groups:historyGroups,focus:'failure',focusRevision:2});
assert.equal(state.query,'');assert.equal(state.direction,'all');assert.equal(state.status,'all');
assert.equal(state.expanded,'failure');
assert.equal(rows.count,4,'explicit notification focus reveals a previously filtered history group');
state.query='no match';state.refreshRows();
state.accept({version:1,groups:historyGroups.map(group=>group.id==='live'?{...group,state:'completed'}:group)});
assert.equal(rows.count,0,'completed work follows current history filters');
assert.equal(state.groups.length,4,'filtering never edits canonical history');
state.resetFilters();assert.equal(rows.count,4);
assert.match(panel,/Shared.SearchField/);assert.match(panel,/No matching history/);
assert.match(panel,/Qt.Key_F.*Qt.ControlModifier/);
assert.match(source,/onQueryChanged: refreshRows\(\)/);
assert.match(source,/onDirectionChanged: refreshRows\(\)/);
assert.match(source,/onStatusChanged: refreshRows\(\)/);
console.log('Transfers native history search, combined filters, active jobs and notification reveal passed');

assert.match(shell,/onVisibleChanged: if \(visible\) Qt.callLater\(function\(\) \{ transfersPanel.forceActiveFocus\(\) \}\)/, 'opening Transfers focuses the panel that handles Ctrl+F');
assert.match(panel,/function onRevealRequested\(\) \{ Qt.callLater\(panel.revealGroup\) \}/, 'repeated notification focus scrolls after clearing filters');
