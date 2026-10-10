import QtQuick

// The container behind an icon action, drawn as Material 3 Expressive's icon
// button: round at rest, squared towards the medium corner while it is on,
// and pinched to the small corner for as long as it is held, so a press is
// felt in the shape before the action lands. The pointer is reported by a
// state layer over whatever the button's state already put down, so the
// button that is on is still the one that answers the cursor.
Rectangle {
  id: iconButton

  required property var theme
  property bool active: false
  property bool hovered: false
  property bool pressed: false
  // An action whose state is not the accent's -- a mute that is on, a
  // recording that is running -- passes its own colour, and its container is
  // that colour's tone over the surface instead of the secondary container.
  property color tint: iconButton.theme.accent
  property color pressTint: iconButton.theme.clearColor
  property color hoverTint: iconButton.theme.hoverColor

  implicitWidth: iconButton.theme.controlHeight
  implicitHeight: iconButton.theme.controlHeight
  radius: iconButton.pressed ? iconButton.theme.shapeSmall
    : iconButton.active ? iconButton.theme.shapeMedium
    : Math.min(width, height) / 2
  color: iconButton.pressed && iconButton.pressTint.a > 0 ? iconButton.pressTint
    : !iconButton.active ? iconButton.theme.alpha(iconButton.tint, 0)
    : Qt.colorEqual(iconButton.tint, iconButton.theme.accent) ? iconButton.theme.secondaryContainer
    : Qt.tint(iconButton.theme.surfaceContainerHighest, iconButton.theme.alpha(iconButton.tint, 0.24))
  antialiasing: true

  Behavior on color { ColorAnimation { duration: iconButton.theme.durationFast } }
  Behavior on radius {
    NumberAnimation {
      duration: iconButton.theme.durationFastSpatial
      easing.type: Easing.BezierSpline
      easing.bezierCurve: iconButton.theme.springFastSpatial
    }
  }

  HoverWash { theme: iconButton.theme; hovered: iconButton.hovered; pressed: iconButton.pressed; tint: iconButton.hoverTint }
}
