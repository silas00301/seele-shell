.pragma library

// The theme contract at run time: which slots Theme.qml sets when a theme is
// published, and to what. theme-contract.json beside this file is the one
// schema, and `seele-theme` has already held every published document to it,
// so this only decides, slot by slot, between the theme's value and the
// default. Qt owns the assignment itself.

// Every slot a theme may supply, with its type and default. Derived slots are
// computed and fixed ones belong to the contract. The wallpaper is one of
// them: the publication names the image the host's mapping chooses for the
// theme, and the outputs that show another one.
function slots(contract) {
  var result = []
  if (!contract || !Array.isArray(contract.groups)) return result
  contract.groups.forEach(function(group) {
    if (!Array.isArray(group.slots)) return
    group.slots.forEach(function(slot) {
      if (Object.prototype.hasOwnProperty.call(slot, "derived") || slot.fixed) return
      result.push({ name: slot.name, type: slot.type, value: slot.default })
    })
  })
  return result
}

function usable(slot, value) {
  if (value === undefined || value === null || typeof value !== typeof slot.value) return false
  if (typeof value === "number") return Number.isFinite(value)
  if (slot.type === "color") return /^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(value)
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
    var value = Object.prototype.hasOwnProperty.call(supplied, slot.name) ? supplied[slot.name] : undefined
    result[slot.name] = usable(slot, value) ? value : slot.value
  })
  return result
}
