const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
const {nativeBridge, source} = require('./native-functions.cjs');
const notifications = vm.createContext({Bridge:nativeBridge()});
vm.runInContext(source(fs.readFileSync(process.argv[2], 'utf8')), notifications);
const qml = fs.readFileSync(process.argv[3], 'utf8');
const list = qml.slice(qml.indexOf('  component NotificationList:'), qml.indexOf('  // Notification popups'));
function method(name) {
  const start = list.indexOf('function ' + name + '('); assert(start >= 0, name);
  let end = list.indexOf('{', start) + 1, depth = 1;
  while (depth) { if (list[end] === '{') depth++; if (list[end] === '}') depth--; end++; }
  return list.slice(start, end);
}
const Qt = {ControlModifier:1, AltModifier:2, MetaModifier:4, ShiftModifier:8};
for (const [i, key] of ['J','Down','K','Up','Home','G','End','L','Right','H','Left','Return','Enter','D','Delete','P','C'].entries()) Qt['Key_' + key] = i + 100;
Qt.Key_1 = 200; Qt.Key_9 = 208;
const calls = [];
const entries = [{id:1,app_name:'Chat',actions:{default:'Open',reply:'Reply'},action_order:['default','reply'],body:'Your verification code is 123456'}, {id:2,app_name:'Chat'}, {id:3,app_name:'Mail'}];
const c = vm.createContext({Qt, Notifications:notifications, popup:false, visible:true, history:false, cursorId:'', cursorPosition:-1, expandedGroups:{}, entries,
  revealCursor(){}, root:{notificationActionable:e=>!!e.actions?.default, activateNotification:id=>calls.push(['open',id]), dismissNotification:id=>calls.push(['dismiss',id]),runControl:(...args)=>calls.push(args)}, notificationStore:{controller:{group:(...a)=>calls.push(['group',...a]),pin:id=>calls.push(['pin',id])}}});
Object.defineProperty(c,'model',{get:()=>notifications.stackedRows(c.entries,c.expandedGroups)});
Object.defineProperty(c,'cursor',{get:()=>notifications.cursor(c.model,c.cursorId,c.cursorPosition,0)});
for (const name of ['resetCursor','reconcileCursor','toggleGroup','handleKey']) vm.runInContext(method(name),c);
const press=(key,modifiers=0,isAutoRepeat=false)=>c.handleKey({key,modifiers,isAutoRepeat});
assert.equal(c.cursor.id,''); assert(press(Qt.Key_J)); assert.equal(c.cursor.id,'1');
assert(press(Qt.Key_Return)); assert(c.expandedGroups['app:chat']);
assert(press(Qt.Key_J)); assert.equal(c.cursor.id,'2');
assert(press(Qt.Key_H)); c.reconcileCursor(0); assert.equal(c.cursor.id,'1');
assert(press(Qt.Key_Return)); assert(press(Qt.Key_Return)); assert.deepEqual(calls.pop(),['open',1]);
assert(press(Qt.Key_1)); assert.deepEqual(calls.pop(),['notification-action','1','reply']);
assert(press(Qt.Key_C)); assert.deepEqual(calls.pop(),['copy-code','123456','1']);
assert(press(Qt.Key_P)); assert.deepEqual(calls.pop(),['pin',1]);
assert(press(Qt.Key_D)); assert.deepEqual(calls.pop(),['group','app:chat',false]);
const before=calls.length; assert(press(Qt.Key_D,0,true)); assert.equal(calls.length,before);
assert(press(Qt.Key_G,Qt.ShiftModifier)); assert.equal(c.cursor.id,'3');
assert(press(Qt.Key_Home)); assert.equal(c.cursor.id,'1');
assert(press(Qt.Key_J)); assert.equal(c.cursor.id,'2');
c.entries=c.entries.filter(e=>e.id!==2); c.reconcileCursor(0); assert.equal(c.cursor.id,'3');
c.history=true; assert(press(Qt.Key_D)); assert(press(Qt.Key_P)); assert.equal(calls.length,before);
assert.equal(press(Qt.Key_J,Qt.ControlModifier),false);
c.popup=true; assert.equal(press(Qt.Key_J),false);
c.resetCursor(); assert.equal(c.cursor.id,'');
assert.match(qml,/FocusRing \{ shown: !notificationCard.popup && notificationCard.keyboardSelected/);
assert.match(qml,/keyboardSelected && index < 9/);
console.log('Notification cursor identity, folding, removal and production keyboard actions passed');
