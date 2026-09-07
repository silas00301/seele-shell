import QtQuick
import Quickshell
import Quickshell.Io

Scope {
  id: dictation
  property string status: "unavailable"
  property string output: ""
  required property var currentScreen
  readonly property bool active: status === "recording" || status === "transcribing"
  signal level(real value)

  function update(line) {
    try {
      var next = JSON.parse(line).class
      if (["idle", "recording", "transcribing", "stopped", "unavailable"].indexOf(next) < 0) next = "unavailable"
      if (!active && (next === "recording" || next === "transcribing")) output = currentScreen()
      status = next
    } catch (_) { status = "unavailable" }
  }

  Process {
    id: statusWorker
    command: ["voxtype", "status", "--follow", "--format", "json"]
    running: true
    stdout: SplitParser { onRead: data => dictation.update(data) }
    onExited: {
      dictation.status = "unavailable"
      reconnect.restart()
    }
  }
  Timer { id: reconnect; interval: 1000; onTriggered: statusWorker.running = true }

  Process {
    id: levels
    command: ["seele-dictation-levels"]
    stdinEnabled: true
    stdout: SplitParser { onRead: data => dictation.level(Number(data)) }
    onExited: if (dictation.status === "recording") levelReconnect.restart()
  }
  Timer {
    id: levelReconnect
    interval: 1000
    onTriggered: if (dictation.status === "recording") levels.running = true
  }
  onStatusChanged: { levels.running = status === "recording"; if (status !== "recording") level(0) }
}
