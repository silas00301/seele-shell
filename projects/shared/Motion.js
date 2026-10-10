.pragma library

// Material 3 Expressive moves on springs. Qt animates on durations, so each
// spring is sampled into a Bézier spline over its own settling time and handed
// to an animation as its easing curve. Theme.qml names the springs; this file
// is the arithmetic, shared with the auth clients' loading indicator.

// Where a damped spring of unit mass is, `seconds` after release, travelling
// from 0 to 1. `stiffness` and `dampingRatio` are Material's own tokens.
function springPosition(stiffness, dampingRatio, seconds) {
  var natural = Math.sqrt(stiffness)
  if (dampingRatio >= 1) return 1 - (1 + natural * seconds) * Math.exp(-natural * seconds)
  var damped = natural * Math.sqrt(1 - dampingRatio * dampingRatio)
  return 1 - Math.exp(-dampingRatio * natural * seconds)
    * (Math.cos(damped * seconds) + dampingRatio * natural / damped * Math.sin(damped * seconds))
}

// The moment the spring settles within 0.2% of its target, which is where
// the motion visibly stops.
function springSettle(stiffness, dampingRatio) {
  var settled = 0
  for (var seconds = 0; seconds < 2; seconds += 0.002)
    if (Math.abs(springPosition(stiffness, dampingRatio, seconds) - 1) > 0.002) settled = seconds
  return settled + 0.002
}

function springDuration(stiffness, dampingRatio) {
  return Math.round(springSettle(stiffness, dampingRatio) * 1000)
}

// Eight cubic segments, each matching the spring's slope at both ends, so the
// spline follows the spring to within its own settling band and keeps the
// overshoot's peak. The segments are shorter where the spring moves fastest.
// Qt's spline easing overruns its own storage past ten segments, so the count
// is a ceiling rather than a resolution to raise.
var knots = [0, 0.05, 0.11, 0.18, 0.27, 0.38, 0.52, 0.72, 1]

function springCurve(stiffness, dampingRatio) {
  var settle = springSettle(stiffness, dampingRatio)
  function at(x) { return springPosition(stiffness, dampingRatio, x * settle) }
  function slope(x) { return (at(x + 0.00001) - at(Math.max(0, x - 0.00001))) / (x > 0 ? 0.00002 : 0.00001) }
  var points = []
  for (var index = 1; index < knots.length; ++index) {
    var start = knots[index - 1]
    var end = knots[index]
    var third = (end - start) / 3
    var startY = at(start)
    var endY = end === 1 ? 1 : at(end)
    points.push(start + third, startY + third * slope(start), end - third, endY - third * slope(end), end, endY)
  }
  return points
}

// Material's Expressive springs: the fast spatial spring every control moves
// and the loading indicator morphs on, the default spatial spring a surface
// unfolds on, and the fast effects spring colour and opacity change on.
var fastSpatialStiffness = 800
var fastSpatialDamping = 0.6
var defaultSpatialStiffness = 380
var defaultSpatialDamping = 0.8
var fastEffectsStiffness = 3800
var fastEffectsDamping = 1
