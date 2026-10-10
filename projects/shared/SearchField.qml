import QtQuick
import QtQuick.Controls

// The one text input that filters a list, drawn as Material's search bar: a
// full pill on the highest surface step with its mark leading, because what
// it holds is a query rather than content. Focus is the primary colour's
// outline, the one thing that tells the reader typing will land here.
TextField {
  id: searchField

  required property var theme
  property string glyph: "󰍉"

  implicitHeight: searchField.theme.controlHeight
  placeholderText: "Search…"
  color: searchField.theme.text
  placeholderTextColor: searchField.theme.subtext
  selectionColor: searchField.theme.selectedColor
  selectedTextColor: searchField.theme.text
  font.family: searchField.theme.fontFamily
  font.pixelSize: searchField.theme.textBody
  leftPadding: searchField.theme.spaceLarge + searchField.theme.textBody + searchField.theme.spaceSmall
  rightPadding: searchField.theme.spaceLarge
  verticalAlignment: TextInput.AlignVCenter

  background: Rectangle {
    radius: height / 2
    color: searchField.theme.surfaceContainerHighest
    border.width: searchField.activeFocus ? searchField.theme.focusWidth : 0
    border.color: searchField.theme.primary
    antialiasing: true

    Behavior on border.width { NumberAnimation { duration: searchField.theme.durationFast } }

    CenteredGlyph {
      anchors.left: parent.left
      anchors.leftMargin: searchField.theme.spaceLarge
      anchors.verticalCenter: parent.verticalCenter
      width: searchField.theme.textBody + searchField.theme.spaceTight
      height: parent.height
      text: searchField.glyph
      color: searchField.activeFocus ? searchField.theme.primary : searchField.theme.subtext
      font.family: searchField.theme.fontFamily
      font.pixelSize: searchField.theme.textStrong
    }
  }
}
