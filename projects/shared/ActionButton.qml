import QtQuick
import QtQuick.Controls

Button {
  id: button
  required property var theme
  property bool selected: false
  property bool danger: false
  implicitHeight: theme.controlHeight
  implicitWidth: Math.max(theme.controlHeight, label.implicitWidth + theme.spaceLarge * 2)
  opacity: enabled ? 1 : 0.45
  hoverEnabled: true
  // A button an application offers as its way out of a state has to be
  // reachable without the pointer. The ring the background already draws on
  // `activeFocus` is what reports where Tab has landed.
  focusPolicy: Qt.StrongFocus
  contentItem: Text {
    id: label
    text: button.text
    color: button.danger ? button.theme.red : button.selected ? button.theme.accent : button.theme.text
    font.family: button.theme.fontFamily
    font.pixelSize: button.theme.textLabel
    font.weight: button.theme.weightMedium
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
  }
  background: Rectangle {
    readonly property color resting: button.danger ? button.theme.dangerTint : button.selected ? button.theme.selectedColor : button.theme.cardColor
    radius: button.theme.radius
    color: button.down ? button.theme.pressColor : resting
    border.width: 1
    border.color: button.activeFocus ? button.theme.accent : button.theme.cardBorder
    Behavior on color { ColorAnimation { duration: button.theme.durationFast } }
    HoverWash { theme: button.theme; hovered: button.hovered && button.enabled }
  }
  HoverHandler { cursorShape: button.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
