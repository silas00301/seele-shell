import QtQuick
import QtQuick.Controls

// Material's plain tooltip: a short label on the inverse surface, on the
// extra-small corner, so it reads as a note about the control rather than as
// another panel.
ToolTip {
  id: tip

  required property var theme

  delay: 0
  font.family: tip.theme.fontFamily
  font.pixelSize: tip.theme.textCaption
  padding: tip.theme.spaceMedium
  topPadding: tip.theme.spaceTight
  bottomPadding: tip.theme.spaceTight
  contentItem: Text {
    text: tip.text
    textFormat: Text.PlainText
    color: tip.theme.inverseOnSurface
    font: tip.font
    verticalAlignment: Text.AlignVCenter
  }
  background: Rectangle {
    implicitHeight: tip.theme.tooltipHeight
    radius: tip.theme.shapeExtraSmall
    color: tip.theme.inverseSurface
    antialiasing: true
  }
}
