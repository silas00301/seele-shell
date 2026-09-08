// Quickshell exposes minRate/maxRate; there is no rateSupported property.
// Missing MPRIS bounds default to 1x, which offers no change for a 1x player.
function rates(player) {
  if (!player || !player.canControl) return []
  var current = Number(player.rate), minimum = Number(player.minRate), maximum = Number(player.maxRate)
  if (!Number.isFinite(current) || current <= 0 || !Number.isFinite(minimum)
      || !Number.isFinite(maximum) || minimum <= 0 || maximum < minimum) return []
  return [0.75, 1, 1.25, 1.5, 2].filter(function(rate) { return rate >= minimum && rate <= maximum })
}

function nextRate(player) {
  var choices = rates(player)
  if (!choices.length) return null
  var current = Number(player.rate)
  for (var i = 0; i < choices.length; i++) {
    if (choices[i] > current + 0.000001) return choices[i]
  }
  return Math.abs(choices[0] - current) > 0.000001 ? choices[0] : null
}

function cycle(player) {
  var next = nextRate(player)
  if (next === null) return false
  player.rate = next
  return true
}

function label(player) {
  var value = player ? Number(player.rate) : NaN
  return Number.isFinite(value) && value > 0 ? String(value) + "×" : "Unavailable"
}
