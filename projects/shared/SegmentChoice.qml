import QtQuick
import QtQuick.Controls

// A keyboard-accessible choice inside a SegmentWell. Segment owns the selected
// and pointer treatment; FocusRing makes keyboard position explicit without
// turning the segment itself into a one-off outlined control.
Button {
  id: choice

  required property var theme
  property bool selected: false

  hoverEnabled: true
  focusPolicy: Qt.StrongFocus
  Keys.onPressed: event => {
    if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
    if (!event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) choice.clicked()
    event.accepted = true
  }

  contentItem: Label {
    text: choice.text
    font.family: choice.theme.fontFamily
    font.pixelSize: choice.theme.textLabel
    font.weight: choice.selected ? choice.theme.weightStrong : choice.theme.weightMedium
    color: choice.selected ? choice.theme.text : choice.theme.subtext
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
  }

  background: Item {
    Segment {
      anchors.fill: parent
      theme: choice.theme
      selected: choice.selected
      hovered: choice.hovered
      pressed: choice.down
    }
    FocusRing {
      theme: choice.theme
      shown: choice.visualFocus
      radius: choice.theme.radiusSmall
    }
  }

  HoverHandler { cursorShape: choice.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
