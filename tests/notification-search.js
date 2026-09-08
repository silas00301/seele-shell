const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const search = vm.createContext({});
vm.runInContext(fs.readFileSync(process.argv[2], 'utf8'), search);
const notifications = vm.createContext({});
vm.runInContext(fs.readFileSync(process.argv[3], 'utf8'), notifications);
const inbox = [
  {id:1, app_name:'Chat', summary:'Release planning', body:'Meet at <b>five</b> &amp; bring tea'},
  {id:2, app_name:'Mail', summary:'[Invoice].*', body:'<a href="https://hidden.invalid/secret">Résumé</a><br>ready &#x1f680;'},
  {id:3, app_name:'Chat', summary:'Old meeting', body:'Canceled'},
  {id:4, app_name:'Calendar', summary:'<literal title>', body:''},
];
const ids = (entries, query) => Array.from(search.filter(entries, query), row => row.id);
assert.equal(search.filter(inbox, '  '), inbox, 'empty query preserves original model');
assert.deepEqual(ids(inbox, 'CHAT FIVE'), [1], 'terms can span app and visible body');
assert.deepEqual(ids(inbox, 'release'), [1]);
assert.deepEqual(ids(inbox, '[Invoice].*'), [2], 'query is literal, never a regexp');
assert.deepEqual(ids(inbox, 'résumé 🚀'), [2]);
assert.deepEqual(ids(inbox, 'secret'), [], 'hidden link target is not searchable');
assert.deepEqual(ids(inbox, '<b>'), [], 'body markup does not appear in search');
assert.deepEqual(ids(inbox, '<literal'), [4], 'plain title text is preserved');
assert.deepEqual(ids(inbox, '&'), [1]);
assert.equal(search.bodyText('a<br/>b &lt;b&gt; &#65; &#x110000; &#xD800;'), 'a\nb <b> A &#x110000; &#xD800;');
assert.deepEqual(ids([{}], 'missing'), []);
const before = JSON.stringify(inbox);
const history = [{id:10, app_name:'Chat', summary:'Dismissed planning', body:'five'}];
assert.deepEqual(ids(history, 'chat five'), [10], 'same filter supports historical records');
const matched = search.filter(inbox, 'old');
assert.equal(notifications.stackedRows(matched, {})[0].items[0].id, 3, 'matching older records are filtered before stacking');
assert.equal(JSON.stringify(inbox), before, 'filter does not mutate the store or dismiss notifications');
const shell = fs.readFileSync(process.argv[4], 'utf8');
assert.equal((shell.match(/query: notificationSearch.text/g) || []).length, 2);
assert.match(shell, /notificationSearch.forceActiveFocus\(\)/);
assert.match(shell, /notificationSearch.selectAll\(\)/);
assert.match(shell, /maximumLength: 256/);
assert.match(shell, /else root.closeOverlays\(\)/);
assert.match(shell, /No notifications match your search/);
assert.match(shell, /visible: !notificationList.history && notificationList.query.trim\(\) === ""/, 'bulk stack dismiss stays hidden while filtering');
console.log('notification search fields, literal terms, markup, history, stacking and UI wiring passed');
