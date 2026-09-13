.import "../shared/Native.js" as Bridge

function snapshot(player) {
  return player ? {canControl: player.canControl, volumeSupported: player.volumeSupported,
    volume: typeof player.volume === "number" ? Bridge.number(player.volume) : player.volume} : null
}
function supported(player) { return Bridge.call("media.volumeSupported", [snapshot(player)]) }
function writable(player) { return Bridge.call("media.volumeWritable", [snapshot(player)]) }
function percent(player) { return Bridge.call("media.volumePercent", [snapshot(player)]) }
function adjust(player, delta) {
  var next = Bridge.call("media.nextVolume", [snapshot(player), typeof delta === "number" ? Bridge.number(delta) : delta])
  if (next === null) return false
  player.volume = next
  return true
}
