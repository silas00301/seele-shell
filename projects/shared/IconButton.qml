import QtQuick

// A square button carrying a mark rather than a label. The pointer is reported
// by a wash laid over whatever the button's state already put down, so the
// button that is on is still the one that lights under the cursor.
Rectangle {
  id: iconButton

  required property var theme
  property bool active: false
  property bool hovered: false
  property bool pressed: false
  property color tint: iconButton.theme.accent
  property color pressTint: iconButton.theme.pressColor
  property color hoverTint: iconButton.theme.hoverColor

  implicitWidth: iconButton.theme.controlHeight
  implicitHeight: iconButton.theme.controlHeight
  radius: iconButton.theme.radius
  color: iconButton.pressed
    ? iconButton.pressTint
    : iconButton.active
      ? iconButton.theme.alpha(iconButton.tint, 0.14)
      : iconButton.theme.alpha(iconButton.tint, 0)
  antialiasing: true

  Behavior on color { ColorAnimation { duration: iconButton.theme.durationFast } }

  HoverWash { theme: iconButton.theme; hovered: iconButton.hovered; tint: iconButton.hoverTint }
}
