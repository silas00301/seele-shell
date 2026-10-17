.pragma library

// The theme contract at run time: which slots Theme.qml sets when a theme is
// published, and to what. theme-contract.json beside this file is the one
// schema, and `seele-theme` has already held every published document to it,
// so this only decides, slot by slot, between the theme's value and the
// default. Qt owns the assignment itself.

// Every slot a theme may supply, with its type and default. Derived slots are
// computed and fixed ones belong to the contract, except a derived slot marked
// suppliable, which a theme may set in place of its derivation; it is listed
// with `derived` and no default. The wallpaper is one of them: the
// publication names the image the host's mapping chooses for the theme, and
// the outputs that show another one. A weight or an easing carries the names
// it may take, since a derived one has no default to compare with.
function slots(contract) {
  var result = []
  if (!contract || !Array.isArray(contract.groups)) return result
  var enums = contract.enums || {}
  contract.groups.forEach(function(group) {
    if (!Array.isArray(group.slots)) return
    group.slots.forEach(function(slot) {
      if (slot.fixed) return
      var entry = { name: slot.name, type: slot.type }
      if (enums[slot.type]) entry.choices = Object.keys(enums[slot.type])
      if (Object.prototype.hasOwnProperty.call(slot, "derived")) {
        if (!slot.suppliable) return
        entry.derived = true
      } else entry.value = slot.default
      result.push(entry)
    })
  })
  return result
}

var colorPattern = /^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/

// A Bézier curve as Qt's easing.bezierCurve reads it: whole segments of six
// finite numbers, at most eight of them, since longer splines are not safe in
// Qt's easing code, ending at (1, 1), where the transition ends.
function curveValue(value) {
  return Array.isArray(value) && value.length % 6 === 0 && value.length <= 48
    && value.every(function(number) { return typeof number === "number" && Number.isFinite(number) })
    && (value.length === 0 || (value[value.length - 2] === 1 && value[value.length - 1] === 1))
}

function usable(slot, value) {
  if (value === undefined || value === null) return false
  if (slot.type === "curve") return curveValue(value)
  if (slot.choices) return typeof value === "string" && slot.choices.indexOf(value) >= 0
  if (slot.type === "int") return typeof value === "number" && Number.isInteger(value)
  if (slot.type === "real") return typeof value === "number" && Number.isFinite(value)
  if (slot.type === "color") return typeof value === "string" && colorPattern.test(value)
  if (slot.derived) return false
  if (typeof value !== typeof slot.value) return false
  if (slot.type === "outputPaths") return !Array.isArray(value) && Object.keys(value).every(function(output) {
    return typeof value[output] === "string" && value[output].charAt(0) === "/"
  })
  return true
}

// Each slot's value under `theme`: what the theme supplies, or the default
// where it supplies nothing usable, so no slot an earlier theme set outlives
// it. Weights and easings stay names; Theme.qml turns them into Qt's enums.
function values(slots, theme) {
  var result = {}
  var supplied = theme && typeof theme === "object" && !Array.isArray(theme) ? theme : {}
  slots.forEach(function(slot) {
    if (slot.derived) return
    var value = Object.prototype.hasOwnProperty.call(supplied, slot.name) ? supplied[slot.name] : undefined
    result[slot.name] = usable(slot, value) ? value : slot.value
  })
  // An easing and its curve go together: BezierSpline with a curve, a named
  // easing without one. A pair that disagrees takes both defaults, so a
  // transition never runs on a spline it was not given.
  slots.forEach(function(slot) {
    if (slot.type !== "curve") return
    var easing = slot.name.replace(/Curve$/, "")
    if (!Object.prototype.hasOwnProperty.call(result, easing)) return
    if ((result[easing] === "BezierSpline") === (result[slot.name].length > 0)) return
    slots.forEach(function(other) {
      if (other.name === easing || other.name === slot.name) result[other.name] = other.value
    })
  })
  return result
}

// The derived slots `theme` supplies itself, by slot: what Theme.qml's
// `supplied` holds, read before each derivation. A suppliable easing and its
// curve are given together and agree, or neither is applied, so the pair
// falls back to its derivation whole.
function supplied(slots, theme) {
  var result = {}
  var given = theme && typeof theme === "object" && !Array.isArray(theme) ? theme : {}
  slots.forEach(function(slot) {
    if (!slot.derived || !Object.prototype.hasOwnProperty.call(given, slot.name)) return
    if (usable(slot, given[slot.name])) result[slot.name] = given[slot.name]
  })
  slots.forEach(function(slot) {
    if (!slot.derived || slot.type !== "curve") return
    var easing = slot.name.replace(/Curve$/, "")
    var hasEasing = Object.prototype.hasOwnProperty.call(result, easing)
    var hasCurve = Object.prototype.hasOwnProperty.call(result, slot.name)
    if (!hasEasing && !hasCurve) return
    if (hasEasing && hasCurve && (result[easing] === "BezierSpline") === (result[slot.name].length > 0)) return
    delete result[easing]
    delete result[slot.name]
  })
  return result
}
