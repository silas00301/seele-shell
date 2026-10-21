pragma ComponentBehavior: Bound
import QtQuick

Rectangle {
  required property var theme
  id: panelSurface

  readonly property bool hovered: panelHover.hovered
  // A theme that cuts a panel's corners (`panelCut`) draws its material as a
  // CutFrame instead, under everything the panel holds. A cut panel carries
  // no wash or lit edge, which would spill past the cuts; its trim stands in
  // for them, and its film stops short of the cuts. A surface that is not a
  // panel, such as a tooltip, passes `cutCorners: false` and keeps the plain
  // material, as the shell's own tips do.
  property bool cutCorners: true
  readonly property bool cut: panelSurface.cutCorners && panelSurface.theme.panelCut > 0

  anchors.fill: parent
  radius: panelSurface.theme.radius
  color: panelSurface.cut ? "transparent" : panelSurface.theme.panelColor
  border.color: panelSurface.theme.panelBorder
  border.width: panelSurface.cut ? 0 : panelSurface.theme.hairline
  antialiasing: true

  Loader {
    id: cutFrame
    anchors.fill: parent
    active: panelSurface.cut
    sourceComponent: CutFrame { theme: panelSurface.theme }
  }
  SurfaceWash { theme: panelSurface.theme; visible: !panelSurface.cut; radius: panelSurface.theme.radius - panelSurface.theme.hairline }
  SurfaceEdge { theme: panelSurface.theme; visible: !panelSurface.cut }
  SurfaceGrain { theme: panelSurface.theme; inset: Math.max(panelSurface.theme.grainInset, cutFrame.item ? (cutFrame.item as CutFrame).cut / 2 : 0) }
  HoverHandler { id: panelHover }
}
