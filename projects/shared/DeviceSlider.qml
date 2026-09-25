import QtQuick
import QtQuick.Controls

// A device level is drawn as the Audio panel and the player draw theirs: a
// well carrying its own fill, named inside the track it sets. It stays a
// Slider so the keyboard and accessibility contract is Qt's.
Slider {
  id: level
  required property var theme
  required property string title
  required property real current
  property real minimum: 1
  property real maximum: 100
  property real step: 1
  property string suffix: "%"
  // Some devices expose setters but no readback. Their track stays empty and
  // says "Set" until this session has completed a write.
  property bool valueKnown: true
  property real draft: current
  signal committed(real value)
  objectName: level.title
  Accessible.name: level.title
  implicitHeight: level.theme.rowHeight
  padding: 0
  from: level.minimum
  to: level.maximum
  stepSize: level.step
  live: true
  opacity: level.enabled ? 1 : level.theme.disabledOpacity
  Binding on value {
    value: level.current
    when: !level.pressed
    restoreMode: Binding.RestoreNone
  }
  onMoved: level.draft = level.value
  // Only user input commits. A state update never sends another request.
  onPressedChanged: {
    if (level.pressed) level.draft = level.value
    else level.committed(level.draft)
  }
  Keys.onReleased: event => {
    if (!event.isAutoRepeat && (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Home || event.key === Qt.Key_End)) level.committed(level.value)
  }
  background: Rectangle {
    radius: level.theme.radius
    color: level.theme.wellColor
    border.width: 1
    border.color: level.activeFocus ? level.theme.accent : level.theme.alpha(level.theme.text, 0.05)
    clip: true
    antialiasing: true
    Rectangle {
      width: parent.width * (level.valueKnown || level.pressed ? level.visualPosition : 0)
      height: parent.height
      radius: parent.radius
      color: level.theme.fillColor
      antialiasing: true
    }
    Text {
      id: levelReadout
      anchors { right: parent.right; rightMargin: level.theme.spaceLarge; verticalCenter: parent.verticalCenter }
      text: level.valueKnown || level.pressed ? Math.round(level.value) + level.suffix : "Set"
      textFormat: Text.PlainText
      color: level.theme.subtext
      font.family: level.theme.fontFamily
      font.pixelSize: level.theme.textBody
    }
    Text {
      anchors { left: parent.left; leftMargin: level.theme.spaceLarge; right: levelReadout.left; rightMargin: level.theme.spaceMedium; verticalCenter: parent.verticalCenter }
      text: level.title
      textFormat: Text.PlainText
      elide: Text.ElideRight
      color: level.theme.text
      font.family: level.theme.fontFamily
      font.pixelSize: level.theme.textBody
      font.weight: level.theme.weightStrong
    }
  }
  handle: Item {}
  HoverHandler { cursorShape: level.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
