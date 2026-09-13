import QtQuick
import Quickshell
import Quickshell.Io
import "../shared/Native.js" as Bridge

Scope {
  id: store
  signal healthPublished(string state, double success)
  property double healthSuccess: 0
  property bool configured: false
  property bool connected: false
  property bool ready: false
  property var entities: []
  property var preferences: []
  property var catalog: []
  property var pending: ({})
  property var requests: ({})
  property int serial: 0
  property string error: ""
  property string url: ""
  property string summary: ""
  property string summaryText: ""
  property bool settingsPending: false
  readonly property bool busy: settingsPending || !ready
  readonly property var projection: Bridge.call("home_assistant.project", [entities, preferences])
  signal setupComplete()

  function send(message) {
    if (!ready || !worker.running || Object.keys(requests).length >= 128) return false
    message.request = ++serial
    var next = Object.assign({}, requests)
    // Never retain credentials in the request bookkeeping.
    next[message.request] = { action: message.action, time: Date.now() }
    requests = next
    worker.write(JSON.stringify(message) + "\n")
    return true
  }
  function refresh() {
    if (!worker.running) worker.running = true
    else send({action: "refresh"})
  }
  function setup(server, token) {
    if (settingsPending) return
    error = ""
    settingsPending = send({action: "setup", url: server, token: token})
  }
  function discover(open) {
    send({action: "catalog", open: open})
  }
  function setState(entity, value) {
    if (value !== "on" && value !== "off") return
    setValue(entity, {state: value})
  }
  function setValue(entity, desired) {
    if (!connected || settingsPending || !entity.available || !entity.controllable || pending[entity.entity_id]) return
    if (send({action: "set", entity_id: entity.entity_id, desired: desired})) {
      var next = Object.assign({}, pending)
      next[entity.entity_id] = desired
      pending = next
    }
  }
  function save(entries, reading) {
    if (settingsPending) return
    error = ""
    settingsPending = send({action: "preferences", entities: entries, summary: reading})
  }
  function preference(id) {
    return Object.prototype.hasOwnProperty.call(projection.preferences, id) ? projection.preferences[id] : null
  }
  function edit(id, field, value) {
    var next = Bridge.call("home_assistant.edit", [preferences, id, field, value])
    if (next !== null) save(next, summary)
  }
  function select(id) {
    var next = Bridge.call("home_assistant.select", [preferences, id, summary])
    save(next.entries, next.summary)
  }
  function moveTarget(id, offset) {
    var move = Object.prototype.hasOwnProperty.call(projection.moves, id) ? projection.moves[id] : null
    return !move ? -1 : offset === -1 ? move.before : offset === 1 ? move.after : -1
  }
  function move(id, offset) {
    var next = Bridge.call("home_assistant.move", [preferences, id, moveTarget(id, offset)])
    if (next !== null) save(next, summary)
  }
  function rows() {
    return projection.rows
  }
  function receive(line) {
    try {
      var data = JSON.parse(line)
      if (data.ready) ready = true
      if (typeof data.configured === "boolean") {
        configured = data.configured
        connected = data.connected === true
        if (JSON.stringify(entities) !== JSON.stringify(data.entities || [])) entities = data.entities || []
        if (JSON.stringify(preferences) !== JSON.stringify(data.preferences || [])) preferences = data.preferences || []
        pending = data.pending || {}
        error = data.error || ""
        url = data.url || ""
        summary = data.summary || ""
        summaryText = data.summary_text || ""
        if(connected) healthSuccess=Date.now()
        healthPublished(!configured ? "setup-required" : connected ? "healthy" : "disconnected",healthSuccess)
      }
      if (Array.isArray(data.catalog) && JSON.stringify(catalog) !== JSON.stringify(data.catalog)) catalog = data.catalog
      if (data.request !== undefined) {
        var request = requests[data.request]
        var next = Object.assign({}, requests)
        delete next[data.request]
        requests = next
        if (request && (request.action === "setup" || request.action === "preferences")) settingsPending = false
        if (data.ok === false) error = data.error || "Home Assistant could not complete the request."
        if (request && request.action === "setup" && data.ok) setupComplete()
      }
    } catch (_) {
      error = "Home Assistant returned an invalid response."
      connected = false
    }
  }
  function stopped() {
    ready = false
    connected = false
    settingsPending = false
    pending = ({})
    requests = ({})
    error = "Home Assistant connection stopped. Restarting…"
    healthPublished("disconnected",healthSuccess)
    restart.start()
  }
  Process {
    id: worker
    command: ["seele-home-assistant", "watch"]
    running: true
    stdinEnabled: true
    stdout: SplitParser { onRead: line => store.receive(line) }
    stderr: StdioCollector {}
    onExited: store.stopped()
  }
  Timer { id: restart; interval: 3000; onTriggered: worker.running = true }
  Timer {
    interval: 5000
    running: true
    repeat: true
    onTriggered: {
      for (var key in store.requests) {
        if (Date.now() - store.requests[key].time > 150000) {
          worker.running = false
          break
        }
      }
    }
  }
}
