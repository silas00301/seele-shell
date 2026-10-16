.pragma library
// One output with five workspaces: the second in front and holding the focused
// terminal, the first and fourth holding windows, the rest empty.
var monitor = { name: "DP-1", focused: true }
function toplevel(className, title) {
  return { title: title, lastIpcObject: { class: className, initialClass: className, title: title } }
}
var focused = toplevel("com.mitchellh.ghostty", "nvim ~/Developer/seele")
function workspace(id, active, windows) {
  return { id: id, name: String(id), active: active, focused: active, monitor: monitor, toplevels: { values: windows } }
}
var workspaces = { values: [
  workspace(1, false, [toplevel("zen", "Seele Themes")]),
  workspace(2, true, [focused]),
  workspace(3, false, []),
  workspace(4, false, [toplevel("obsidian", "Notes")]),
  workspace(5, false, [])
] }
monitor.activeWorkspace = workspaces.values[1]
var monitors = { values: [monitor] }
var focusedMonitor = monitor
var focusedWorkspace = workspaces.values[1]
var activeToplevel = focused
function dispatch(request) {}
function refreshWorkspaces() {}
function refreshMonitors() {}
function refreshToplevels() {}
