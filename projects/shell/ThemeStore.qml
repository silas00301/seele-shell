import QtQuick
import Quickshell
import Quickshell.Io
import "../shared/Native.js" as Bridge

// The panel's view of `seele-theme`. The catalog, the publication of a theme's
// files and every reload belong to that helper; what is kept here is what one
// open panel is looking at. Listing reads the catalog and changes nothing, and
// the applied theme is read from the selection the helper itself publishes, so
// a theme chosen from the launcher marks the same tile here without polling.
Scope {
  id: store

  readonly property string stateDirectory: Quickshell.env("SEELE_THEME_STATE")
    || (Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state") + "/seele-theme"
  property var catalog: ({ ok: false, current: "", themes: [] })
  property bool panelOpen: false
  property string query: ""
  // "all", "dark" or "light"; the native layout ignores anything else.
  property string mode: "all"
  // How many tiles a row holds. The panel measures it; the layout wraps by it.
  property int columns: 4
  // The tile the reader last moved to. It is only a wish: what is previewed
  // is decided natively from it, the search and the applied theme.
  property string highlighted: ""
  // What the published selection names, rather than what this panel last asked
  // for, so the tile marked as applied is the one the desktop is actually using.
  property string selection: ""
  property string selectionName: ""
  property string error: ""
  property string actionError: ""
  property string reloadPending: ""
  // The one request in flight. Applying publishes a generation of files and
  // asks applications to re-read them, so a second choice never overlaps it.
  property string applying: ""
  readonly property bool busy: applying !== "" || list.running
  readonly property string current: store.selection !== "" ? store.selection : store.catalog.current || ""
  readonly property var themes: store.catalog.themes || []
  readonly property int total: store.themes.length
  readonly property bool filtered: store.query !== "" || store.mode !== "all"
  readonly property var layout: Bridge.call("themes.layout", [store.themes, store.current, store.query, store.mode, store.columns])
  readonly property string focusedId: Bridge.call("themes.focus", [store.layout, store.highlighted, store.current])
  // The preset the preview draws, or null when nothing is shown.
  readonly property var preview: Bridge.call("themes.find", [store.themes, store.focusedId])
  // What the bar-level surfaces name. The published selection carries its own
  // display name, so a tile says which theme is applied before this panel has
  // ever been opened and the catalog read.
  readonly property string currentName: store.selectionName !== "" ? store.selectionName
    : Bridge.call("themes.name", [store.themes, store.current])

  function failure(code) {
    return Bridge.call("themes.failure", [code || ""])
  }
  function refresh() {
    if (list.running) return
    list.running = true
  }
  function accept(value) {
    var next = Bridge.call("themes.catalog", [value])
    if (!next.ok) { error = failure(next.error); return }
    catalog = next
    error = ""
  }
  function highlight(id) {
    highlighted = id
  }
  // Moves the preview to the neighbouring tile; the native layout decides
  // which one that is, so a wrapped family and a filtered grid move alike.
  function step(direction) {
    highlighted = Bridge.call("themes.step", [layout, focusedId, direction])
  }
  function resetFilters() {
    query = ""
    mode = "all"
  }
  function known(id) {
    for (var i = 0; i < store.themes.length; i++) if (store.themes[i].id === id) return true
    return false
  }
  function apply(id) {
    if (busy || id === "") return false
    if (!known(id)) { actionError = failure("unknown"); return false }
    actionError = ""
    reloadPending = ""
    applying = id
    // The command carries the reviewed ID rather than a binding, so nothing
    // starts a second helper by changing a property.
    set.command = ["seele-theme", "set", id]
    set.running = true
    guard.restart()
    return true
  }
  function applyFocused() {
    return apply(focusedId)
  }
  function applied(value) {
    // The selection file says which theme is current; this reply only reports
    // what the helper could not reload while it was there.
    reloadPending = Bridge.call("themes.pending", [(value && value.pending) || []])
    actionError = ""
  }

  onPanelOpenChanged: {
    if (!panelOpen) {
      // Reopening starts from the applied theme with nothing filtered.
      resetFilters()
      highlighted = ""
      actionError = ""
      reloadPending = ""
      return
    }
    refresh()
  }

  // The published selection, watched rather than polled: the helper renames it
  // into place, and the shell's own palette follows that same file.
  FileView {
    path: store.stateDirectory + "/selection.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: {
      try {
        var picked = Bridge.call("themes.selected", [JSON.parse(text())])
        store.selection = picked.id
        store.selectionName = picked.name
      } catch (_) {
        store.selection = ""
        store.selectionName = ""
      }
    }
    onLoadFailed: {
      store.selection = ""
      store.selectionName = ""
    }
  }

  Process {
    id: list
    command: ["seele-theme", "list"]
    stdout: StdioCollector {
      onStreamFinished: {
        try { store.accept(JSON.parse(text)) } catch (_) { store.error = store.failure("invalid") }
      }
    }
    stderr: StdioCollector {}
    // A helper that could not answer leaves the catalog that was already read,
    // and says so instead of emptying the panel.
    onExited: function (code) {
      if (code !== 0) store.error = store.failure("unavailable")
    }
  }
  Process {
    id: set
    stdout: StdioCollector {
      onStreamFinished: {
        try { store.applied(JSON.parse(text)) } catch (_) { store.actionError = store.failure("invalid") }
      }
    }
    stderr: StdioCollector {}
    onExited: function (code) {
      if (code !== 0 && store.actionError === "") store.actionError = store.failure("failed")
      store.applying = ""
      guard.stop()
      // Whatever happened, what is selected now is read from the helper again
      // rather than assumed from the request.
      store.refresh()
    }
  }
  // Publication and its reloads are bounded work. What this catches is a helper
  // that stopped answering: it is ended, and the panel says what it does not
  // know rather than waiting on it.
  Timer {
    id: guard
    interval: 30000
    onTriggered: {
      store.actionError = store.failure("timeout")
      set.running = false
    }
  }
}
