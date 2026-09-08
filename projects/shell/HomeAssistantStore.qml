import QtQuick
import Quickshell
import Quickshell.Io

Scope {
  id: store
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
  signal setupComplete()

  function send(message) {
    if (!ready || !worker.running) return false
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
    return preferences.find(function(item) { return item.entity_id === id }) || null
  }
  function edit(id, field, value) {
    var next = preferences.map(function(item) { return Object.assign({}, item) })
    var item = next.find(function(entry) { return entry.entity_id === id })
    if (!item) return
    item[field] = value
    save(next, summary)
  }
  function select(id) {
    var selected = preference(id)
    var next = preferences.filter(function(item) { return item.entity_id !== id })
    if (!selected) next.push({entity_id: id, name: "", room: "", favorite: false})
    save(next, summary === id && selected ? "" : summary)
  }
  function moveTarget(id, offset) {
    var index = preferences.findIndex(function(item) { return item.entity_id === id })
    var entity = entities.find(function(item) { return item.entity_id === id })
    if (!entity) return -1
    for (var i = index + offset; i >= 0 && i < preferences.length; i += offset) {
      var candidate = entities.find(function(item) { return item.entity_id === preferences[i].entity_id })
      if (candidate && (entity.favorite ? candidate.favorite : !candidate.favorite && candidate.room === entity.room)) return i
    }
    return -1
  }
  function move(id, offset) {
    var next = preferences.slice()
    var index = next.findIndex(function(item) { return item.entity_id === id })
    var destination = moveTarget(id, offset)
    if (index < 0 || destination < 0) return
    var swap = next[destination]
    next[destination] = next[index]
    next[index] = swap
    save(next, summary)
  }
  function rows() {
    var result = []
    var favorites = entities.filter(function(item) { return item.favorite })
    if (favorites.length) {
      result.push({heading: "Favorites"})
      favorites.forEach(function(item) { result.push(item) })
    }
    var rooms = []
    entities.forEach(function(item) { if (!item.favorite && rooms.indexOf(item.room) < 0) rooms.push(item.room) })
    rooms.forEach(function(room) {
      var members = entities.filter(function(item) { return !item.favorite && item.room === room })
      var readings = entities.filter(function(item) { return item.room === room && (item.unit === "°C" || item.unit === "°F" || item.device_class === "humidity") && item.entity_id.indexOf("sensor.") === 0 && item.available })
      result.push({heading: room, detail: readings.slice(0, 2).map(function(item) { return item.state + item.unit }).join(" · ")})
      members.forEach(function(item) { result.push(item) })
    })
    return result
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
