import QtQuick
import "Motion.js" as Motion
import "Shapes.js" as Shapes

// Material 3 Expressive's loading indicator: one shape flowing into the next
// on the fast spatial spring while the whole of it turns, so work in progress
// reads as alive rather than as a spinner. It needs no theme, because the
// lock, the greeter and the polkit agent draw it too.
Canvas {
  id: indicator

  property color color: "white"
  property bool running: visible
  // Material steps through seven shapes, each morph turning the indicator on
  // by a further quarter while the whole of it rotates slowly underneath.
  readonly property int morphInterval: 650
  readonly property real morphSettle: Motion.springSettle(Motion.fastSpatialStiffness, Motion.fastSpatialDamping)
  property real phase: 0

  antialiasing: true
  onColorChanged: requestPaint()
  onWidthChanged: requestPaint()
  onVisibleChanged: if (visible) requestPaint()
  onPhaseChanged: requestPaint()
  onPaint: {
    var context = getContext("2d")
    context.reset()
    var sequence = Shapes.loadingSequence
    var step = Math.floor(indicator.phase) % sequence.length
    var local = indicator.phase - Math.floor(indicator.phase)
    // The morph runs on the spring over the first part of each interval and
    // the shape holds for the rest, the way Material's indicator does.
    var progress = Math.min(1, local * indicator.morphInterval / 1000 / indicator.morphSettle)
    var eased = Motion.springPosition(Motion.fastSpatialStiffness, Motion.fastSpatialDamping, progress * indicator.morphSettle)
    var shape = Shapes.blend(sequence[step], sequence[(step + 1) % sequence.length], eased)
    var rotation = indicator.phase * 360 / sequence.length * 2 + (Math.floor(indicator.phase) + eased) * 90
    Shapes.fill(context, shape, width, height, rotation, indicator.color)
  }

  NumberAnimation on phase {
    from: 0
    to: Shapes.loadingSequence.length
    duration: indicator.morphInterval * Shapes.loadingSequence.length
    loops: Animation.Infinite
    running: indicator.running && indicator.visible
  }
}
