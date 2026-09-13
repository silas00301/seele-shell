import "../shared/ListModels.js" as Models
import QtQuick
import Quickshell
import Quickshell.Io
import "ai-activity.js" as Activity

Scope {
  id: store
  property var jobs: []
  property alias model: jobModel
  property var expandedIds: ({})
  ListModel { id: jobModel }
  property string epoch: ""
  property string error: ""
  property string pendingId: ""
  property string actionErrorId: ""
  property string actionError: ""
  property double now: Date.now() / 1000
  readonly property string indicator: Activity.indicator(jobs)

  function reconcile(values) {
    jobs = values
    Models.reconcile(jobModel, values, "modelData", function(item) { return item.id })
  }
  function refresh() {
    if (!poll.running) poll.running = true
  }
  function accept(snapshot) {
    if (!snapshot.ok) {
      reconcile([])
      epoch = ""
      error = "Codex broker unavailable"
      return
    }
    if (epoch !== snapshot.epoch) {
      actionErrorId = ""
      actionError = ""
      expandedIds = ({})
    }
    epoch = String(snapshot.epoch)
    reconcile(Activity.rows(snapshot.jobs, now))
    error = ""
  }
  function act(job, operation) {
    if (action.running || !epoch || !Activity.actions(job.state).some(function(a) { return a.op === operation })) return
    pendingId = job.id
    actionErrorId = ""
    actionError = ""
    action.targetEpoch = epoch
    action.targetState = job.state
    action.targetUpdated = job.updated
    action.payload = JSON.stringify({op:operation,id:job.id,epoch:epoch}) + "\n"
    action.running = true
  }
  function acceptAction(reply) {
    var current = jobs.find(function(job) { return job.id === pendingId })
    if (action.targetEpoch !== epoch || !current) return
    if (!reply.ok && current.state === action.targetState && current.updated === action.targetUpdated) {
      actionErrorId = pendingId
      actionError = "Action failed · refresh and retry"
    }
  }
  Timer {
    interval: 1000; running: true; repeat: true; triggeredOnStart: true
    onTriggered: { store.now = Date.now() / 1000; store.refresh() }
  }
  Process {
    id: poll
    command: ["seele-codex", "request"]
    stdinEnabled: true
    onStarted: { write('{"op":"list"}\n'); stdinEnabled = false }
    onRunningChanged: if (!running) stdinEnabled = true
    stdout: SplitParser { onRead: data => { try { store.accept(JSON.parse(data)) } catch (_) { store.accept({ok:false}) } } }
    onExited: function(code) { if (code !== 0) store.accept({ok:false}) }
  }
  Process {
    id: action
    property string payload: ""
    property string targetEpoch: ""
    property string targetState: ""
    property double targetUpdated: 0
    command: ["seele-codex", "request"]
    stdinEnabled: true
    onStarted: { write(payload); stdinEnabled = false }
    stdout: SplitParser { onRead: data => { try { store.acceptAction(JSON.parse(data)) } catch (_) { store.acceptAction({ok:false}) } } }
    onExited: function(code) {
      if (code !== 0) store.acceptAction({ok:false})
      store.pendingId = ""
      stdinEnabled = true
      store.refresh()
    }
  }
}
