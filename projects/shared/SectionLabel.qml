import QtQuick

Text {
  id: sectionLabel
  required property var theme
  color: sectionLabel.theme.overlay
  font.family: sectionLabel.theme.fontFamily
  font.pixelSize: sectionLabel.theme.textCaption
  font.weight: sectionLabel.theme.weightMedium
  font.letterSpacing: sectionLabel.theme.trackingLabel
}
