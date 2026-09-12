import "../shared/ListModels.js" as Models
import QtQuick
import Quickshell
import Quickshell.Io
import "../shared/Native.js" as Bridge

Scope {
  id: store
  property var groups: []
  property var targets: []
  property var selection: []
  property var capabilities: ({})
  property string error: ""
  property string actionError: ""
  property string expanded: ""
  property string lastFocus: ""
  property int lastFocusRevision: 0
  property bool panelOpen: false
  property var queue: []
  property string payload: ""
  property alias model: rows
  readonly property bool busy: action.running || queue.length > 0
  readonly property var projection: Bridge.call("transfers.project", [groups.map(function(group) { return {state: group.state, size: group.size, bytes: group.bytes, seen: group.seen} })])
  readonly property bool attention: projection.attention
  readonly property string barText: projection.barText
  signal revealRequested()

  ListModel { id: rows }
  function accept(value) {
    if (!value || value.version !== 1) return
    groups = value.groups || []
    targets = value.targets || []
    selection = value.selection || []
    capabilities = value.capabilities || ({})
    error = value.error || ""
    Models.reconcile(rows, groups, "entry", function(item) { return item.id })
    if (value.focus && (value.focus !== lastFocus || Number(value.focusRevision || 0) !== lastFocusRevision)) {
      lastFocus = value.focus
      lastFocusRevision = Number(value.focusRevision || 0)
      expanded = value.focus
      revealRequested()
    }
    if (panelOpen && !busy) markSeen()
  }
  function markSeen() {
    var ids = []
    for (var i = 0; i < groups.length; i++) if (!groups[i].seen) ids.push(groups[i].id)
    if (ids.length) enqueue({ op: "seen", ids: ids })
  }
  onPanelOpenChanged: if (panelOpen) markSeen()
  function enqueue(value) {
    var next = Bridge.call("transfers.enqueue", [queue, value])
    if (next === null) return
    if (next.error) { actionError = next.error; return }
    queue = next.queue
    runNext()
  }
  function runNext() {
    if (action.running || !queue.length) return
    payload = JSON.stringify(queue[0]) + "\n"
    queue = queue.slice(1)
    actionError = ""
    action.running = true
  }
  function selectUrls(urls) {
    var values = []
    for (var i = 0; i < urls.length; i++) values.push(String(urls[i]))
    var result = Bridge.call("transfers.selectUrls", [values])
    if (result.error) { actionError = result.error; return }
    enqueue({ op: "select", paths: result.paths })
  }
  function failure(code) {
    return Bridge.call("transfers.failure", [code])
  }
  Process {
    id: watch
    command: ["seele-transfers", "watch"]
    running: true
    stdout: SplitParser { onRead: data => { try { store.accept(JSON.parse(data)) } catch (_) {} } }
    onExited: { store.error = "service-unavailable"; reconnect.restart() }
  }
  Timer { id: reconnect; interval: 2000; onTriggered: watch.running = true }
  Process {
    id: action
    command: ["seele-transfers", "request"]
    stdinEnabled: true
    onStarted: { write(store.payload); stdinEnabled = false }
    stdout: StdioCollector {
      onStreamFinished: {
        try { store.actionError = store.failure(JSON.parse(text).error || "") }
        catch (_) { store.actionError = "The action failed. Try again." }
      }
    }
    onExited: { stdinEnabled = true; Qt.callLater(store.runNext) }
  }
}
