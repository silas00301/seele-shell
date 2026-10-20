import QtQuick

Rectangle {
  id: surfaceWash
  required property var theme
  anchors.fill: parent
  anchors.margins: surfaceWash.theme.hairline
  color: "transparent"

  gradient: Gradient {
    GradientStop { position: 0.0; color: surfaceWash.theme.alpha(surfaceWash.theme.text, surfaceWash.theme.washCrownOpacity) }
    GradientStop { position: surfaceWash.theme.washFadeStop; color: surfaceWash.theme.alpha(surfaceWash.theme.text, surfaceWash.theme.washFadeOpacity) }
    GradientStop { position: surfaceWash.theme.washClearStop; color: "transparent" }
    GradientStop { position: 1.0; color: surfaceWash.theme.alpha(surfaceWash.theme.crust, surfaceWash.theme.washFootOpacity) }
  }
}
