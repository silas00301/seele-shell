import QtQuick

Rectangle {
  required property var theme
  id: cardEdge

  anchors.fill: parent
  radius: cardEdge.theme.radius
  color: "transparent"
  border.width: 1
  border.color: cardEdge.theme.cardBorder
  antialiasing: true

  Rectangle {
    anchors.top: parent.top
    anchors.topMargin: 1
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.leftMargin: cardEdge.radius
    anchors.rightMargin: cardEdge.radius
    height: 1
    color: cardEdge.theme.edgeLight
  }
}
