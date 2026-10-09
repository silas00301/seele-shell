import QtQuick
import QtQuick.Controls

// A one-shot action, drawn as Material 3 Expressive's button: a full pill in
// the secondary container at rest, the primary colour when it is the action
// that is on, the error container when it destroys something. Held, it
// pinches to the small corner; on, it squares towards the medium corner, the
// way an Expressive toggle button says it has latched.
Button {
  id: button
  required property var theme
  property bool selected: false
  property bool danger: false
  readonly property color container: button.danger ? button.theme.errorContainer
    : button.selected ? button.theme.primary
    : button.theme.secondaryContainer
  readonly property color content: button.danger ? button.theme.textOnErrorContainer
    : button.selected ? button.theme.textOnPrimary
    : button.theme.textOnSecondaryContainer
  implicitHeight: theme.controlHeight
  implicitWidth: Math.max(theme.controlHeight * 2, label.implicitWidth + theme.buttonPadding * 2)
  opacity: enabled ? 1 : theme.disabledOpacity
  hoverEnabled: true
  // A button an application offers as its way out of a state has to be
  // reachable without the pointer. The ring is what reports where Tab has
  // landed, and Enter answers it the way Space does.
  focusPolicy: Qt.StrongFocus
  Keys.onPressed: event => {
    if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
    if (!event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) button.clicked()
    event.accepted = true
  }
  contentItem: Text {
    id: label
    text: button.text
    textFormat: Text.PlainText
    color: button.content
    font.family: button.theme.fontFamily
    font.pixelSize: button.theme.textLabel
    font.weight: button.theme.weightMedium
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
    Behavior on color { ColorAnimation { duration: button.theme.durationFast } }
  }
  background: Rectangle {
    radius: button.down ? button.theme.shapeSmall
      : button.selected ? button.theme.shapeMedium
      : height / 2
    color: button.container
    antialiasing: true
    Behavior on color { ColorAnimation { duration: button.theme.durationFast } }
    Behavior on radius {
      NumberAnimation {
        duration: button.theme.durationFastSpatial
        easing.type: Easing.BezierSpline
        easing.bezierCurve: button.theme.springFastSpatial
      }
    }
    HoverWash {
      theme: button.theme
      hovered: button.hovered && button.enabled
      pressed: button.down
      tint: button.theme.alpha(button.content, button.theme.stateHover)
    }
    FocusRing { theme: button.theme; shown: button.activeFocus }
  }
  HoverHandler { cursorShape: button.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
