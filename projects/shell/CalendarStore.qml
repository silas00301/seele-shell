import QtQuick
import Quickshell
import Quickshell.Io

// The resident Google Calendar worker's projection. The worker owns every
// calendar policy and sends a section only when it changed, so each property
// here is replaced only when its own content did and nothing bound to one
// section is rebuilt by another's update.
Scope {
  id: store
  signal healthPublished(string state, string summary, double success)
  // A Retry from Integration Health, answered once the refresh it started ends.
  signal retried(bool ok)

  property bool ready: false
  property var account: ({ status: "connecting", configured: false, account: "", syncing: false, signing_in: false, error: "", refreshed_at: 0, updated: "", stale: true })
  property var calendars: []
  // Day, as yyyy-MM-dd, to the calendar colours drawn under it.
  property var dots: ({})
  property var agenda: ({ day: "", covered: false, loading: false, items: [] })
  property var indicator: null
  property var busyBlocks: []
  property var coverage: []
  property string requestedDay: ""
  property string lastHealth: ""
  property double lastHealthAt: 0
  property bool retrying: false
  property bool retrySawSync: false

  readonly property string status: ready ? account.status : "unavailable"
  readonly property bool signedIn: ["online", "offline", "connecting", "expired"].indexOf(status) >= 0
  readonly property bool configured: account.configured
  readonly property bool connected: signedIn
  readonly property bool stale: account.stale
  readonly property int selectedCount: calendars.filter(function(calendar) { return calendar.selected }).length

  function send(action, extra) {
    if (!worker.running || !ready) return
    worker.write(JSON.stringify(Object.assign({ action: action }, extra || {})) + "\n")
  }
  function setup(clientId) { send("setup", { client_id: clientId }) }
  function forgetClient() { send("setup", { client_id: "" }) }
  function signin() { send("signin") }
  function cancelSignin() { send("cancel") }
  function refresh() { send("refresh") }
  function disconnect() { send("disconnect") }
  function retry() {
    if (!ready || !signedIn) { retried(false); return }
    retrying = true
    retrySawSync = false
    retryTimeout.restart()
    refresh()
  }
  function finishRetry() {
    if (!retrying) return
    retrying = false
    retryTimeout.stop()
    lastHealth = ""
    publishHealth()
    retried(status === "online")
  }
  function choose(id, selected) { send("select", { id: id, selected: selected }) }
  function forDay(date) {
    if (!date || requestedDay === date) return
    requestedDay = date
    send("day", { date: date })
  }
  function browse(date) { if (date) send("browse", { date: date }) }
  function dotsFor(date) { return dots[date] || null }
  function hasDay(date) { return coverage.some(function(range) { return range[0] <= date && date < range[1] }) }
  function busy(from, to) { return busyBlocks.filter(function(block) { return block.start < to && block.end > from }) }

  function publishHealth() {
    var state = "", summary = ""
    if (status === "setup") { state = "setup-required"; summary = "Set up Google Calendar" }
    else if (status === "signed-out") { state = "setup-required"; summary = "Sign in with Google" }
    else if (status === "expired") { state = "disconnected"; summary = "Google sign-in expired" }
    else if (status === "offline") { state = "degraded"; summary = "Offline · showing cached events" }
    else if (status === "online") { state = account.stale ? "degraded" : "healthy"; summary = account.stale ? "Not refreshed recently" : "Connected" }
    else if (status === "unavailable") { state = "degraded"; summary = "Calendar worker restarting" }
    // Connecting and signing in are transitions, not a health state. An
    // unchanged state is republished as a heartbeat well inside the deadline.
    var now = Date.now()
    if (!state || (lastHealth === state + summary && now - lastHealthAt < 120000)) return
    lastHealth = state + summary
    lastHealthAt = now
    healthPublished(state, summary, account.refreshed_at * 1000)
  }

  function receive(line) {
    var data
    try { data = JSON.parse(line) } catch (_) { return }
    var first = !ready
    ready = true
    if (data.account !== undefined) account = data.account
    if (data.calendars !== undefined) calendars = data.calendars
    if (data.dots !== undefined) dots = data.dots
    if (data.agenda !== undefined) agenda = data.agenda
    if (data.indicator !== undefined) indicator = data.indicator
    if (data.busy !== undefined) busyBlocks = data.busy
    if (data.coverage !== undefined) coverage = data.coverage
    // A restarted worker starts from today; tell it where the popup was.
    if (first && requestedDay) send("day", { date: requestedDay })
    if (retrying && account.syncing) retrySawSync = true
    else if (retrying && retrySawSync) finishRetry()
    publishHealth()
  }

  Process {
    id: worker
    command: ["seele-calendar"]
    running: true
    stdinEnabled: true
    stdout: SplitParser { onRead: line => store.receive(line) }
    stderr: StdioCollector {}
    onExited: {
      store.ready = false
      store.indicator = null
      store.publishHealth()
      store.finishRetry()
      restart.restart()
    }
  }
  // The worker bounds a refresh to a minute; this only covers one that never began.
  Timer { id: retryTimeout; interval: 70000; onTriggered: store.finishRetry() }
  Timer { id: restart; interval: 3000; onTriggered: worker.running = true }
}
