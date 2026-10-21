import QtQuick
import QtQuick.Shapes

// A press ripple, as Material draws one: a soft-edged disc that grows from
// where the press landed toward the control's centre until it covers the
// control, and fades once the press is let go. It is laid over a control by
// HoverWash, only for a theme that gives `durationRipple`, and drawn in the
// control's own corners, so nothing spills past a rounded edge.
//
// The press fades in over `durationRippleIn`. Let go early and the ripple
// still holds until `durationRippleHold` from the press, so a click is seen;
// then it fades over `durationRippleOut` while it keeps growing.
Item {
  id: ripple

  required property var theme
  property bool pressed: false
  // Where the press landed, in this item's coordinates. A press without a
  // point, such as a key's, grows from the centre.
  property real pressX: width / 2
  property real pressY: height / 2
  property color tint: ripple.theme.pressColor
  property real radius: 0

  anchors.fill: parent

  property real centreX: 0
  property real centreY: 0
  property real reach: 0
  property real strength: 0
  // Let go before the hold ran out: fade once it does.
  property bool letGo: false

  function press() {
    var longer = Math.max(ripple.width, ripple.height)
    var softEdge = Math.max(ripple.theme.rippleSoftEdgeRatio * longer, ripple.theme.rippleSoftEdge)
    grow.stop()
    fade.stop()
    growX.from = ripple.pressX
    growX.to = ripple.width / 2
    growY.from = ripple.pressY
    growY.to = ripple.height / 2
    growReach.from = Math.floor(longer * ripple.theme.rippleOriginScale) / 2
    growReach.to = (Math.sqrt(ripple.width * ripple.width + ripple.height * ripple.height) + ripple.theme.ripplePadding + softEdge) / 2
    grow.start()
    fade.to = 1
    fade.duration = ripple.theme.durationRippleIn
    fade.start()
    ripple.letGo = false
    hold.restart()
  }

  function release() {
    if (hold.running) {
      ripple.letGo = true
      return
    }
    fade.stop()
    fade.to = 0
    fade.duration = ripple.theme.durationRippleOut
    fade.start()
  }

  onPressedChanged: ripple.pressed ? ripple.press() : ripple.release()

  Timer {
    id: hold
    interval: ripple.theme.durationRippleHold
    onTriggered: if (ripple.letGo && !ripple.pressed) ripple.release()
  }

  ParallelAnimation {
    id: grow
    NumberAnimation { id: growX; target: ripple; property: "centreX"; duration: ripple.theme.durationRipple; easing.type: ripple.theme.easingStandard; easing.bezierCurve: ripple.theme.easingStandardCurve }
    NumberAnimation { id: growY; target: ripple; property: "centreY"; duration: ripple.theme.durationRipple; easing.type: ripple.theme.easingStandard; easing.bezierCurve: ripple.theme.easingStandardCurve }
    NumberAnimation { id: growReach; target: ripple; property: "reach"; duration: ripple.theme.durationRipple; easing.type: ripple.theme.easingStandard; easing.bezierCurve: ripple.theme.easingStandardCurve }
  }

  NumberAnimation { id: fade; target: ripple; property: "strength" }

  Shape {
    anchors.fill: parent
    visible: ripple.strength > 0
    opacity: ripple.strength
    preferredRendererType: Shape.CurveRenderer

    ShapePath {
      strokeColor: "transparent"
      fillGradient: RadialGradient {
        centerX: ripple.centreX
        centerY: ripple.centreY
        centerRadius: ripple.reach
        focalX: ripple.centreX
        focalY: ripple.centreY
        GradientStop { position: 0; color: ripple.tint }
        // Solid to within the feather of the rim, but never less than the core.
        GradientStop { position: Math.max(1 - ripple.theme.rippleFeather / Math.max(1, ripple.reach), ripple.theme.rippleCore); color: ripple.tint }
        GradientStop { position: 1; color: ripple.theme.alpha(ripple.tint, 0) }
      }
      PathRectangle {
        width: ripple.width
        height: ripple.height
        radius: ripple.radius
      }
    }
  }
}
