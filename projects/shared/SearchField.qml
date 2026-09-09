import QtQuick
import QtQuick.Controls

// The one text input that filters a list. It is a well rather than a card,
// because what it holds is a query rather than content.
TextField {
  id: searchField

  required property var theme
  property string glyph: "󰍉"

  implicitHeight: searchField.theme.controlHeight
  placeholderText: "Search…"
  color: searchField.theme.text
  placeholderTextColor: searchField.theme.overlay
  selectionColor: searchField.theme.selectedColor
  selectedTextColor: searchField.theme.text
  font.family: searchField.theme.fontFamily
  font.pixelSize: searchField.theme.textBody
  leftPadding: searchField.theme.spaceLarge + searchField.theme.textBody
  rightPadding: searchField.theme.spaceMedium
  verticalAlignment: TextInput.AlignVCenter

  background: Rectangle {
    radius: searchField.theme.radius
    color: searchField.theme.wellColor
    border.width: 1
    border.color: searchField.activeFocus ? searchField.theme.accent : searchField.theme.cardBorder
    antialiasing: true

    Behavior on border.color { ColorAnimation { duration: searchField.theme.durationFast } }

    CenteredGlyph {
      anchors.left: parent.left
      anchors.leftMargin: searchField.theme.spaceSmall
      anchors.verticalCenter: parent.verticalCenter
      width: searchField.theme.textBody + searchField.theme.spaceTight
      height: parent.height
      text: searchField.glyph
      color: searchField.activeFocus ? searchField.theme.accent : searchField.theme.overlay
      font.family: searchField.theme.fontFamily
      font.pixelSize: searchField.theme.textStrong
    }
  }
}
