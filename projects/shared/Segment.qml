import QtQuick

// One choice inside a SegmentWell. Only the chosen segment carries a fill; the
// rest are clear, so the well reads as a single control with one lit part. A
// press is the press colour laid flat, or a ripple for a theme that grows them.
Rectangle {
  id: segment

  required property var theme
  property bool selected: false
  property bool hovered: false
  property bool pressed: false
  property real pressX: width / 2
  property real pressY: height / 2

  height: parent ? parent.height : segment.theme.chipHeight
  radius: segment.theme.radiusSmall
  color: segment.pressed && segment.theme.durationRipple === 0
    ? segment.theme.pressColor
    : segment.selected
      ? segment.theme.selectedColor
      : segment.theme.clearColor
  antialiasing: true

  Behavior on color { ColorAnimation { duration: segment.theme.durationEffects; easing.type: segment.theme.easingEffects; easing.bezierCurve: segment.theme.easingEffectsCurve } }

  HoverWash { theme: segment.theme; hovered: segment.hovered; pressed: segment.pressed; pressX: segment.pressX; pressY: segment.pressY }
}
