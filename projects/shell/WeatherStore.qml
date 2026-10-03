import QtQuick
import Quickshell
import Quickshell.Io

// The resident weather worker's projection. The worker owns the place, the
// forecast, units and every label, and sends a section only when it changed,
// so each property here is replaced only when its own content did. Nothing
// here parses a forecast or decides what a reading means.
Scope {
  id: store
  // The worker's Integration Health payload, already decided.
  signal healthPublished(var health)
  // A Health Retry, answered once the refresh it started has ended.
  signal retried(int token, bool ok)

  property bool ready: false
  property var place: ({ name: "", detail: "", chosen: false, city: "" })
  property var status: ({ state: "connecting", fetching: false, stale: true, label: "", badge: "", error: "" })
  property var current: null
  property var hours: []
  property var days: []
  property var search: ({ query: "", busy: false, error: "", results: [] })
  // Kept so a restarting worker still reports when the forecast last arrived.
  property double lastSuccess: 0

  // `no-place`, `connecting`, `online` or `offline`, or `unavailable` while
  // the worker is restarting.
  readonly property string mode: ready ? status.state : "unavailable"

  function send(action, extra) {
    if (!worker.running || !ready) return false
    worker.write(JSON.stringify(Object.assign({ action: action }, extra || {})) + "\n")
    return true
  }
  function refresh() { send("refresh") }
  function retry(token) { if (!send("refresh", { token: token })) retried(token, false) }
  function find(query) { send("search", { query: query }) }
  function choose(id) { send("choose", { id: id }) }
  function reset() { send("reset") }

  function receive(line) {
    var data
    try { data = JSON.parse(line) } catch (_) { return }
    ready = true
    if (data.place !== undefined) place = data.place
    if (data.status !== undefined) status = data.status
    if (data.current !== undefined) current = data.current
    if (data.hours !== undefined) hours = data.hours
    if (data.days !== undefined) days = data.days
    if (data.search !== undefined) search = data.search
    if (data.health) {
      lastSuccess = data.health.last_success
      healthPublished(data.health)
    }
    if (data.retried !== undefined) data.retried.forEach(function(answer) { store.retried(answer.token, answer.ok) })
  }

  Process {
    id: worker
    command: ["seele-weather"]
    running: true
    stdinEnabled: true
    stdout: SplitParser { onRead: line => store.receive(line) }
    stderr: StdioCollector {}
    onExited: {
      store.ready = false
      store.healthPublished({ state: "degraded", summary: "Weather worker restarting", last_success: store.lastSuccess })
      restart.restart()
    }
  }
  Timer { id: restart; interval: 3000; onTriggered: worker.running = true }
}
