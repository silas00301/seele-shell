const {nativeBridge, source: nativeSource} = require("./native-functions.cjs");
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const speed = vm.createContext({Bridge: nativeBridge()});
vm.runInContext(nativeSource(fs.readFileSync(process.argv[2], 'utf8')), speed);
const make = overrides => ({canControl:true, rate:1, minRate:0.5, maxRate:3, ...overrides});
const player = make();
assert.deepEqual(Array.from(speed.rates(player)), [0.75,1,1.25,1.5,2]);
assert.deepEqual(Array.from(speed.rates(make({minRate:1.25,maxRate:1.5}))), [1.25,1.5]);
// The group is offered only where there is something to choose between, so a
// player pinned to one rate has to report that rate as its whole set rather
// than as an empty one; Spotify is that player.
assert.deepEqual(Array.from(speed.rates(make({rate:1,minRate:1,maxRate:1}))), [1],
  'a fixed-rate player offers the one rate it runs, which is not a choice');
assert.deepEqual(Array.from(speed.rates(make({rate:1.4,minRate:1.3,maxRate:1.4}))), [],
  'a range containing no preset offers nothing at all');
for (const invalid of [null, {}, make({canControl:false}), make({rate:NaN}), make({rate:0}), make({minRate:0}), make({maxRate:Infinity}), make({minRate:2,maxRate:1})]) {
  assert.deepEqual(Array.from(speed.rates(invalid)), [], 'unsupported players offer no rates');
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

// Execute the real QML handlers. The set is a dropdown on the group's own
// rule, so the rate it writes is the row the user picked and the group is
// drawn only while more than one rate is on offer.
const qml = fs.readFileSync(process.argv[3], 'utf8');
const rule = qml.slice(qml.indexOf('id: playbackSpeedRule'), qml.indexOf('// Audio controls'));
assert.match(rule, /visible: playbackSpeedRule\.rates\.length > 1/,
  'one rate is not a choice, so the group withdraws');
assert.match(rule, /chosen: function\(rate\) \{ return MediaSpeed\.active\(mediaWindow\.player, rate\) \}/,
  'the running rate is the row the list lights');
assert.match(rule, /displayText: mediaWindow\.player \? MediaSpeed\.label\(mediaWindow\.player\) : ""/,
  'the closed box reads the rate the player is actually running');
const activated = rule.match(/onActivated: function\(index\) \{([^]*?)\}\n/);
assert(activated, 'production rate selection handler exists');
const selected = make();
const other = make();
const context = vm.createContext({MediaSpeed:speed, mediaWindow:{player:selected},
  playbackSpeedRule:{rates:speed.rates(selected)}});
vm.runInContext('function activate(index) {' + activated[1] + '}', context);
context.activate(2);
assert.equal(selected.rate, 1.25, 'the picked row is the rate that is written');
assert.equal(other.rate, 1, 'only the selected player is changed');
context.activate(2);
assert.equal(selected.rate, 1.25, 'the lit row is not written again');
context.activate(99);
assert.equal(selected.rate, 1.25, 'a row outside the set is refused');
context.mediaWindow.player = null;
context.activate(0);
assert.equal(selected.rate, 1.25, 'a player disappearing before the click is safe');
console.log('Playback speed bounds, preset selection and dropdown intent checks passed');
