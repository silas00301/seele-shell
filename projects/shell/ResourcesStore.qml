pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io

ResourcesState {
  id: store
  property bool panelOpen: false
  property int generation: 0
  onPanelOpenChanged: {
    generation++
    retry.stop()
    search.stop()
    session.active = false
    reset()
    if (panelOpen) session.active = true
  }
  onRequest: value => {
    if (value.op === "query") search.restart()
    else send(value)
  }
  function send(value) {
    const activeWorker = session.item as Process
    if (panelOpen && activeWorker && activeWorker.running)
      activeWorker.write(JSON.stringify(value) + "\n")
  }
  property Timer search: Timer {
    interval: 120
    onTriggered: store.send({op: "query", text: store.query})
  }
  // A closing panel destroys its Process. The captured generation also refuses
  // queued stdout/exit signals from that object after a rapid reopen.
  property Loader session: Loader {
    active: false
    sourceComponent: Component {
      Process {
        id: worker
        property int sessionGeneration: -1
        readonly property bool currentSession: store.panelOpen && sessionGeneration === store.generation
        command: ["seele-resources"]
        stdinEnabled: true
        Component.onCompleted: {
          sessionGeneration = store.generation
          running = true
        }
        onStarted: {
          if (!currentSession) return
          store.send({op: "query", text: store.query})
          store.send({op: "sort", value: store.sort})
          store.send({op: "select", id: store.selected})
        }
        stdout: SplitParser {
          onRead: data => { if (worker.currentSession) { try { store.accept(JSON.parse(data)) } catch (_) {} } }
        }
        stderr: StdioCollector {}
        onExited: {
          if (!currentSession) return
          const selected = store.selected
          store.reset()
          store.selected = selected
          store.error = "The resource inspector stopped. Reconnecting…"
          store.retry.restart()
        }
      }
    }
  }
  property Timer retry: Timer {
    interval: 2000
    onTriggered: {
      if (!store.panelOpen) return
      store.generation++
      store.session.active = false
      store.session.active = true
    }
  }
}
