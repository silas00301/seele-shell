import QtQuick

Rectangle {
  required property var theme
  id: panelSurface

  readonly property bool hovered: panelHover.hovered

  anchors.fill: parent
  radius: panelSurface.theme.radius
  color: panelSurface.theme.panelColor
  border.color: panelSurface.theme.panelBorder
  border.width: panelSurface.theme.hairline
  antialiasing: true

  SurfaceWash { theme: panelSurface.theme; radius: panelSurface.theme.radius - panelSurface.theme.hairline }
  SurfaceEdge { theme: panelSurface.theme;}
  SurfaceGrain { theme: panelSurface.theme; inset: panelSurface.theme.grainInset }
  HoverHandler { id: panelHover }
}
