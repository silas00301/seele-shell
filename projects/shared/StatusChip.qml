import QtQuick

// A short state named in its own tint: a priority, a severity, the stage a row
// has reached. It is a tinted plate on the shell's small radius rather than a
// pill, because pills are kept for switches, meters and status dots, and it
// takes no pointer because it is a readout rather than a control.
Rectangle {
  id: statusChip

  required property var theme
  property string text: ""
  property color tint: statusChip.theme.overlay

  implicitWidth: statusChipLabel.implicitWidth + statusChip.theme.spaceMedium * 2
  implicitHeight: statusChip.theme.chipHeight
  radius: statusChip.theme.radiusSmall
  color: statusChip.theme.alpha(statusChip.tint, 0.14)
  border.width: 1
  border.color: statusChip.theme.alpha(statusChip.tint, 0.24)
  antialiasing: true

  Behavior on color { ColorAnimation { duration: statusChip.theme.durationFast } }

  Text {
    id: statusChipLabel

    anchors.centerIn: parent
    text: statusChip.text
    textFormat: Text.PlainText
    color: statusChip.tint
    font.family: statusChip.theme.fontFamily
    font.pixelSize: statusChip.theme.textLabel
    font.weight: statusChip.theme.weightMedium
  }
}
