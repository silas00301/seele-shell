const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const notifications = {};
vm.createContext(notifications);
vm.runInContext(fs.readFileSync(process.argv[2], 'utf8'), notifications);
const code = (body, summary = '') => notifications.verificationCode({body, summary});
assert.equal(code('Your verification code is 012345'), '012345');
assert.equal(code('Use <b>123456</b> to sign in'), '123456');
assert.equal(code('Security code: 123-456'), '123456');
assert.equal(code('Your login code is A1B2C3'), 'A1B2C3');
assert.equal(code('Bestätigungscode: 987654'), '987654');
assert.equal(code('OTP: 123456. Repeat: 123456'), '123456');
assert.equal(code('123456', 'Verification code'), '123456');
assert.equal(code('Order 123456 has shipped'), '');
assert.equal(code('Verification code 123456 or 654321'), '');
assert.equal(code('Login at https://example.invalid/123456'), '');
assert.equal(code('Your verification code is 123456789'), '');
assert.equal(code('Login SUCCESS'), '');
assert.equal(notifications.actions({actions:{default:'Open',reply:'Reply'}}).length, 1);
assert.equal(notifications.actions({actions:{default:'Open',reply:'Reply'}})[0].key, 'reply');
assert.equal(notifications.actions({}).length, 0);
assert.equal(notifications.actions({actions:['reply','Reply']}).length, 0);
console.log('notification actions and verification codes passed');

const rows = entries => JSON.parse(JSON.stringify(notifications.stackedRows(entries, {})));
const a = {id:1,app_name:'Chat',desktop_entry:'org.chat.desktop'};
const b = {id:2,app_name:'Chat',desktop_entry:'org.chat'};
const c = {id:3,app_name:'Mail'};
assert.deepEqual(rows([a,c,b]).map(r => [r.entry.id,r.count,r.depth]), [[1,2,1],[3,1,0]]);
assert.equal(notifications.stackedRows([a,c,b], {'desktop:org.chat':true}).length,3);
assert.equal(rows([{id:4},{id:5}]).length,2, 'unidentified senders must not merge');
assert.equal(rows([{id:4,app_name:'__proto__'}, {id:5,app_name:'__proto__'}])[0].count,2);
assert.equal(notifications.localImage('https://example.invalid/tracker.png'),'');
assert.equal(notifications.localImage('image://qsimage/test'),'image://qsimage/test');
assert.equal(notifications.bodyMarkup('<b>Hi</b><img src="https://bad.invalid/x"><a href="https://example.invalid/">Link</a>'), '<b>Hi</b><a href="https://example.invalid/">Link</a>');

function harness() {
  let view, dnd, arrivals = 0;
  const store = notifications.createStore((v,d) => {view=v;dnd=d}, () => arrivals++, 1000);
  const make = (id, attrs = {}) => {
    const n = Object.assign({id, appName:'Chat',appIcon:'chat',desktopEntry:'org.chat',summary:'Hello',body:'World',hints:{},actions:[],urgency:1,expireTimeout:-1}, attrs);
    n.dismissed = 0; n.expired = 0; n.invoked = [];
    n.dismiss = () => {n.dismissed++;store.closed(id,2)};
    n.expire = () => {n.expired++;store.closed(id,1)};
    n.actions = Object.keys(attrs.offered || {}).map(identifier => ({identifier,text:attrs.offered[identifier],invoke:() => {n.invoked.push(identifier);if (!n.resident) n.dismiss()}}));
    return n;
  };
  return {store,make,get view(){return view},get dnd(){return dnd},get arrivals(){return arrivals}};
}
{
  const h=harness(), n=h.make(1);
  h.store.receive(n,1000);h.store.advance(1029.9);
  assert.equal(h.view.popups.length,1);
  h.store.advance(1030);
  assert.equal(h.view.popups.length,0);
  assert.equal(h.view.items.length,1,'toast expiry preserves the inbox and actions');
  assert.equal(n.dismissed+n.expired,0);
}
{
  const h=harness(), n=h.make(1);
  h.store.receive(n,1000);h.store.pause(true,1010);h.store.advance(1100);
  assert.equal(h.view.popups.length,1);
  h.store.pause(false,1100);h.store.advance(1119);
  assert.equal(h.view.popups.length,1);
  h.store.advance(1120);assert.equal(h.view.popups.length,0);
}
{
  const h=harness(), permanent=h.make(1,{expireTimeout:0}), critical=h.make(2,{urgency:2}), resident=h.make(3,{resident:true,offered:{reply:'Reply'}});
  [permanent,critical,resident].forEach(n=>h.store.receive(n,1000));
  h.store.advance(2000);
  assert.equal(h.view.popups.length,2,'resident is independent of never-expiring');
  assert.equal(h.store.invoke(3,'reply'),true);assert.equal(h.view.items.length,3);
  assert.equal(h.store.invoke(3,'unsupported'),false);
  h.store.retire(1);assert.equal(h.view.items.length,3);
  h.store.dismiss(1);assert.equal(h.view.history.length,1);
  assert.equal(h.store.invoke(1,'reply'),false,'history cannot invoke actions');
}
{
  const h=harness(), transient=h.make(1,{transient:true}), suppressed=h.make(2,{transient:true});
  h.store.receive(transient,1000);assert.equal(h.view.items.length,0);
  h.store.advance(1030);assert.equal(transient.expired,1);assert.equal(h.view.history.length,0);
  h.store.setDnd(true);h.store.receive(suppressed,1030);h.store.advance(1060);
  assert.equal(suppressed.expired,1);h.store.setDnd(false);assert.equal(h.view.popups.length,0);
}
{
  const h=harness(), n=h.make(1,{offered:{reply:'Reply'}});
  h.store.receive(n,1000);h.store.pin(1);h.store.advance(2000);
  assert.equal(h.view.popups.length,1);h.store.pin(1);h.store.advance(2030);
  assert.equal(h.view.popups.length,0);
  h.store.invoke(1,'reply');assert.deepEqual(n.invoked,['reply']);assert.equal(h.view.items.length,0);
}
{
  const h=harness(), n=h.make(1,{hints:{value:10}});
  h.store.receive(n,1000);h.store.advance(1010);n.hints.value=50;h.store.receive(n,1010);
  assert.equal(h.view.items[0].progress,50);assert.equal(h.arrivals,1);
  h.store.advance(1030);assert.equal(h.view.popups.length,0);
  n.body='New message';h.store.receive(n,1030);assert.equal(h.view.items.length,1);assert.equal(h.view.popups.length,1);
  h.store.advance(1060);assert.equal(h.view.popups.length,0);
}
{
  const h=harness(), first=h.make(1,{hints:{'x-dunst-stack-tag':'download'}}), replacement=h.make(2,{hints:{'x-dunst-stack-tag':'download'}});
  h.store.receive(first,1000);h.store.receive(replacement,1000);
  assert.equal(h.view.items.length,1);assert.equal(h.view.history.length,0);
  h.store.closed(2,3);assert.equal(h.view.history.length,0,'app-withdrawn notifications do not enter history');
  h.store.receive(h.make(3,{lastGeneration:true}),1000);assert.equal(h.view.popups.length,0,'reload does not re-toast old notifications');
}
{
  const h=harness();
  h.store.receive(h.make(1),1000);h.store.receive(h.make(2),1000);
  h.store.group('desktop:org.chat',true);assert.equal(h.view.popups.length,0);assert.equal(h.view.items.length,2);
  h.store.group('desktop:org.chat',false);assert.equal(h.view.items.length,0);assert.equal(h.view.history.length,2);
  h.store.clear(true);assert.equal(h.view.history.length,0);
}
console.log('notification stacks, lifecycle, urgency, transients, replacements, and actions passed');

{
  const h=harness();h.store.receive(h.make(1),1000);h.store.advance(1020);h.store.receive(h.make(2),1029);
  h.store.advance(1030);assert.equal(h.view.popups.length,1);assert.equal(h.view.popups[0].id,2);
  h.store.advance(1058);assert.equal(h.view.popups.length,1);
  h.store.advance(1059);assert.equal(h.view.popups.length,0,'new arrivals get their own full timeout');
}
{
  const h=harness(), n=h.make(1,{expireTimeout:0});h.store.receive(n,1000);h.store.advance(1020);
  n.expireTimeout=5000;h.store.receive(n,1020);h.store.advance(1024);assert.equal(h.view.popups.length,1);
  h.store.advance(1025);assert.equal(h.view.popups.length,0,'replacement can change a permanent timeout');
}

{
  const old=harness(), n=old.make(1);old.store.receive(n,1000);old.store.pin(1);
  old.store.receive(old.make(2),1000);old.store.dismiss(2);
  const next=harness();next.store.restore(old.store.save());
  next.store.receive(next.make(1,{lastGeneration:true}),1005);
  assert.equal(next.view.items[0].pinned,true);assert.equal(next.view.items[0].time,1000);
  assert.equal(next.view.history.length,1);assert.equal(next.view.popups.length,1);
  next.store.advance(5000);assert.equal(next.view.popups.length,1,'pins survive a shell reload');
}

assert.equal(code('Security update 2026'), '');
assert.equal(code('Login from a new device on 2026-09-07'), '');
assert.equal(code('Your verification code for 2026-09-07: 654321'), '654321');
assert.equal(code('Your code is 654321'), '654321');
{
  const h=harness();h.store.receive(h.make(1),1000);h.store.advance(1030);
  h.store.pin(1);assert.equal(h.view.popups.length,1,'pinning an inbox item makes it visible again');
}

// Run the production QML receive callback with signal objects, including a
// queued replacement that arrives just before the app closes its notification.
if (process.argv[3]) {
  const source = fs.readFileSync(process.argv[3], 'utf8');
  const start = source.indexOf('  function receive(notification) {');
  const end = source.indexOf('  Timer {', start);
  assert(start >= 0 && end > start);
  const h = harness(), queued = new Set();
  const context = vm.createContext({store:{controller:h.store},Qt:{callLater:fn=>queued.add(fn)},Date:{now:()=>1000000}});
  vm.runInContext(source.slice(start,end), context);
  const n = h.make(1);
  for (const name of ['summary','body','actions','hints','image','urgency','resident','transient','expireTimeout','appName','appIcon','desktopEntry']) {
    const callbacks = [];
    n[name+'Changed'] = {connect:fn=>callbacks.push(fn),emit:()=>callbacks.forEach(fn=>fn())};
  }
  const closed=[];
  n.closed={connect:fn=>closed.push(fn)};
  n.dismiss=()=>closed.forEach(fn=>fn(2));
  context.receive(n);assert.equal(n.tracked,true);assert.equal(h.view.items.length,1);
  n.summary='Replacement';n.summaryChanged.emit();n.bodyChanged.emit();
  assert.equal(queued.size,1,'one snapshot for a property-update batch');
  queued.forEach(fn=>fn());queued.clear();assert.equal(h.view.items[0].summary,'Replacement');
  n.summaryChanged.emit();n.dismiss();queued.forEach(fn=>fn());
  assert.equal(h.view.items.length,0,'a stale callback cannot revive a closed QObject');
  console.log('native notification signal wiring passed');
}
