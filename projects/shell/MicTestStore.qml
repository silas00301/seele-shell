import QtQuick
import Quickshell
import Quickshell.Io
import "mic-test.js" as MicTest

// The Audio panel's microphone test, for as long as the panel is open. The
// worker owns the capture stream, the playback stream and the five-second
// sample; closing the panel ends its process, and with it all three. Nothing
// here survives that, so reopening the panel starts idle by construction.
Scope {
  id: store

  property bool panelOpen: false
  // The Audio panel's own device model, passed through rather than queried
  // again, so the test always tests the microphone the panel shows selected.
  property var devices: []
  property string chosenOutput: ""
  property var status: ({ mode: "idle", sample: false, clipped: false, input: "", output: "", muted: false, error: "" })
  property var meter: ({ level: 0, peak: 0, clipped: false, remaining: null })
  property var users: []
  property bool detection: false
  property bool usageReady: false
  // The mode a microphone-use warning is standing in front of, empty when no
  // warning is open. Cancelling clears it and starts nothing.
  property string confirming: ""
  property string notice: ""

  readonly property var resolved: MicTest.devices(store.devices, store.chosenOutput, store.status)
  readonly property var presentation: MicTest.view(store.status, store.meter)
  readonly property var usage: MicTest.users(store.users, store.detection)
  readonly property var outputs: store.resolved.options
  readonly property string output: store.resolved.output
  readonly property string outputName: store.resolved.outputName
  readonly property string input: store.resolved.input
  readonly property string inputName: store.resolved.inputName
  readonly property bool active: store.presentation.active

  function accept(line) {
    var value = JSON.parse(line)
    if (value.mode !== undefined) {
      store.status = value
      if (value.error) store.notice = ""
      return
    }
    if (value.level !== undefined) {
      store.meter = value
      return
    }
    if (value.users !== undefined) {
      store.users = value.users || []
      store.detection = value.detection !== false
      store.usageReady = true
    }
  }
  function send(request) {
    if (worker.running) worker.write(JSON.stringify(request) + "\n")
  }
  // Both modes reach the microphone-use gate through the same call, so neither
  // can start without it.
  function begin(action, acknowledged) {
    if (!store.usageReady) { store.notice = "Checking microphone use…"; return }
    var result = MicTest.start(action, store.input, store.output, store.users, store.detection, acknowledged === true)
    if (result.error) { store.confirming = ""; store.notice = result.error; return }
    if (result.confirm) { store.confirming = result.action; return }
    store.confirming = ""
    store.notice = ""
    store.meter = ({ level: 0, peak: 0, clipped: false, remaining: null })
    store.send(result.command)
  }
  function confirm() { if (store.confirming) store.begin(store.confirming, true) }
  function cancel() { store.confirming = "" }
  function replay() {
    store.confirming = ""
    store.notice = ""
    store.send({ command: "replay", input: store.input, output: store.output })
  }
  function stop() {
    store.confirming = ""
    store.send({ command: "stop" })
  }
  function chooseOutput(node) {
    store.chosenOutput = String(node || "")
    // Changing where the test plays ends the current one rather than moving a
    // stream that is already running to a different output.
    if (store.active) store.stop()
  }
  function idle() {
    store.status = ({ mode: "idle", sample: false, clipped: false, input: "", output: "", muted: false, error: "" })
    store.meter = ({ level: 0, peak: 0, clipped: false, remaining: null })
    store.users = []
    store.detection = false
    store.usageReady = false
    store.confirming = ""
    store.notice = ""
  }
  // A device that disappears, or a microphone the panel no longer has
  // selected, ends the running test instead of continuing on something else.
  function reconcile() {
    if (!store.active) return
    if (store.resolved.lost) {
      store.notice = store.resolved.message
      store.stop()
      return
    }
    if (store.status.input && store.input && store.status.input !== store.input) {
      store.notice = "The selected microphone changed. Start the test again."
      store.stop()
    }
  }
  onResolvedChanged: Qt.callLater(store.reconcile)
  onPanelOpenChanged: if (!panelOpen) store.idle()

  // Bumped only by the restart timer, so a crashed worker can re-evaluate the
  // binding below. `running` is never assigned imperatively: that would drop
  // its `panelOpen` dependency, and a capture started after a restart could
  // then outlive the Audio panel.
  property int attempt: 0

  Process {
    id: worker
    command: ["seele-mic-test"]
    running: store.panelOpen && store.attempt >= 0
    stdinEnabled: true
    stdout: SplitParser {
      onRead: data => { try { store.accept(data) } catch (_) {} }
    }
    onExited: {
      if (!store.panelOpen) return
      store.idle()
      store.notice = "The microphone test helper stopped."
      restart.restart()
    }
  }
  Timer {
    id: restart
    interval: 2000
    onTriggered: if (store.panelOpen) store.attempt++
  }
}
