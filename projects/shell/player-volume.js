function supported(player) {
  return !!player && !!player.volumeSupported
    && typeof player.volume === "number" && isFinite(player.volume)
}

function writable(player) {
  return supported(player) && !!player.canControl
}

function percent(player) {
  return supported(player) ? Math.round(Math.max(0, player.volume) * 100) : null
}

function adjust(player, delta) {
  if (!writable(player) || typeof delta !== "number" || !isFinite(delta)) return false
  // MPRIS permits players with amplification. Shell controls never request it.
  var value = Math.max(0, Math.min(1, player.volume + delta))
  if (value === player.volume) return false
  player.volume = value
  return true
}
