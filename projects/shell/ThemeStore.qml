import "../shared/ListModels.js" as Models
import QtQuick
import Quickshell
import Quickshell.Io
import "../shared/Native.js" as Bridge

// The panel's view of `seele-theme`. The catalog, the publication of a theme's
// files and every reload belong to that helper; what is kept here is what one
// open panel is looking at. Listing reads the catalog and changes nothing, and
// the applied theme is read from the selection the helper itself publishes, so
// a theme chosen from the launcher marks the same row here without polling.
Scope {
  id: store
  property alias model: rows
  ListModel { id: rows }

  readonly property string stateDirectory: Quickshell.env("SEELE_THEME_STATE")
    || (Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state") + "/seele-theme"
  property var catalog: ({ ok: false, current: "", themes: [] })
  property bool panelOpen: false
  property string query: ""
  // What the published selection names, rather than what this panel last asked
  // for, so the row marked as current is the one the desktop is actually using.
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
  // What the bar-level surfaces name. The published selection carries its own
  // display name, so a tile says which theme is applied before this panel has
  // ever been opened and the catalog read.
  readonly property string currentName: store.selectionName !== "" ? store.selectionName
    : Bridge.call("themes.name", [store.themes, store.current])
  readonly property var themes: store.catalog.themes || []
  readonly property int total: store.themes.length
  readonly property string detail: Bridge.call("themes.detail", [{ themes: store.themes, current: store.current }, rows.count])

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
    publish()
  }
  function publish() {
    Models.reconcile(rows, Bridge.call("themes.rows", [store.themes, current, query]), "entry", function (item) { return item.id })
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
  function applied(value) {
    // The selection file says which theme is current; this reply only reports
    // what the helper could not reload while it was there.
    reloadPending = Bridge.call("themes.pending", [(value && value.pending) || []])
    actionError = ""
  }

  onQueryChanged: publish()
  onCurrentChanged: publish()
  onPanelOpenChanged: {
    if (!panelOpen) {
      query = ""
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
