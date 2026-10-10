import QtQuick

// A short state named in its own tint: a priority, a severity, the stage a row
// has reached. It is Material's chip -- the small corner, a container toned
// from its own colour -- rather than a pill, because a pill is a control the
// pointer aims at, and it takes no pointer because it is a readout.
Rectangle {
  id: statusChip

  required property var theme
  property string text: ""
  property color tint: statusChip.theme.overlay

  implicitWidth: statusChipLabel.implicitWidth + statusChip.theme.spaceMedium * 2
  implicitHeight: statusChip.theme.chipHeight
  radius: statusChip.theme.radiusSmall
  color: Qt.tint(statusChip.theme.surfaceContainerHighest, statusChip.theme.alpha(statusChip.tint, 0.2))
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
