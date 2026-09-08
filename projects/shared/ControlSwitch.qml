import QtQuick

Rectangle {
  id: control
  required property var theme

  property bool checked: false
  property bool busy: false
  signal toggled()

  implicitWidth: 40
  implicitHeight: 22
  opacity: enabled ? 1 : 0.42
  radius: height / 2
  antialiasing: true
  // Off, the track is a well cut into the surface rather than a grey pill,
  // so an unset switch is quiet and a set one is the only lit thing in the
  // row.
  color: switchMouse.pressed ? control.theme.alpha(control.checked ? control.theme.accent : control.theme.text, 0.6) : control.checked ? control.theme.accent : control.theme.wellColor
  border.width: 1
  border.color: control.busy ? control.theme.accent
    : switchMouse.containsMouse ? control.theme.alpha(control.theme.accent, 0.55)
    : control.checked ? "transparent" : control.theme.edgeLight

  Behavior on color { ColorAnimation { duration: control.theme.durationFast } }
  Behavior on border.color { ColorAnimation { duration: control.theme.durationFast } }

  Rectangle {
    visible: !control.busy
    width: parent.height - 6
    height: width
    radius: width / 2
    y: 3
    x: control.checked ? control.width - width - 3 : 3
    color: control.checked ? control.theme.crust : control.theme.alpha(control.theme.text, 0.82)
    antialiasing: true

    Behavior on x { NumberAnimation { duration: control.theme.durationFast; easing.type: Easing.OutCubic } }
    Behavior on color { ColorAnimation { duration: control.theme.durationFast } }
  }

  RefreshGlyph {
    theme: control.theme
    visible: control.busy
    anchors.centerIn: parent
    width: 16
    height: 16
    spinning: visible
    color: control.checked ? control.theme.crust : control.theme.text
    font.pixelSize: control.theme.textBody
  }

  MouseArea { id: switchMouse; anchors.fill: parent; enabled: control.enabled && !control.busy; hoverEnabled: true; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: control.toggled() }
}
