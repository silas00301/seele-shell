import QtQuick
import Quickshell
import Quickshell.Io
import "../shared/Native.js" as Bridge

Scope {
  id: store
  property bool enabled: false
  property var snapshot: ({version: 1, state: "disconnected", pending: []})
  property string error: ""
  property string payload: ""
  readonly property bool busy: action.running
  readonly property var projection: Bridge.call("hermes.project", [snapshot])
  signal heartbeat(var projection)

  function send(request) {
    if (busy) return
    error = ""
    payload = JSON.stringify(request) + "\n"
    action.running = true
  }
  function openDesktop() { if (!desktop.running) desktop.running = true }
  function accept(value) {
    if (!value || value.version !== 1) return
    snapshot = value
    heartbeat(projection)
  }
  FileView {
    path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/seele-shell/hermes.json"
    printErrors: false
    onLoaded: { try { store.enabled = JSON.parse(text()).enable === true } catch (_) { store.enabled = false } }
  }
  Process {
    id: watcher
    command: ["seele-hermes", "watch"]
    running: store.enabled
    stdout: SplitParser { onRead: line => { try { store.accept(JSON.parse(line)) } catch (_) {} } }
    onExited: {
      store.snapshot = ({version: 1, state: "disconnected", pending: []})
      store.heartbeat(store.projection)
      if (store.enabled) reconnect.restart()
    }
  }
  Timer { id: reconnect; interval: 2000; onTriggered: if (store.enabled) watcher.running = true }
  Process {
    id: action
    command: ["seele-hermes", "request"]
    stdinEnabled: true
    onStarted: { write(store.payload); stdinEnabled = false }
    stdout: StdioCollector {
      onStreamFinished: {
        try { var value = JSON.parse(text); if (value.ok === false) store.error = value.error || "Hermes could not complete the action." }
        catch (_) { store.error = "Hermes could not complete the action." }
      }
    }
    onExited: stdinEnabled = true
  }
  Process { id: desktop; command: ["hermes-desktop"] }
}
