import QtQuick
import Quickshell
import Quickshell.Io
import "../shared/ListModels.js" as ListModels

Item {
  id: store
  property var snapshot: ({ items: [], count: 0, complete: false, refreshing: true, viewer: "", host: "github.com", selected: "", detail: null, error: "", notice: "" })
  property string connectionError: ""
  readonly property alias model: entries
  readonly property alias detailModel: detailEntries
  signal focusRequested()
  signal updated()

  function send(op, id) {
    if (!worker.running) {
      worker.running = true
      return false
    }
    worker.write(JSON.stringify({ op: op, id: id || "" }) + "\n")
    return true
  }
  function receive(line) {
    try {
      var value = JSON.parse(line)
      if (value.event === "focus") { focusRequested(); return }
      if (value.event !== "snapshot" || !Array.isArray(value.items)) return
      connectionError = ""
      if (snapshot.selected !== value.selected) detailEntries.clear()
      if (JSON.stringify(snapshot.detail) === JSON.stringify(value.detail)) value.detail = snapshot.detail
      snapshot = value
      ListModels.reconcile(entries, value.items, "entry", function(row) { return row.id }, "threadId")
      ListModels.reconcile(detailEntries, value.detail ? value.detail.blocks : [], "block", function(row) { return row.key }, "blockKey")
      updated()
    } catch (_) { connectionError = "The GitHub inbox returned an unreadable update. Refresh to reconnect." }
  }
  ListModel { id: detailEntries; dynamicRoles: true }
  ListModel { id: entries; dynamicRoles: true }
  Process {
    id: worker
    command: ["seele-github-inbox"]
    stdinEnabled: true
    running: true
    stdout: SplitParser { onRead: line => store.receive(line) }
    onExited: {
      store.connectionError = "The GitHub inbox stopped. Reconnecting…"
      reconnect.restart()
    }
  }
  Timer { id: reconnect; interval: 5000; onTriggered: worker.running = true }
}
