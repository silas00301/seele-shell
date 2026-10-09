import QtQuick
import QtQuick.Controls

// A device level, drawn as the shared LevelTrack. It stays a Slider so the
// keyboard and accessibility contract is Qt's.
Slider {
  id: level
  required property var theme
  required property string title
  required property real current
  property real minimum: 1
  property real maximum: 100
  property real step: 1
  property string suffix: "%"
  property real draft: current
  // A level that sets a colour rather than an amount, such as a light's
  // temperature, passes the colours at the track's start, middle and end,
  // and the track shows the setting in that colour.
  property var spectrum: []
  // `changing` follows the pointer for a device that can take every step,
  // such as a light being dimmed; `committed` is the value the user let go on.
  signal changing(real value)
  signal committed(real value)
  objectName: level.title
  Accessible.name: level.title
  implicitHeight: level.theme.rowHeight
  padding: 0
  from: level.minimum
  to: level.maximum
  stepSize: level.step
  live: true
  hoverEnabled: true
  opacity: level.enabled ? 1 : level.theme.disabledOpacity
  Binding on value {
    value: level.current
    when: !level.pressed
    restoreMode: Binding.RestoreNone
  }
  onMoved: {
    level.draft = level.value
    level.changing(level.value)
  }
  // Only user input commits. A state update never sends another request.
  onPressedChanged: {
    if (level.pressed) level.draft = level.value
    else level.committed(level.draft)
  }
  Keys.onReleased: event => {
    if (!event.isAutoRepeat && (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Home || event.key === Qt.Key_End)) level.committed(level.value)
  }
  background: LevelTrack {
    implicitHeight: level.theme.rowHeight
    theme: level.theme
    ratio: level.visualPosition
    title: level.title
    value: Math.round(level.value) + level.suffix
    hovered: level.hovered && level.enabled
    pressed: level.pressed
    focused: level.activeFocus
    spectrum: level.spectrum
  }
  handle: Item {}
  HoverHandler { cursorShape: level.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
