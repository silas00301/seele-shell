import QtQuick

Rectangle {
  id: wash
  required property var theme
  property bool hovered: false

  anchors.fill: parent
  radius: (parent as Rectangle) ? (parent as Rectangle).radius : wash.theme.radius
  color: hovered ? wash.theme.hoverColor : wash.theme.clearColor
  antialiasing: true

  Behavior on color { ColorAnimation { duration: wash.theme.durationFast } }
}
