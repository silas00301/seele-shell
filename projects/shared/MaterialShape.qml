import QtQuick
import "Shapes.js" as Shapes

// One of Material 3 Expressive's shapes, filled in a colour. A mark that
// leads a panel or an empty state sits in one of these rather than in a
// rounded square, and a shape that changes with its state morphs: set
// `shape` and the outline flows into the new one on the fast spatial spring.
Canvas {
  id: materialShape

  required property var theme
  property string shape: "cookie9"
  property color color: materialShape.theme.primaryContainer
  property real rotationAngle: 0
  property string shown: materialShape.shape
  property string previous: materialShape.shape
  property real morph: 1

  antialiasing: true
  onShapeChanged: {
    materialShape.previous = materialShape.shown
    materialShape.shown = materialShape.shape
    morphAnimation.restart()
  }
  onMorphChanged: requestPaint()
  onColorChanged: requestPaint()
  onRotationAngleChanged: requestPaint()
  onWidthChanged: requestPaint()
  onHeightChanged: requestPaint()
  onPaint: {
    var context = getContext("2d")
    context.reset()
    Shapes.fill(context, Shapes.blend(materialShape.previous, materialShape.shown, materialShape.morph),
      width, height, materialShape.rotationAngle, materialShape.color)
  }

  NumberAnimation {
    id: morphAnimation

    target: materialShape
    property: "morph"
    from: 0
    to: 1
    duration: materialShape.theme.durationFastSpatial
    easing.type: Easing.BezierSpline
    easing.bezierCurve: materialShape.theme.springFastSpatial
  }
}
