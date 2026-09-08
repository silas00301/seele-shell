import QtQuick

// A set of exclusive choices drawn as one well with the chosen one lit inside
// it. Several outlined boxes side by side spend an outline on every
// alternative to say what the fill of the one that is on already says, and
// the well is what makes the group read as a single control.
Rectangle {
  id: segmentWell

  required property var theme
  default property alias content: segmentWellRow.data

  implicitHeight: segmentWell.theme.chipHeight
  radius: segmentWell.theme.radius
  color: segmentWell.theme.wellColor
  border.width: 1
  border.color: segmentWell.theme.alpha(segmentWell.theme.text, 0.05)
  antialiasing: true

  Row {
    id: segmentWellRow

    anchors.fill: parent
    anchors.margins: 2
  }
}
