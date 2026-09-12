import QtQuick
import Quickshell
import Quickshell.Io

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
  readonly property bool attention: groups.some(g => ["sending", "receiving", "retrying", "failed"].indexOf(g.state) >= 0 || !g.seen)
  readonly property string barText: {
    var active = groups.filter(g => ["sending", "receiving", "retrying"].indexOf(g.state) >= 0)
    if (active.length) {
      var size = active.reduce((n, g) => n + g.size, 0)
      var bytes = active.reduce((n, g) => n + g.bytes, 0)
      return "󰇚 " + (size > 0 ? Math.floor(bytes / size * 100) + "%" : "…")
    }
    return groups.some(g => g.state === "failed") ? "󰇚 !" : "󰇚 " + groups.filter(g => !g.seen).length
  }
  signal revealRequested()

  ListModel { id: rows }
  function accept(value) {
    if (!value || value.version !== 1) return
    groups = value.groups || []
    targets = value.targets || []
    selection = value.selection || []
    capabilities = value.capabilities || ({})
    error = value.error || ""
    for (var i = 0; i < groups.length; i++) {
      var found = -1
      for (var j = i; j < rows.count; j++) if (rows.get(j).entry.id === groups[i].id) { found = j; break }
      if (found < 0) rows.insert(i, { entry: groups[i] })
      else {
        if (found !== i) rows.move(found, i, 1)
        if (JSON.stringify(rows.get(i).entry) !== JSON.stringify(groups[i])) rows.setProperty(i, "entry", groups[i])
      }
    }
    while (rows.count > groups.length) rows.remove(rows.count - 1)
    if (value.focus && (value.focus !== lastFocus || Number(value.focusRevision || 0) !== lastFocusRevision)) {
      lastFocus = value.focus
      lastFocusRevision = Number(value.focusRevision || 0)
      expanded = value.focus
      revealRequested()
    }
    if (panelOpen && !busy) markSeen()
  }
  function markSeen() {
    for (var i = 0; i < groups.length; i++) if (!groups[i].seen) enqueue({ op: "seen", id: groups[i].id })
  }
  onPanelOpenChanged: if (panelOpen) markSeen()
  function enqueue(value) {
    var next = queue.slice()
    if (next.some(v => JSON.stringify(v) === JSON.stringify(value))) return
    next.push(value)
    queue = next
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
    var paths = []
    for (var i = 0; i < urls.length; i++) {
      var url = String(urls[i])
      if (url.indexOf("file:///") !== 0) { actionError = "Only local files can be sent."; return }
      try { paths.push(decodeURIComponent(url.slice(7))) } catch (_) { actionError = "Invalid file."; return }
    }
    enqueue({ op: "select", paths: paths })
  }
  function failure(code) {
    var messages = {
      "provider-unavailable": "Tailscale is unavailable. Connect it and try again.",
      "service-unavailable": "Transfers service is unavailable.",
      "target-unavailable": "This device is unavailable. Bring it online, then retry.",
      "source-missing": "A source file is missing. Choose the file again.",
      "source-changed": "A source file changed. Choose it again.",
      "not-a-file": "Choose files; folders are not supported.",
      "cancel-at-sender": "Stop this incoming transfer on the sending device.",
      "interrupted": "Transfer interrupted. Retry when the device is available.",
      "choose-files": "Choose files before selecting a device.",
      "cancelled": "Cancelled", "already-active": "This transfer is already active."
    }
    return messages[code] || (code ? "The action failed. Try again." : "")
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
