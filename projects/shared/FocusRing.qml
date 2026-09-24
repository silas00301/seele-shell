import QtQuick

// Where the keyboard is. A control that can be tabbed to and draws nothing for
// it is a control the keyboard cannot actually reach, so the ring is a part
// rather than a rectangle each surface remembers to add. It is laid over the
// control it belongs to and takes that control's own corner, so a picker and a
// transport button are ringed the same way. The ring rests on
// the accent at zero alpha rather than on `transparent`, because a surface may
// animate it and Qt drags a colour interpolated against black through grey.
Rectangle {
  id: focusRing

  required property var theme
  property bool shown: false

  anchors.fill: parent
  radius: (parent as Rectangle) ? (parent as Rectangle).radius : focusRing.theme.radius
  visible: focusRing.shown
  color: focusRing.theme.alpha(focusRing.theme.accent, 0)
  border.width: 1
  border.color: focusRing.theme.accent
  antialiasing: true
}
