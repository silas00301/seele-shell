const {nativeBridge, source: nativeSource} = require("./native-functions.cjs");
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const volume = vm.createContext({Bridge: nativeBridge()});
vm.runInContext(nativeSource(fs.readFileSync(process.argv[2], 'utf8')), volume);
const player = {canControl:true, volumeSupported:true, volume:.5};
assert.equal(volume.percent(player),50);
assert.equal(volume.adjust(player,.05),true);
assert.equal(volume.percent(player),55);
assert.equal(volume.adjust(player,-.05),true);
assert.equal(volume.percent(player),50);
for(const incapable of [null,{}, {...player,canControl:false},{...player,volumeSupported:false},{...player,volume:NaN},{...player,volume:Infinity},{...player,volume:'0.5'}]) {
  assert.equal(volume.adjust(incapable,.05),false,'unsupported/read-only/invalid players receive no writes');
}
assert.equal(volume.adjust(player,NaN),false);
assert.equal(volume.adjust(player,Infinity),false);
assert.equal(volume.adjust(player,'0.1'),false);
player.volume=.99;
volume.adjust(player,.05);
assert.equal(player.volume,1);
assert.equal(volume.adjust(player,.05),false,'upper bound does not cause repeated writes');
player.volume=.01;
volume.adjust(player,-.05);
assert.equal(player.volume,0);
assert.equal(volume.adjust(player,-.05),false,'lower bound does not cause repeated writes');
player.volume=1.4;
assert.equal(volume.percent(player),140,'external amplification remains visible');
volume.adjust(player,-.05);
assert.equal(player.volume,1,'shell never writes amplified volume');
const second = {...player,volume:.3};
volume.adjust(second,.05);
assert.equal(player.volume,1,'adjusting another selected player leaves the prior player untouched');
assert.equal(volume.percent(second),35);
second.volume=.8;
assert.equal(volume.percent(second),80,'external changes remain live');
second.canControl=false;
assert.equal(volume.percent(second),80,'read-only volume can still be displayed');
assert.equal(volume.adjust(second,-.05),false,'capability changes are checked at activation');

// A level is dragged to a share of its track rather than nudged by a delta, so
// the position is clamped, refused when it is not a number, and dropped when
// it lands on the volume the player is already at.
const dragged = {canControl:true, volumeSupported:true, volume:.5};
assert.equal(volume.ratio(dragged), .5);
assert.equal(volume.seek(dragged, .25), true);
assert.equal(dragged.volume, .25);
assert.equal(volume.seek(dragged, .25), false, 'the level it is already at is not written again');
assert.equal(volume.seek(dragged, 2), true);
assert.equal(dragged.volume, 1, 'a drag past the end of the track stops at full volume');
assert.equal(volume.seek(dragged, -1), true);
assert.equal(dragged.volume, 0);
for (const invalid of [NaN, Infinity, '0.5', null, undefined]) {
  assert.equal(volume.seek(dragged, invalid), false);
  assert.equal(dragged.volume, 0, 'an invalid position is never written');
}
for (const incapable of [null, {}, {...dragged, canControl:false}, {...dragged, volumeSupported:false}]) {
  assert.equal(volume.seek(incapable, .5), false, 'read-only and unsupported players receive no writes');
}
dragged.volume = 1.4;
assert.equal(volume.ratio(dragged), 1, 'the track ends at full volume even when the player is amplified');
assert.equal(volume.ratio({...dragged, volumeSupported:false}), null);
assert.equal(volume.ratio(null), null);

console.log('per-player volume capabilities, bounds, drag positions, live state and isolation passed');
