import QtQuick
import QtQuick.Controls

// A keyboard-accessible icon action on the shared IconButton container. Keep
// the glyph, its accessible name and its tooltip together in every host.
Button {
  id: button
  required property var theme
  required property string glyph
  property bool selected: false
  implicitWidth: theme.controlHeight
  implicitHeight: theme.controlHeight
  hoverEnabled: true
  focusPolicy: Qt.StrongFocus
  Accessible.name: text
  Keys.onPressed: event => {
    if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
    if (!event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) button.clicked()
    event.accepted = true
  }
  opacity: enabled ? 1 : theme.disabledOpacity
  contentItem: CenteredGlyph {
    text: button.glyph
    color: button.selected ? button.theme.textOnSecondaryContainer : button.theme.subtext
    font.family: button.theme.fontFamily
    font.pixelSize: button.theme.textIcon
  }
  background: IconButton {
    theme: button.theme
    active: button.selected
    hovered: button.hovered && button.enabled
    pressed: button.down
    // Keyboard focus is a ring, never the selected fill, so a focused action
    // cannot be mistaken for one that is on.
    FocusRing { theme: button.theme; shown: button.visualFocus }
  }
  HoverHandler { cursorShape: button.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
  PlainTooltip {
    theme: button.theme
    visible: button.hovered && button.text !== ""
    text: button.text
  }
}
