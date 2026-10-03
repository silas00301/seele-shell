import QtQuick
import Quickshell
import Quickshell.Io
import "drift.js" as Drift

// One open Fix me panel's view of `seele-drift`. Diff is a read: it runs when
// the panel opens and when Check again is pressed, and a reply that claims to
// have changed the machine is refused. Restore names only the checks still
// switched on. Nothing here is written down.
Scope {
  id: store

  property bool panelOpen: false
  property var checks: []
  property var skipped: []
  property var restored: []
  property string error: ""
  property bool busy: false
  readonly property int drifted: store.countDrifted(store.checks)

  onPanelOpenChanged: if (panelOpen) diff()

  function countDrifted(list) {
    var count = 0
    var rows = list || []
    for (var i = 0; i < rows.length; i++) if (rows[i] && rows[i].drifted) count++
    return count
  }

  function toggle(id) {
    if (busy) return
    var next = skipped.slice()
    var index = next.indexOf(id)
    if (index < 0) next.push(id)
    else next.splice(index, 1)
    skipped = next
  }

  function accept(value) {
    if (!value || value.version !== 1) {
      error = "The check could not be read."
      return
    }
    if (value.action === "diff" && value.mutated) {
      error = "The check changed the machine."
      return
    }
    if (!value.ok) {
      error = value.error || "The check could not be read."
      checks = []
      restored = value.restored || []
      return
    }
    error = ""
    checks = value.checks || []
    restored = value.restored || []
    skipped = []
  }

  function diff() {
    if (busy) return
    run(Drift.argumentsFor("diff", []))
  }

  function apply() {
    if (busy) return
    run(Drift.argumentsFor("apply", Drift.chosenIds(checks, skipped)))
  }

  function run(command) {
    if (!command || probe.running) return
    error = ""
    probe.command = command
    busy = true
    probe.running = true
  }

  function finish(text) {
    try { accept(JSON.parse(text)) }
    catch (_) { error = "The check could not be read." }
  }

  Process {
    id: probe
    stdout: StdioCollector {
      onStreamFinished: store.finish(text)
    }
    stderr: StdioCollector {}
    onExited: store.busy = false
  }
}
