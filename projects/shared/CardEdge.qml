import QtQuick

Rectangle {
  required property var theme
  id: cardEdge

  anchors.fill: parent
  radius: cardEdge.theme.radius
  color: "transparent"
  border.width: cardEdge.theme.hairline
  border.color: cardEdge.theme.cardBorder
  antialiasing: true

  Rectangle {
    anchors.top: parent.top
    anchors.topMargin: cardEdge.theme.hairline
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.leftMargin: cardEdge.radius
    anchors.rightMargin: cardEdge.radius
    height: cardEdge.theme.hairline
    color: cardEdge.theme.edgeLight
  }
}
