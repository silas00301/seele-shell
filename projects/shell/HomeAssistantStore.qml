import QtQuick
import Quickshell
import Quickshell.Io

Scope {
  id: store

  property bool configured: false
  property bool connected: false
  property var entities: []
  property string error: ""
  property string pendingEntity: ""
  property string pendingValue: ""
  property bool received: false
  property int generation: 0
  property bool requestPending: false
  property bool timedOut: false
  readonly property bool busy: requestPending || worker.running

  function refresh() {
    if (busy) return
    start(["seele-home-assistant", "status"])
  }

  function setState(entity, value) {
    if (busy || !connected || !entity.controllable || !entity.available) return
    if (value !== "on" && value !== "off") return
    pendingEntity = entity.entity_id
    pendingValue = value
    start(["seele-home-assistant", "set", entity.entity_id, value])
  }

  function start(command) {
    generation += 1
    requestPending = true
    timedOut = false
    received = false
    error = ""
    worker.command = command
    worker.running = true
    watchdog.restart()
  }

  function accept(text) {
    if (timedOut) return
    try {
      var message = JSON.parse(text)
      if (typeof message.configured !== "boolean" || typeof message.connected !== "boolean") throw new Error("invalid")
      configured = message.configured
      connected = message.connected
      if (Array.isArray(message.entities)) entities = message.entities
      error = String(message.error || "")
      received = true
    } catch (_) {
      connected = false
      error = "Home Assistant returned an invalid response."
      received = true
    }
  }

  function finish() {
    watchdog.stop()
    var finishedGeneration = generation
    Qt.callLater(function() {
      if (store.generation !== finishedGeneration) return
      if (!store.received && !store.timedOut) {
        store.connected = false
        store.error = "Home Assistant is unavailable."
      }
      store.pendingEntity = ""
      store.pendingValue = ""
      store.requestPending = false
    })
  }

  function timeout() {
    timedOut = true
    connected = false
    error = "Home Assistant took too long to respond."
    pendingEntity = ""
    pendingValue = ""
    requestPending = false
    worker.running = false
  }

  Process {
    id: worker
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: store.accept(text) }
    // Never forward a helper's stderr into shell logs.
    stderr: StdioCollector { waitForEnd: true }
    onExited: store.finish()
  }

  Timer {
    id: watchdog
    interval: 15000
    onTriggered: store.timeout()
  }

  Timer {
    interval: 30000
    repeat: true
    running: true
    onTriggered: store.refresh()
  }

  Component.onCompleted: refresh()
}
