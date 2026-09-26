import QtQuick
import Quickshell
import Quickshell.Io
import "calendar.js" as Calendar

Scope {
  id: store
  signal healthPublished(string state, double success)
  property double healthSuccess: 0
  property bool configured: false
  property bool connected: false
  property var calendars: []
  property var selected: []
  property var events: []
  property var colors: ({})
  property string rangeStart: ""
  property string rangeEnd: ""
  property var ranges: []
  property double refreshedAt: 0
  property string error: ""
  property bool ready: false
  property bool selecting: false
  property string requestedDay: ""
  readonly property bool stale: !connected || Date.now() / 1000 - refreshedAt > 600

  function send(action, extra) {
    if (!worker.running || !ready) return
    worker.write(JSON.stringify(Object.assign({action: action}, extra || {})) + "\n")
  }
  function setup(clientId) { send("setup", {client_id: clientId}) }
  function signin() { send("signin") }
  function choose(id) {
    if (selecting || !connected) return
    var next = selected.slice(), at = next.indexOf(id)
    if (at >= 0) next.splice(at, 1)
    else next.push(id)
    selecting = true
    send("select", {ids: next})
  }
  function forDay(date) {
    if (!date || (requestedDay === date && hasDay(date))) return
    requestedDay = date
    if (ready) send("day", {date: date})
  }
  function agenda(date) { return Calendar.agenda(events, selected, date) }
  function dots(date) { return Calendar.dots(events, calendars, selected, date) }
  function indicator(now) { return Calendar.indicator(events, selected, now) }
  function eventColor(event) { return Calendar.color(event, calendars, colors) }
  function meeting(event) { return Calendar.meeting(event) }
  function safeLink(value) { return Calendar.safeLink(value) }
  function allDayLabel(event) { return Calendar.allDayLabel(event) }
  function rsvp(event) { return Calendar.rsvp(event) }
  function description(event) { return Calendar.description(event) }
  function hasDay(date) { return ranges.some(function(range) { return date >= range[0] && date < range[1] }) || (ranges.length === 0 && rangeStart !== "" && date >= rangeStart && date < rangeEnd) }
  function receive(line) {
    try {
      var data = JSON.parse(line)
      var wasReady = ready
      ready = true
      configured = !!data.configured
      connected = !!data.connected
      calendars = data.calendars || []
      selected = data.selected || []
      events = data.events || []
      colors = data.colors || {}
      rangeStart = data.range_start || ""
      rangeEnd = data.range_end || ""
      ranges = data.ranges || []
      refreshedAt = data.refreshed_at || 0
      error = data.error || ""
      selecting = false
      if (connected) healthSuccess = Date.now()
      healthPublished(!configured ? "setup-required" : connected ? "healthy" : "degraded", healthSuccess)
      if (!wasReady && requestedDay) send("day", {date: requestedDay})
    } catch (_) { error = "Calendar worker returned invalid data." }
  }
  Process {
    id: worker
    command: ["seele-calendar"]
    running: true
    stdinEnabled: true
    stdout: SplitParser { onRead: line => store.receive(line) }
    stderr: StdioCollector {}
    onExited: { store.ready = false; store.connected = false; store.selecting = false; store.healthPublished("degraded", store.healthSuccess); restart.restart() }
  }
  Timer { id: restart; interval: 3000; onTriggered: worker.running = true }
}
