pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io

// Starting the process opens a new observation session; closing kills it and
// discards every peak. The worker has no off-panel sampling mode, so nothing
// reads a sensor while the panel is closed.
Scope {
  id: store
  property bool panelOpen: false
  property var snapshot: ({ rows: [], elapsed: 0, cadenceSeconds: 2, summary: "", attention: 0, limited: false, skipped: 0, deviceLimit: 0 })
  property string error: ""
  property bool received: false
  property int generation: 0
  property var worker: null
  readonly property var rows: snapshot.rows || []
  function accept(value, session) {
    if (session !== generation || !panelOpen || !value || value.version !== 1 || !Array.isArray(value.rows)) return
    snapshot = value
    error = value.error || ""
    received = true
  }
  function reset() {
    if (worker && worker.running) worker.write('{"op":"reset"}\n')
  }
  function clear() {
    generation++
    if (worker) {
      worker.running = false
      worker.destroy()
      worker = null
    }
    snapshot = ({ rows: [], elapsed: 0, cadenceSeconds: 2, summary: "", attention: 0, limited: false, skipped: 0, deviceLimit: 0 })
    received = false
    error = ""
  }
  function start() {
    if (!panelOpen) return
    worker = workerComponent.createObject(store, { session: generation })
    worker.running = true
  }
  function retry() {
    if (!panelOpen) return
    clear()
    start()
  }
  function failed(session) {
    if (!panelOpen || session !== generation) return
    snapshot = ({ rows: [], elapsed: 0, cadenceSeconds: 2, summary: "", attention: 0, limited: false, skipped: 0, deviceLimit: 0 })
    error = "Sensors stopped. Retry starts a new session."
  }
  onPanelOpenChanged: {
    clear()
    if (panelOpen) start()
  }
  // Never reuse a Process across panels. A generation also rejects buffered
  // callbacks delivered while a previously destroyed process is draining.
  Component {
    id: workerComponent
    Process {
      id: process
      required property int session
      command: ["seele-sensors"]
      stdinEnabled: true
      stdout: SplitParser {
        onRead: data => {
          try { store.accept(JSON.parse(data), process.session) }
          catch (_) { store.failed(process.session) }
        }
      }
      onExited: store.failed(session)
    }
  }
}
