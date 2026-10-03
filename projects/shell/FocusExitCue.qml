import QtQuick

// One play of the focus-exit cue. The shell maps a rim per output while
// `playing` is true; this object only owns the fade, so every screen shares
// one clock and a second completion replaces a cue still on screen.
Item {
  id: cue

  required property var theme

  property real strength: 0
  readonly property bool playing: fade.running

  function play() {
    fade.restart()
  }

  SequentialAnimation {
    id: fade

    NumberAnimation {
      target: cue
      property: "strength"
      from: 0
      to: 1
      duration: cue.theme.durationFast
      easing.type: Easing.OutCubic
    }
    PauseAnimation { duration: cue.theme.durationGlance }
    NumberAnimation {
      target: cue
      property: "strength"
      to: 0
      duration: cue.theme.durationSettle
      easing.type: Easing.InCubic
    }
  }
}
