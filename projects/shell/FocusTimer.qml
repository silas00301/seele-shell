import QtQuick
import Quickshell
import "focus.js" as Focus

Item {
  id: timer
  property bool initialized: false
  readonly property var timerState: retained.state
  readonly property string label: timerState.status === "done" ? "Done" : Focus.label(timerState.remaining)
  signal completed()

  function command(action, minutes) {
    if (!initialized) return
    var before = retained.state
    var next = Focus.update(before, action, Date.now(), minutes)
    retained.state = next
    if (before.status === "running" && next.status === "done") completed()
  }

  PersistentProperties {
    id: retained
    reloadableId: "seele-focus-timer"
    property var state: Focus.initial()
    onLoaded: {
      if (!Focus.valid(state)) state = Focus.initial()
      timer.initialized = true
      timer.command("tick")
    }
  }

  Timer {
    interval: 250
    repeat: true
    running: timer.initialized && timer.timerState.status === "running"
    onTriggered: timer.command("tick")
  }
}
