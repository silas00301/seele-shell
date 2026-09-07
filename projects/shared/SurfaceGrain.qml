import QtQuick

Item {
  required property var theme
  id: grainLayer

  property real inset: 0

  anchors.fill: parent
  z: 1

  Image {
    anchors.fill: parent
    anchors.margins: grainLayer.inset
    source: grainLayer.theme.grain
    fillMode: Image.Tile
    opacity: grainLayer.theme.grainOpacity
    smooth: false
  }
}
