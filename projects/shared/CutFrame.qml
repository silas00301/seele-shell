import QtQuick
import QtQuick.Shapes

// A panel's material with two corners cut instead of rounded, for a theme
// that gives `panelCut`: the top left and the bottom right are taken off at
// 45 degrees, and the panel's edge runs along the cuts. PanelSurface lays it
// under everything the panel holds, in place of its rounded rectangle.
//
// A theme that also gives `panelTrim` lights the top cut in `panelTrimColor`
// and runs a bar `strokeStrong` thick along the top edge from it. Past the
// bar's end, `panelGlitch` draws a slice in `panelGlitchColor` knocked down
// off the edge, out of register with it.
//
// Nothing is drawn outside the frame, whatever its size: the cut takes at most
// a third of the shorter side, the lit band at most the cut, the trim and the
// glitch only what is left of the top edge, and a glitch with no room is not
// drawn at all.
//
// A theme may also make the trim move (`durationPanelTrim`,
// `durationPanelGlitch`): as the panel appears the trim sweeps out from the
// cut and the glitch flickers after it, and the glitch flickers again when the
// pointer comes onto the panel. Both start only on those events and stop where
// the still frame draws them, so a panel that is open and left alone costs no
// frame, and a user who holds the theme still (`motionEnabled`) sees exactly
// the still frame.
Item {
  id: frame

  required property var theme
  // Whether the pointer is over the panel, which sets the glitch flickering.
  property bool hovered: false
  readonly property real cut: Math.min(frame.theme.panelCut, Math.min(frame.width, frame.height) / 3)
  // The edge is stroked on the pixel grid, half a hairline inside the bounds.
  readonly property real edge: frame.theme.hairline / 2
  // The lit band along the cut is as wide, along each edge, as two trims.
  readonly property real lit: Math.min(frame.theme.strokeStrong * 2, frame.cut)
  readonly property bool trimmed: frame.theme.panelTrim > 0
  // The trim's whole length and where the glitch rests, inside the frame.
  readonly property real trimLength: Math.max(0, Math.min(frame.theme.panelTrim, frame.width - frame.cut - frame.lit))
  readonly property real glitchX: frame.cut + frame.lit + frame.trimLength + frame.theme.spaceMedium
  readonly property real glitchLength: Math.max(0, Math.min(frame.theme.panelGlitch, frame.width - frame.glitchX))
  readonly property bool glitched: frame.trimmed && frame.glitchLength > 0
    && Math.min(frame.theme.strokeStrong, frame.height) + frame.theme.strokeStrong <= frame.height

  // Motion. While `sweeping`, `drawn` is how much of the trim is drawn; `jolt`
  // is how far the glitch is knocked sideways and `glitchShown` whether it is
  // there at all. At rest the trim is its whole length and the glitch where
  // the still frame draws it.
  readonly property bool moving: frame.theme.motionEnabled && frame.trimmed
  readonly property bool sweeps: frame.moving && frame.theme.durationPanelTrim > 0
  readonly property bool flickers: frame.moving && frame.glitched && frame.theme.durationPanelGlitch > 0
  property bool sweeping: false
  property real drawn: 0
  property real jolt: 0
  property bool glitchShown: true
  // One flicker is three beats: knocked one way, gone, knocked the other.
  readonly property int beat: Math.round(frame.theme.durationPanelGlitch / (Math.max(1, frame.theme.panelGlitchFlickers) * 3))
  // Whether the panel is on screen: its window shown and the frame itself
  // visible. A panel that closes is hidden and shown again rather than built
  // anew, either as its window or, where the window stays mapped, as its
  // surface inside it.
  readonly property var host: frame.Window.window
  readonly property bool shown: frame.visible && frame.host !== null && frame.host.visible

  function rest() {
    arrival.stop()
    flicker.stop()
    frame.sweeping = false
    frame.jolt = 0
    frame.glitchShown = true
  }
  // The panel has just appeared.
  function arrive() {
    if (frame.sweeps) arrival.restart()
    else if (frame.flickers) flicker.restart()
  }
  // The trim is drawn whole: the glitch follows it.
  function swept() {
    frame.sweeping = false
    if (frame.flickers) flicker.restart()
    else frame.glitchShown = true
  }
  function flick() {
    if (frame.shown && frame.flickers && !arrival.running) flicker.restart()
  }

  onMovingChanged: if (!frame.moving) frame.rest()
  onHoveredChanged: if (frame.hovered) frame.flick()
  onShownChanged: {
    if (frame.shown) frame.arrive()
    else frame.rest()
  }
  Component.onCompleted: if (frame.shown) frame.arrive()

  SequentialAnimation {
    id: arrival
    objectName: "arrival"
    PropertyAction { target: frame; property: "glitchShown"; value: false }
    PropertyAction { target: frame; property: "drawn"; value: 0 }
    PropertyAction { target: frame; property: "sweeping"; value: true }
    NumberAnimation {
      target: frame
      property: "drawn"
      from: 0
      to: frame.trimLength
      duration: frame.theme.durationPanelTrim
      easing.type: frame.theme.easingStandard
      easing.bezierCurve: frame.theme.easingStandardCurve
    }
    ScriptAction { script: frame.swept() }
  }

  SequentialAnimation {
    id: flicker
    objectName: "flicker"
    SequentialAnimation {
      loops: Math.max(1, frame.theme.panelGlitchFlickers)
      PropertyAction { target: frame; property: "jolt"; value: frame.theme.panelGlitchShift }
      PropertyAction { target: frame; property: "glitchShown"; value: true }
      PauseAnimation { duration: frame.beat }
      PropertyAction { target: frame; property: "glitchShown"; value: false }
      PauseAnimation { duration: frame.beat }
      PropertyAction { target: frame; property: "jolt"; value: -frame.theme.panelGlitchShift }
      PropertyAction { target: frame; property: "glitchShown"; value: true }
      PauseAnimation { duration: frame.beat }
    }
    PropertyAction { target: frame; property: "jolt"; value: 0 }
    PropertyAction { target: frame; property: "glitchShown"; value: true }
  }

  anchors.fill: parent

  Shape {
    anchors.fill: parent
    preferredRendererType: Shape.CurveRenderer

    ShapePath {
      fillColor: frame.theme.panelColor
      strokeColor: frame.theme.panelBorder
      strokeWidth: frame.theme.hairline
      joinStyle: ShapePath.MiterJoin
      startX: frame.cut + frame.edge
      startY: frame.edge
      PathLine { x: frame.width - frame.edge; y: frame.edge }
      PathLine { x: frame.width - frame.edge; y: frame.height - frame.cut - frame.edge }
      PathLine { x: frame.width - frame.cut - frame.edge; y: frame.height - frame.edge }
      PathLine { x: frame.edge; y: frame.height - frame.edge }
      PathLine { x: frame.edge; y: frame.cut + frame.edge }
      PathLine { x: frame.cut + frame.edge; y: frame.edge }
    }

    // The top cut, lit.
    ShapePath {
      fillColor: frame.trimmed ? frame.theme.panelTrimColor : "transparent"
      strokeColor: "transparent"
      startX: frame.cut
      startY: 0
      PathLine { x: frame.cut + frame.lit; y: 0 }
      PathLine { x: 0; y: frame.cut + frame.lit }
      PathLine { x: 0; y: frame.cut }
      PathLine { x: frame.cut; y: 0 }
    }
  }

  Rectangle {
    id: trim
    objectName: "trim"
    visible: frame.trimmed && width > 0
    x: frame.cut + frame.lit
    width: frame.sweeping ? frame.drawn : frame.trimLength
    height: Math.min(frame.theme.strokeStrong, frame.height)
    color: frame.theme.panelTrimColor
  }

  // Knocked sideways, the glitch still stays inside the frame.
  Rectangle {
    id: glitch
    objectName: "glitch"
    visible: frame.glitched && frame.glitchShown
    x: Math.max(0, Math.min(frame.glitchX + frame.jolt, frame.width - width))
    y: trim.height
    width: frame.glitchLength
    height: frame.theme.strokeStrong
    color: frame.theme.panelGlitchColor
  }
}
