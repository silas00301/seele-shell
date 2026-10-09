import QtQuick

// The container every floating panel is drawn on: a solid step of the surface
// ramp on the extra-large corner, edged with the outline variant's hairline
// because a layer surface casts no shadow to lift it off the window below.
Rectangle {
  required property var theme
  id: panelSurface

  readonly property bool hovered: panelHover.hovered

  anchors.fill: parent
  radius: panelSurface.theme.radiusPanel
  color: panelSurface.theme.panelColor
  border.color: panelSurface.theme.panelBorder
  border.width: panelSurface.theme.hairline
  antialiasing: true

  HoverHandler { id: panelHover }
}
