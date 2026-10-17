import QtQuick

// A control's state layer: the pointer's wash, and for a theme that grows
// ripples (`durationRipple`), the press growing from where it landed. A
// control that passes `pressed` lays its press colour flat itself when the
// theme grows none, so a press is reported once either way.
Rectangle {
  id: wash
  required property var theme
  property bool hovered: false
  property color tint: wash.theme.hoverColor
  property bool pressed: false
  // Where the press landed, in the wash's coordinates; a key's press, with no
  // point, grows from the centre.
  property real pressX: width / 2
  property real pressY: height / 2
  property color pressTint: wash.theme.pressColor

  anchors.fill: parent
  radius: (parent as Rectangle) ? (parent as Rectangle).radius : wash.theme.radius
  color: hovered ? tint : wash.theme.alpha(tint, 0)
  antialiasing: true

  Behavior on color { ColorAnimation { duration: wash.theme.durationState } }

  Loader {
    anchors.fill: parent
    active: wash.theme.durationRipple > 0
    sourceComponent: Ripple {
      theme: wash.theme
      pressed: wash.pressed
      pressX: wash.pressX
      pressY: wash.pressY
      tint: wash.pressTint
      radius: wash.radius
    }
  }
}
