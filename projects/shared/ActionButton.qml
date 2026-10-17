import QtQuick
import QtQuick.Controls

Button {
  id: button
  required property var theme
  property bool selected: false
  property bool danger: false
  implicitHeight: theme.controlHeight
  implicitWidth: Math.max(theme.controlHeight, label.implicitWidth + theme.spaceLarge * 2)
  opacity: enabled ? 1 : theme.disabledOpacity
  hoverEnabled: true
  // A button an application offers as its way out of a state has to be
  // reachable without the pointer. The focus ring reports where Tab has
  // landed -- the background's own edge, where the theme lays the ring inside
  // it -- and Enter answers it the way Space does.
  focusPolicy: Qt.StrongFocus
  Keys.onPressed: event => {
    if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
    if (!event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) button.clicked()
    event.accepted = true
  }
  contentItem: Text {
    id: label
    text: button.text
    textFormat: Text.PlainText
    color: button.danger ? button.theme.red : button.selected ? button.theme.accent : button.theme.text
    font.family: button.theme.fontFamily
    font.pixelSize: button.theme.textLabel
    font.weight: button.theme.weightMedium
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
  }
  background: Rectangle {
    readonly property color resting: button.danger ? button.theme.dangerTint : button.selected ? button.theme.selectedColor : button.theme.cardColor
    // A ring inside the edge is the edge itself; one outside it is a ring,
    // shown, as Material shows it, only where the keyboard put the focus.
    readonly property bool edgeRing: button.activeFocus && button.theme.focusRingGap < 0
    // A press closes the corners while it is held.
    radius: button.down ? Math.min(button.theme.radius, button.theme.radiusPressed) : button.theme.radius
    color: button.down && button.theme.durationRipple === 0 ? button.theme.pressColor : resting
    border.width: edgeRing ? button.theme.focusRingWidth : button.theme.hairline
    border.color: edgeRing ? button.theme.focusRingColor : button.theme.cardBorder
    Behavior on color { ColorAnimation { duration: button.theme.durationEffects; easing.type: button.theme.easingEffects; easing.bezierCurve: button.theme.easingEffectsCurve } }
    Behavior on radius { NumberAnimation { duration: button.theme.durationSpatial; easing.type: button.theme.easingSpatial; easing.bezierCurve: button.theme.easingSpatialCurve } }
    HoverWash { theme: button.theme; hovered: button.hovered && button.enabled; pressed: button.down; pressX: button.pressX; pressY: button.pressY }
    FocusRing { theme: button.theme; shown: button.visualFocus && button.theme.focusRingGap >= 0 }
  }
  HoverHandler { cursorShape: button.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
