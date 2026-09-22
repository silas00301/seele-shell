import QtQuick
import QtQuick.Controls

// A plain editable value on the shared well material. Unlike SearchField it
// carries no search mark: the placeholder and accessible name identify it.
TextField {
  id: field
  required property var theme
  property color borderColor: field.activeFocus ? field.theme.accent : field.theme.cardBorder
  implicitHeight: theme.controlHeight
  color: theme.text
  placeholderTextColor: theme.overlay
  selectionColor: theme.selectedColor
  selectedTextColor: theme.text
  font.family: theme.fontFamily
  font.pixelSize: theme.textBody
  leftPadding: theme.spaceMedium
  rightPadding: theme.spaceMedium
  verticalAlignment: TextInput.AlignVCenter
  background: Rectangle {
    radius: field.theme.radius
    color: field.theme.wellColor
    border.width: field.theme.hairline
    border.color: field.borderColor
    antialiasing: true
    Behavior on border.color { ColorAnimation { duration: field.theme.durationFast } }
  }
}
