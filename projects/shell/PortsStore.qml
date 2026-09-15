import "../shared/ListModels.js" as Models
import QtQuick
import Quickshell
import Quickshell.Io
import "../shared/Native.js" as Bridge

// The panel's view of `seele-ports`. Discovery, ownership, privilege and the
// escalation rule are the worker's; what is kept here is what one open panel
// is looking at. Nothing is retained after the panel closes, and no request is
// sent by merely listing ports.
Scope {
  id: store
  property alias model: rows
  ListModel { id: rows }

  property var snapshot: ({ version: 1, rows: [], total: 0, limited: false, query: ({}) })
  property bool panelOpen: false
  property string query: ""
  // A reviewed stop, exactly as the worker described it. It is replaced only
  // by another plan and cleared by anything that could have changed it.
  property var plan: null
  property var outcome: null
  property string expanded: ""
  property string error: ""
  property string actionError: ""
  property var schemes: ({})
  property var queue: []
  // The one request waiting for its answer. Every action is a round trip
  // through the worker, so a second one never overlaps a confirmation.
  property string pending: ""
  property string copied: ""
  readonly property int total: snapshot.total || 0
  readonly property bool limited: !!snapshot.limited
  readonly property bool busy: pending !== "" || queue.length > 0

  function accept(value) {
    if (!value) return
    if (value.type === "snapshot" && value.version !== 1) return
    if (value.type === "snapshot") {
      // A snapshot answers the requests that have no reply of their own; a
      // typed reply always precedes the snapshot that follows it.
      if (pending === "query" || pending === "refresh" || pending === "panel") finish()
      snapshot = value
      error = ""
      Models.reconcile(rows, value.rows || [], "entry", function (item) { return item.id })
      // A row that is gone cannot be acted on, and a confirmation that names
      // it has to go with it rather than wait for a Confirm that would be
      // refused anyway.
      if (plan && !find(plan.id)) dismiss()
      if (expanded !== "" && !find(expanded)) expanded = ""
      return
    }
    if (value.type === "plan") reviewed(value)
    else if (value.type === "stop") stopped(value)
    else if (value.type === "identify") identified(value)
    else if (value.type !== "select") return
    finish()
  }
  function finish() {
    pending = ""
    guard.stop()
    Qt.callLater(runNext)
  }
  function find(id) {
    var list = snapshot.rows || []
    for (var i = 0; i < list.length; i++) if (list[i].id === id) return list[i]
    return null
  }
  function reviewed(value) {
    if (!value.ok) { actionError = failure(value.error); plan = null; return }
    // A plan that arrived for a row the reader has since folded away is not a
    // confirmation anyone is looking at.
    if (value.id !== expanded) { plan = null; return }
    actionError = ""
    outcome = null
    plan = value
  }
  function stopped(value) {
    outcome = value
    plan = null
    actionError = value.ok ? "" : value.remaining && !value.error
      ? "The listener is still there. Stopping it did not free the port."
      : failure(value.error)
  }
  function identified(value) {
    actionError = value.ok ? "" : failure(value.error)
  }
  function failure(code) {
    return Bridge.call("ports.failure", [code || ""])
  }
  function summary(entry) {
    return Bridge.call("ports.summary", [entry])
  }
  // The scheme a row proposes: the one the reader typed into the search, the
  // one they picked for this row, and otherwise plain HTTP, which the panel
  // always shows so a port is never mistaken for proof of it.
  function scheme(id) {
    if (schemes[id]) return schemes[id]
    var current = snapshot.query || ({})
    return current.explicitScheme ? current.scheme : "http"
  }
  function setScheme(id, value) {
    var next = Object.assign({}, schemes)
    next[id] = value
    schemes = next
  }
  function url(entry) {
    if (!entry) return ""
    return Bridge.call("ports.url", [entry.destination, entry.port, scheme(entry.id)])
  }
  function openUrl(entry) {
    var target = url(entry)
    if (target === "") return false
    Qt.openUrlExternally(target)
    return true
  }
  function copy(entry) {
    var target = url(entry)
    if (target === "") return false
    copied = ""
    clipboard.payload = target
    clipboard.stdinEnabled = true
    clipboard.running = true
    return true
  }

  function send(value) {
    var next = Bridge.call("ports.pending", [queue, value])
    if (next === null) return
    if (next.error) { actionError = next.error; return }
    queue = next.queue
    runNext()
  }
  function runNext() {
    if (pending !== "" || !queue.length || !worker.running) return
    var message = queue[0]
    queue = queue.slice(1)
    pending = message.op
    guard.restart()
    worker.write(JSON.stringify(message) + "\n")
  }
  function refresh() { send({ op: "refresh" }) }
  function select(id, pid) {
    dismiss()
    send({ op: "select", id: id, pid: pid })
  }
  function identify(id) { send({ op: "identify", id: id }) }
  // Reviewing is the only way to reach a stop, and it is a request of its own:
  // the disclosure it returns is read from systemd at this moment, not kept
  // from the last refresh.
  function review(id, mode) {
    var entry = find(id)
    if (!entry || busy) return false
    if (mode === "force" && !(outcome && outcome.canForce && outcome.token === entry.token)) return false
    plan = null
    outcome = mode === "force" ? outcome : null
    actionError = ""
    send({ op: "plan", id: id, mode: mode })
    return true
  }
  function confirm() {
    if (!plan || !plan.ok) return false
    var reviewed = plan
    plan = null
    // The row must still be the row that was described. The worker checks this
    // again against the kernel, and the privileged helper a third time after
    // authentication; this only keeps an obviously stale Confirm off the wire.
    var entry = find(reviewed.id)
    if (!entry || entry.token !== reviewed.token) {
      actionError = failure("changed")
      return false
    }
    send({ op: "stop", token: reviewed.token, mode: reviewed.mode || "graceful" })
    return true
  }
  function dismiss() {
    plan = null
  }
  function expand(id) {
    if (expanded !== id) dismiss()
    expanded = expanded === id ? "" : id
    outcome = null
    actionError = ""
  }

  onQueryChanged: search.restart()
  onPanelOpenChanged: {
    if (!panelOpen) {
      expanded = ""
      plan = null
      outcome = null
      actionError = ""
      schemes = ({})
      copied = ""
    }
    send({ op: "panel", open: panelOpen })
  }
  Timer {
    id: search
    interval: 120
    onTriggered: store.send({ op: "query", text: store.query })
  }

  Process {
    id: worker
    command: ["seele-ports"]
    running: true
    stdinEnabled: true
    stdout: SplitParser {
      onRead: data => { try { store.accept(JSON.parse(data)) } catch (_) {} }
    }
    stderr: StdioCollector {}
    onExited: {
      store.error = "The port inspector stopped."
      store.snapshot = ({ version: 1, rows: [], total: 0, limited: false, query: ({}) })
      Models.reconcile(rows, [], "entry", function (item) { return item.id })
      store.plan = null
      restart.restart()
    }
  }
  Timer {
    id: restart
    interval: 2000
    onTriggered: {
      worker.running = true
      if (store.panelOpen) store.send({ op: "panel", open: true })
    }
  }
  Process {
    id: clipboard
    property string payload: ""
    command: ["wl-copy", "--type", "text/plain;charset=utf-8"]
    onStarted: { write(payload); stdinEnabled = false }
    onExited: function (code) {
      stdinEnabled = true
      store.copied = code === 0 ? clipboard.payload : ""
      if (code !== 0) store.actionError = "The address could not be copied."
    }
  }
  // Authentication is a person at a prompt, so the guard is long. What it
  // catches is a worker that stopped answering, not a slow decision.
  Timer {
    id: guard
    interval: 180000
    onTriggered: {
      store.pending = ""
      store.plan = null
      store.actionError = "The port inspector did not answer. Refresh and try again."
      Qt.callLater(store.runNext)
    }
  }
}
