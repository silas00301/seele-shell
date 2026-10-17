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
Item {
  id: frame

  required property var theme
  readonly property real cut: Math.min(frame.theme.panelCut, Math.min(frame.width, frame.height) / 3)
  // The edge is stroked on the pixel grid, half a hairline inside the bounds.
  readonly property real edge: frame.theme.hairline / 2
  // The lit band along the cut is as wide, along each edge, as two trims.
  readonly property real lit: Math.min(frame.theme.strokeStrong * 2, frame.cut)
  readonly property bool trimmed: frame.theme.panelTrim > 0

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
    visible: frame.trimmed && width > 0
    x: frame.cut + frame.lit
    width: Math.max(0, Math.min(frame.theme.panelTrim, frame.width - x))
    height: Math.min(frame.theme.strokeStrong, frame.height)
    color: frame.theme.panelTrimColor
  }

  Rectangle {
    id: glitch
    visible: frame.trimmed && width > 0 && y + height <= frame.height
    x: trim.x + trim.width + frame.theme.spaceMedium
    y: trim.height
    width: Math.max(0, Math.min(frame.theme.panelGlitch, frame.width - x))
    height: frame.theme.strokeStrong
    color: frame.theme.panelGlitchColor
  }
}
