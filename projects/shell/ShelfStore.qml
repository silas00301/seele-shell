import QtQuick
import Quickshell
import Quickshell.Io

Scope {
  id: store
  property var items: []
  property string error: ""
  property bool ready: false
  property var queue: []
  property bool panelOpen: false
  readonly property var selected: items.filter(function(item) { return item.selected && item.available })
  readonly property var paths: selected.map(function(item) { return item.path })
  readonly property string uris: selected.map(function(item) { return item.uri }).join("\r\n")
  function send(value) {
    if (!ready) {
      if (queue.length < 16) queue = queue.concat([value])
      else error = "The shelf is busy. Try again."
      return
    }
    var frame = JSON.stringify(value) + "\n"
    try {
      if (encodeURIComponent(frame).replace(/%[0-9A-F]{2}/g, "x").length > 256 * 1024) { error = "This shelf request is too large."; return }
    } catch (_) { error = "This shelf request contains invalid text."; return }
    worker.write(frame)
  }
  function accept(value) {
    if (!value || value.version !== 1 || !Array.isArray(value.items)) return
    items = value.items
    error = value.error || ""
    if (!ready) {
      ready = true
      var pending = queue
      queue = []
      pending.forEach(store.send)
    }
  }
  Process {
    id: worker
    command: ["seele-shelf"]
    stdinEnabled: true
    running: true
    stdout: SplitParser { onRead: data => { try { store.accept(JSON.parse(data)) } catch (_) {} } }
    onExited: { store.ready = false; store.items = []; store.error = "Shelf stopped. Open it again to start a fresh shelf." }
  }
  onPanelOpenChanged: {
    if (panelOpen && !worker.running) { worker.stdinEnabled = true; worker.running = true }
    else if (panelOpen) send({op:"status"})
  }
  Timer { interval: 3000; repeat: true; running: store.panelOpen && store.ready; onTriggered: store.send({op:"status"}) }
}
