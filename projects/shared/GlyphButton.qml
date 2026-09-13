import QtQuick
import QtQuick.Controls

// A keyboard-accessible action on the shared IconButton material. Keep the
// glyph, its accessible name and its tooltip together in every host.
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
    color: button.selected ? button.theme.accent : button.theme.subtext
    font.family: button.theme.fontFamily
    font.pixelSize: button.theme.textIcon
  }
  background: IconButton {
    theme: button.theme
    active: button.selected || button.activeFocus
    hovered: button.hovered && button.enabled
    pressed: button.down
  }
  HoverHandler { cursorShape: button.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
  ToolTip {
    id: tip
    visible: button.hovered && button.text !== ""
    delay: button.theme.durationNormal
    text: button.text
    font.family: button.theme.fontFamily
    font.pixelSize: button.theme.textCaption
    padding: button.theme.spaceMedium
    contentItem: Text {
      text: button.text
      textFormat: Text.PlainText
      color: button.theme.text
      font: tip.font
    }
    background: PanelSurface { theme: button.theme }
  }
}
