import QtQuick

Rectangle {
  id: control
  required property var theme

  property bool checked: false
  property bool busy: false
  signal toggled()

  implicitWidth: control.theme.switchWidth
  implicitHeight: control.theme.switchHeight
  opacity: enabled ? 1 : control.theme.unavailableOpacity
  radius: height / 2
  antialiasing: true
  // Off, the track is a well cut into the surface rather than a grey pill,
  // so an unset switch is quiet and a set one is the only lit thing in the
  // row.
  color: switchMouse.pressed ? control.theme.alpha(control.checked ? control.theme.accent : control.theme.text, control.theme.switchPressedOpacity) : control.checked ? control.theme.accent : control.theme.wellColor
  border.width: control.theme.hairline
  border.color: control.busy ? control.theme.accent
    : switchMouse.containsMouse ? control.theme.alpha(control.theme.accent, control.theme.switchHoverOpacity)
    : control.checked ? control.theme.clearColor : control.theme.edgeLight

  Behavior on color { ColorAnimation { duration: control.theme.durationFast } }
  Behavior on border.color { ColorAnimation { duration: control.theme.durationFast } }

  Rectangle {
    visible: !control.busy
    width: parent.height - control.theme.switchKnobInset * 2
    height: width
    radius: width / 2
    y: control.theme.switchKnobInset
    x: control.checked ? control.width - width - control.theme.switchKnobInset : control.theme.switchKnobInset
    color: control.checked ? control.theme.crust : control.theme.alpha(control.theme.text, control.theme.switchKnobOpacity)
    antialiasing: true

    Behavior on x { NumberAnimation { duration: control.theme.durationFast; easing.type: control.theme.easingStandard; easing.bezierCurve: control.theme.easingStandardCurve } }
    Behavior on color { ColorAnimation { duration: control.theme.durationFast } }
  }

  RefreshGlyph {
    theme: control.theme
    visible: control.busy
    anchors.centerIn: parent
    width: control.theme.spinnerMedium
    height: control.theme.spinnerMedium
    spinning: visible
    color: control.checked ? control.theme.crust : control.theme.text
    font.pixelSize: control.theme.textBody
  }

  ActionArea { id: switchMouse; anchors.fill: parent; enabled: control.enabled && !control.busy; hoverEnabled: true; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onTriggered: control.toggled() }
}
