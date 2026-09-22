pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io

// Starting the process opens a new observation session; closing kills it and
// discards every counter and trace. The worker has no off-panel sampling mode.
Scope {
  id: store
  property bool panelOpen: false
  property var snapshot: ({ rows: [], elapsed: 0, limited: false, interfaceLimit: 0, historyCapacity: 0 })
  property string error: ""
  property string selectedId: ""
  property bool received: false
  property int generation: 0
  property var worker: null
  readonly property var rows: snapshot.rows || []
  readonly property var selected: {
    for (var i = 0; i < rows.length; i++) if (rows[i].id === selectedId) return rows[i]
    return null
  }
  readonly property int selectedIndex: {
    for (var i = 0; i < rows.length; i++) if (rows[i].id === selectedId) return i
    return -1
  }
  function accept(value, session) {
    if (session !== generation || !panelOpen || !value || value.version !== 1 || !Array.isArray(value.rows)) return
    snapshot = value
    error = value.error || ""
    received = true
    if (!selectedId && rows.length) {
      var first = rows.filter(function(row) { return row.state === "Up" })[0] || rows[0]
      selectedId = first.id
    }
  }
  function select(index) {
    if (index >= 0 && index < rows.length) selectedId = rows[index].id
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
    snapshot = ({ rows: [], elapsed: 0, limited: false, interfaceLimit: 0, historyCapacity: 0 })
    selectedId = ""
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
    snapshot = ({ rows: [], elapsed: 0, limited: false, interfaceLimit: 0, historyCapacity: 0 })
    error = "Network activity stopped. Retry starts a new session."
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
      command: ["seele-network-activity"]
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
