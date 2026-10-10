import QtQuick

// Asynchronous work in place. At rest it is the refresh glyph that asks for
// the work; while the work is live it is the shared loading indicator, so a
// control that is busy keeps its geometry and says so without a spinner of
// its own.
Item {
  required property var theme
  id: refreshGlyph

  property bool spinning: false
  property color color: refreshGlyph.theme.primary
  property alias font: idleRefresh.font

  Text {
    id: idleRefresh

    visible: !refreshGlyph.spinning
    anchors.fill: parent
    text: "󰑐"
    color: refreshGlyph.color
    font.family: refreshGlyph.theme.fontFamily
    font.pixelSize: refreshGlyph.theme.textSubhead
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
  }

  LoadingIndicator {
    visible: refreshGlyph.spinning
    anchors.centerIn: parent
    width: Math.min(parent.width, parent.height, idleRefresh.font.pixelSize * 1.15)
    height: width
    color: refreshGlyph.color
  }
}
