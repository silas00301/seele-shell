import QtQuick

Rectangle {
  required property var theme
  id: panelSurface

  readonly property bool hovered: panelHover.hovered

  anchors.fill: parent
  radius: panelSurface.theme.radius
  color: panelSurface.theme.panelColor
  border.color: panelSurface.theme.panelBorder
  border.width: 1
  antialiasing: true

  SurfaceWash { theme: panelSurface.theme; radius: panelSurface.theme.radius - 1 }
  SurfaceEdge { theme: panelSurface.theme;}
  SurfaceGrain { theme: panelSurface.theme; inset: 3 }
  HoverHandler { id: panelHover }
}
