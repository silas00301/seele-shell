import QtQuick

Rectangle {
  id: wash
  required property var theme
  property bool hovered: false
  property color tint: wash.theme.hoverColor

  anchors.fill: parent
  radius: (parent as Rectangle) ? (parent as Rectangle).radius : wash.theme.radius
  color: hovered ? tint : wash.theme.alpha(tint, 0)
  antialiasing: true

  Behavior on color { ColorAnimation { duration: wash.theme.durationFast } }
}
