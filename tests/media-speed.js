const {nativeBridge, source: nativeSource} = require("./native-functions.cjs");
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const speed = vm.createContext({Bridge: nativeBridge()});
vm.runInContext(nativeSource(fs.readFileSync(process.argv[2], 'utf8')), speed);
const make = overrides => ({canControl:true, rate:1, minRate:0.5, maxRate:3, ...overrides});
const player = make();
assert.deepEqual(Array.from(speed.rates(player)), [0.75,1,1.25,1.5,2]);
for (const rate of [1.25,1.5,2,0.75,1]) {
  assert.equal(speed.cycle(player), true);
  assert.equal(player.rate, rate, 'cycle supported rates and wrap to the first');
}
assert.deepEqual(Array.from(speed.rates(make({minRate:1.25,maxRate:1.5}))), [1.25,1.5]);
assert.equal(speed.nextRate(make({rate:1.1,minRate:1,maxRate:2})), 1.25, 'external rates advance to the next preset');
assert.equal(speed.nextRate(make({rate:3})), 0.75);
assert.equal(speed.nextRate(make({rate:1,minRate:1,maxRate:1})), null, 'fixed-rate player has nothing to change');
assert.equal(speed.nextRate(make({rate:1.4,minRate:1.3,maxRate:1.4})), null, 'no supported preset disables the action');
for (const invalid of [null, {}, make({canControl:false}), make({rate:NaN}), make({rate:0}), make({minRate:0}), make({maxRate:Infinity}), make({minRate:2,maxRate:1})]) {
  const before = invalid && invalid.rate;
  assert.equal(speed.cycle(invalid), false);
  if (invalid) assert.equal(invalid.rate, before, 'unsupported players receive no writes');
}
assert.equal(speed.label(make({rate:1.25})), '1.25×');
assert.equal(speed.label(null), 'Unavailable');

// Only the presets the player supports are offered, the one it is running is
// lit, and a rate outside the set lights none of them.
assert.equal(speed.active(make({rate:1.25}), 1.25), true);
assert.equal(speed.active(make({rate:1.25}), 1), false);
assert.equal(speed.active(make({rate:1.1}), 1), false, 'an externally set rate lights no preset');
assert.equal(speed.active(null, 1), false);
const picked = make();
assert.equal(speed.select(picked, 1.5), true);
assert.equal(picked.rate, 1.5);
assert.equal(speed.select(picked, 1.5), false, 'the lit preset is not offered again');
assert.equal(picked.rate, 1.5);
for (const invalid of [3, 1.1, 0, NaN, Infinity, '1.25', null, undefined]) {
  assert.equal(speed.select(picked, invalid), false, 'only supported presets are written');
  assert.equal(picked.rate, 1.5);
}
assert.equal(speed.select(make({minRate:1.25,maxRate:1.5}), 2), false, 'a rate the player cannot reach is refused');
for (const incapable of [null, {}, make({canControl:false})]) assert.equal(speed.select(incapable, 1.25), false);

// Execute the real QML handlers, including keyboard repeat/modifier guards.
const qml = fs.readFileSync(process.argv[3], 'utf8');
const action = qml.match(/      function cyclePlaybackSpeed\([^]*?\n      }/);
const key = qml.slice(qml.indexOf('id: playbackSpeedWell')).match(/Keys.onPressed: event => \{([^]*?)\n\s*}/);
assert(action && key, 'production playback action and key handler exist');
assert.match(qml, /onClicked: MediaSpeed\.select\(mediaWindow\.player, speedPreset\.modelData\)/,
  'each preset in the well writes its own rate');
const selected = make();
const other = make();
const context = vm.createContext({MediaSpeed:speed,mediaWindow:{player:selected},Qt:{Key_Return:1,Key_Enter:2,Key_Space:3,ControlModifier:1,AltModifier:2,MetaModifier:4}});
vm.runInContext(action[0], context);
context.mediaWindow.cyclePlaybackSpeed = context.cyclePlaybackSpeed;
vm.runInContext('function press(event) {' + key[1] + '}', context);
const event = {key:2,isAutoRepeat:false,modifiers:0};
context.press(event);
assert.equal(event.accepted, true);
assert.equal(selected.rate, 1.25);
assert.equal(other.rate, 1, 'only the selected player is changed');
context.press({key:2,isAutoRepeat:true,modifiers:0});
context.press({key:3,isAutoRepeat:false,modifiers:1});
assert.equal(selected.rate, 1.25, 'held or modified shortcuts do not cycle again');
const unrelated = {key:9,isAutoRepeat:false,modifiers:0};
context.press(unrelated);
assert.equal(unrelated.accepted, undefined);
context.mediaWindow.player = null;
assert.equal(context.cyclePlaybackSpeed(), false, 'a player disappearing before the click is safe');
console.log('Playback speed bounds, preset selection and keyboard intent checks passed');
