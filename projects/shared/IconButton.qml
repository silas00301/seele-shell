import QtQuick

// A square button carrying a mark rather than a label. The pointer is reported
// by a wash laid over whatever the button's state already put down, so the
// button that is on is still the one that lights under the cursor. A press is
// the press colour laid flat, or a ripple from `pressX`, `pressY` for a theme
// that grows them, and closes the corners to `radiusPressed` while held.
Rectangle {
  id: iconButton

  required property var theme
  property bool active: false
  property bool hovered: false
  property bool pressed: false
  property real pressX: width / 2
  property real pressY: height / 2
  property color tint: iconButton.theme.accent
  property color pressTint: iconButton.theme.pressColor
  property color hoverTint: iconButton.theme.hoverColor
  property real restingRadius: iconButton.theme.radius

  implicitWidth: iconButton.theme.controlHeight
  implicitHeight: iconButton.theme.controlHeight
  radius: iconButton.pressed ? Math.min(iconButton.restingRadius, iconButton.theme.radiusPressed) : iconButton.restingRadius
  color: iconButton.pressed && iconButton.theme.durationRipple === 0
    ? iconButton.pressTint
    : iconButton.active
      ? iconButton.theme.alpha(iconButton.tint, iconButton.theme.statusFillOpacity)
      : iconButton.theme.alpha(iconButton.tint, 0)
  antialiasing: true

  Behavior on color { ColorAnimation { duration: iconButton.theme.durationEffects; easing.type: iconButton.theme.easingEffects; easing.bezierCurve: iconButton.theme.easingEffectsCurve } }
  Behavior on radius { NumberAnimation { duration: iconButton.theme.durationSpatial; easing.type: iconButton.theme.easingSpatial; easing.bezierCurve: iconButton.theme.easingSpatialCurve } }

  HoverWash {
    theme: iconButton.theme
    hovered: iconButton.hovered
    tint: iconButton.hoverTint
    pressed: iconButton.pressed
    pressX: iconButton.pressX
    pressY: iconButton.pressY
    pressTint: iconButton.pressTint
  }
}
