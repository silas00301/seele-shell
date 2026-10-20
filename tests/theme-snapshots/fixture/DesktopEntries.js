.pragma library
// Names come from desktop entries; icons are left out (see Quickshell.js).
var known = { "com.mitchellh.ghostty": { name: "Ghostty", icon: "" } }
var applications = { values: [known["com.mitchellh.ghostty"]] }
function heuristicLookup(name) { return known[String(name)] || null }
function byId(id) { return known[String(id)] || null }
