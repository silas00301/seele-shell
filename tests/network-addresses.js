const {nativeBridge, source: nativeSource} = require("./native-functions.cjs");
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const network = vm.createContext({Bridge: nativeBridge()});
vm.runInContext(nativeSource(fs.readFileSync(process.argv[2], 'utf8')), network);
const rows = entries => JSON.parse(JSON.stringify(network.addresses(entries, 'eth0')));
const v4 = {family:'inet',local:'192.0.2.1',prefixlen:24,scope:'global'};
const v6 = {family:'inet6',local:'2001:db8::1',prefixlen:64,scope:'global'};
assert.deepEqual(rows([v4,v6]), [
  {label:'IPv4',value:'192.0.2.1',detail:'192.0.2.1/24'},
  {label:'IPv6',value:'2001:db8::1',detail:'2001:db8::1/64'}
]);
assert.equal(rows([{...v6,local:'fe80::1',scope:'link'}])[0].value,'fe80::1%eth0','link-local copies retain the required scope');
assert.equal(rows([v4,v4,v6]).length,2,'duplicate address snapshots do not create duplicate copy actions');
assert.equal(rows([null,{...v4,tentative:true},{...v6,dadfailed:true},{...v4,valid_life_time:0},{...v4,scope:'host'},{...v4,local:'$(unsafe)'}]).length,0);
assert.equal(rows(null).length,0);
assert.equal(network.addresses([{...v6,local:'fe80::1',scope:'link'}],'bad interface').length,0);
assert.equal(rows(Array.from({length:20},(_,i)=>({...v6,local:'2001:db8::'+i}))).length,8,'snapshot rows stay bounded');
console.log('network address families, scopes, filtering and limits passed');
