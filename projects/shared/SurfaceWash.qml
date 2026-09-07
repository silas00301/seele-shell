import QtQuick

Rectangle {
  id: surfaceWash
  required property var theme
  anchors.fill: parent
  anchors.margins: 1
  color: "transparent"

  gradient: Gradient {
    GradientStop { position: 0.0; color: surfaceWash.theme.alpha(surfaceWash.theme.text, 0.075) }
    GradientStop { position: 0.28; color: surfaceWash.theme.alpha(surfaceWash.theme.text, 0.02) }
    GradientStop { position: 0.6; color: "transparent" }
    GradientStop { position: 1.0; color: surfaceWash.theme.alpha(surfaceWash.theme.crust, 0.5) }
  }
}
