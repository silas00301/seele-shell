import QtQuick

// Material's state layer: the content's own colour laid over a control at the
// hover opacity, and at the pressed opacity while it is held. It is laid over
// whatever the control already says, so a selected or running control still
// answers the pointer. It takes its parent's corners, each one separately, so
// it follows a segment whose inner and outer corners differ.
Rectangle {
  id: wash
  required property var theme
  property bool hovered: false
  property bool pressed: false
  property color tint: wash.theme.hoverColor
  readonly property Rectangle frame: wash.parent as Rectangle

  anchors.fill: parent
  radius: wash.frame ? wash.frame.radius : wash.theme.radius
  topLeftRadius: wash.frame ? wash.frame.topLeftRadius : wash.radius
  topRightRadius: wash.frame ? wash.frame.topRightRadius : wash.radius
  bottomLeftRadius: wash.frame ? wash.frame.bottomLeftRadius : wash.radius
  bottomRightRadius: wash.frame ? wash.frame.bottomRightRadius : wash.radius
  color: wash.pressed ? wash.theme.alpha(wash.tint, Math.min(1, wash.tint.a * wash.theme.statePressed / wash.theme.stateHover))
    : wash.hovered ? wash.tint
    : wash.theme.alpha(wash.tint, 0)
  antialiasing: true

  Behavior on color { ColorAnimation { duration: wash.theme.durationFast } }
}
