import QtQuick

Item {
  required property var theme
  id: surfaceEdge

  property real radius: surfaceEdge.theme.radius - surfaceEdge.theme.hairline

  anchors.fill: parent
  z: 1

  Rectangle {
    anchors.fill: parent
    anchors.margins: surfaceEdge.theme.hairline
    radius: surfaceEdge.radius
    color: "transparent"
    border.width: surfaceEdge.theme.hairline
    border.color: surfaceEdge.theme.edgeLight
    antialiasing: true
  }

  Rectangle {
    anchors.top: parent.top
    anchors.topMargin: surfaceEdge.theme.hairline
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.leftMargin: surfaceEdge.radius
    anchors.rightMargin: surfaceEdge.radius
    height: surfaceEdge.theme.hairline
    color: surfaceEdge.theme.edgeCrown
  }
}
