import QtQuick

// Where the keyboard is. Material draws focus as an outline in the secondary
// colour held just clear of the control, so it never reads as the control's
// own border or selection. A control that can be tabbed to and draws nothing
// for it is a control the keyboard cannot actually reach, so the ring is a
// part rather than a rectangle each surface remembers to add. It takes the
// control's own corners, grown by the gap, and rests on the colour at zero
// alpha rather than on `transparent`, because Qt drags a colour interpolated
// against black through grey.
Rectangle {
  id: focusRing

  required property var theme
  property bool shown: false
  // Inside a parent that clips, the ring is drawn on the control's edge
  // rather than outside it.
  property real gap: focusRing.theme.focusGap
  readonly property Rectangle frame: focusRing.parent as Rectangle

  // The corner the control itself is drawn on, for a control that is not a
  // Rectangle the ring can read it from.
  property real baseRadius: focusRing.frame ? focusRing.frame.radius : focusRing.theme.radius

  anchors.fill: parent
  anchors.margins: -focusRing.gap
  radius: focusRing.baseRadius + focusRing.gap
  topLeftRadius: (focusRing.frame ? focusRing.frame.topLeftRadius : focusRing.baseRadius) + focusRing.gap
  topRightRadius: (focusRing.frame ? focusRing.frame.topRightRadius : focusRing.baseRadius) + focusRing.gap
  bottomLeftRadius: (focusRing.frame ? focusRing.frame.bottomLeftRadius : focusRing.baseRadius) + focusRing.gap
  bottomRightRadius: (focusRing.frame ? focusRing.frame.bottomRightRadius : focusRing.baseRadius) + focusRing.gap
  visible: focusRing.shown
  color: focusRing.theme.alpha(focusRing.theme.secondary, 0)
  border.width: focusRing.theme.focusWidth
  border.color: focusRing.theme.secondary
  antialiasing: true
}
