import QtQuick

Item {
  required property var theme
  id: refreshGlyph

  property bool spinning: false
  property color color: refreshGlyph.theme.accent
  property alias font: idleRefresh.font
  onColorChanged: activitySpinner.requestPaint()

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

  Canvas {
    id: activitySpinner

    visible: refreshGlyph.spinning
    anchors.centerIn: parent
    width: Math.min(parent.width, parent.height, idleRefresh.font.pixelSize)
    height: width
    antialiasing: true
    transformOrigin: Item.Center
    onVisibleChanged: if (visible) requestPaint()
    onWidthChanged: requestPaint()
    onPaint: {
      var context = getContext("2d")
      context.clearRect(0, 0, width, height)
      context.beginPath()
      context.lineWidth = Math.max(1.5, width * 0.14)
      context.lineCap = "round"
      context.strokeStyle = refreshGlyph.color
      context.arc(width / 2, height / 2, Math.max(1, width / 2 - context.lineWidth), -Math.PI / 2, Math.PI)
      context.stroke()
    }

    NumberAnimation on rotation {
      from: 0
      to: 360
      duration: 720
      loops: Animation.Infinite
      running: activitySpinner.visible
    }
  }
}
