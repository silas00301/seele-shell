import QtQuick
import Quickshell
import Quickshell.Io
import "../shared/Native.js" as Bridge

// The session itself belongs to `seele-caffeinate`, so this store neither owns
// it nor ends it: the bar and the launcher read one service and either Stop
// releases the same inhibitor. Closing this panel is not an end of anything.
Scope {
  id: store
  readonly property var idle: ({ version: 1, active: false, mode: "", elapsed: 0, remaining: 0, task: null })
  property var session: store.idle
  property string error: ""
  property string actionError: ""
  property string payload: ""
  readonly property bool busy: action.running
  readonly property var projection: Bridge.call("caffeinate.project", [session])
  readonly property bool active: projection.active
  readonly property string barText: projection.barText
  readonly property string headline: projection.headline
  readonly property string detail: projection.detail
  readonly property string hoverText: projection.hoverText
  readonly property var task: projection.task

  function accept(value) {
    if (!value || value.version !== 1) return
    session = value
    error = value.error || ""
  }
  function failure(code) {
    return Bridge.call("caffeinate.failure", [code])
  }
  // One request in flight. A repeated Stop is the same Stop, and the service
  // answers a second one with `no-session` rather than releasing anything twice.
  function send(request) {
    if (action.running) return
    actionError = ""
    payload = JSON.stringify(request) + "\n"
    action.running = true
  }
  function stop() {
    send({ op: "stop" })
  }

  Process {
    id: watch
    command: ["seele-caffeinate", "watch"]
    running: true
    stdout: SplitParser { onRead: data => { try { store.accept(JSON.parse(data)) } catch (_) {} } }
    // A dead watcher knows nothing about the session, so the last snapshot
    // stands until the reconnected service says otherwise.
    onExited: { store.error = "service-unavailable"; reconnect.restart() }
  }
  Timer { id: reconnect; interval: 2000; onTriggered: watch.running = true }
  Process {
    id: action
    command: ["seele-caffeinate", "request"]
    stdinEnabled: true
    onStarted: { write(store.payload); stdinEnabled = false }
    stdout: StdioCollector {
      onStreamFinished: {
        try {
          var value = JSON.parse(text)
          if (value.ok) store.accept(value)
          else store.actionError = store.failure(value.error || "")
        } catch (_) { store.actionError = store.failure("") }
      }
    }
    onExited: stdinEnabled = true
  }
}
