import QtQuick

Item {
  required property var theme
  id: surfaceEdge

  property real radius: surfaceEdge.theme.radius - 1

  anchors.fill: parent
  z: 1

  Rectangle {
    anchors.fill: parent
    anchors.margins: 1
    radius: surfaceEdge.radius
    color: "transparent"
    border.width: 1
    border.color: surfaceEdge.theme.edgeLight
    antialiasing: true
  }

  Rectangle {
    anchors.top: parent.top
    anchors.topMargin: 1
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.leftMargin: surfaceEdge.radius
    anchors.rightMargin: surfaceEdge.radius
    height: 1
    color: surfaceEdge.theme.edgeCrown
  }
}
