import QtQuick
import QtQuick.Controls

// A short editable value, drawn as Material's filled text field: the highest
// surface step with its top corners rounded and its bottom closed by an
// active indicator, a hairline at rest that thickens into the primary colour
// while the field has the keyboard. It carries no search mark: the
// placeholder and accessible name identify it. A field whose value is wrong
// passes `invalid`, and its indicator says so in the error colour.
TextField {
  id: field
  required property var theme
  property bool invalid: false
  readonly property color indicatorColor: field.invalid ? field.theme.error
    : field.activeFocus ? field.theme.primary
    : field.theme.subtext
  implicitHeight: theme.controlHeight
  color: theme.text
  placeholderTextColor: theme.subtext
  selectionColor: theme.selectedColor
  selectedTextColor: theme.text
  font.family: theme.fontFamily
  font.pixelSize: theme.textBody
  leftPadding: theme.spaceLarge
  rightPadding: theme.spaceLarge
  verticalAlignment: TextInput.AlignVCenter
  background: Rectangle {
    topLeftRadius: field.theme.shapeExtraSmall
    topRightRadius: field.theme.shapeExtraSmall
    bottomLeftRadius: 0
    bottomRightRadius: 0
    color: field.theme.surfaceContainerHighest
    antialiasing: true
    HoverWash { theme: field.theme; hovered: field.hovered && !field.activeFocus }
    Rectangle {
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      height: field.activeFocus || field.invalid ? field.theme.focusWidth : field.theme.hairline
      color: field.indicatorColor
      Behavior on color { ColorAnimation { duration: field.theme.durationFast } }
    }
  }
}
