import QtQuick

// One choice inside a SegmentWell. Only the chosen segment carries a fill; the
// rest are clear, so the well reads as a single control with one lit part.
Rectangle {
  id: segment

  required property var theme
  property bool selected: false
  property bool hovered: false
  property bool pressed: false

  height: parent ? parent.height : segment.theme.chipHeight
  radius: segment.theme.radiusSmall
  color: segment.pressed
    ? segment.theme.pressColor
    : segment.selected
      ? segment.theme.selectedColor
      : segment.theme.clearColor
  antialiasing: true

  Behavior on color { ColorAnimation { duration: segment.theme.durationFast } }

  HoverWash { theme: segment.theme; hovered: segment.hovered }
}
