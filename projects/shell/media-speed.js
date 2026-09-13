.import "../shared/Native.js" as Bridge

function snapshot(player) {
  return player ? {canControl: player.canControl, rate: Bridge.number(player.rate),
    minRate: Bridge.number(player.minRate), maxRate: Bridge.number(player.maxRate)} : null
}
function rates(player) { return Bridge.call("media.rates", [snapshot(player)]) }
function nextRate(player) { return Bridge.call("media.nextRate", [snapshot(player)]) }
function cycle(player) {
  var next = nextRate(player)
  if (next === null) return false
  player.rate = next
  return true
}
function label(player) { return Bridge.call("media.rateLabel", [snapshot(player)]) }
