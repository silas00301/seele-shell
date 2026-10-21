import QtQuick

Rectangle {
  id: control
  required property var theme

  property bool checked: false
  property bool busy: false
  signal toggled()

  // A theme may centre a round state layer on the handle; the pointer and the
  // press are then reported there, and the track keeps its colour.
  readonly property bool stateLayer: control.theme.switchStateLayerSize > 0
  readonly property bool active: switchMouse.containsMouse || switchMouse.pressed

  implicitWidth: control.theme.switchWidth
  implicitHeight: control.theme.switchHeight
  opacity: enabled ? 1 : control.theme.unavailableOpacity
  radius: height / 2
  antialiasing: true
  // Off, the track is a well cut into the surface rather than a grey pill,
  // so an unset switch is quiet and a set one is the only lit thing in the
  // row.
  color: switchMouse.pressed && !control.stateLayer ? control.theme.alpha(control.checked ? control.theme.accent : control.theme.text, control.theme.switchPressedOpacity) : control.checked ? control.theme.accent : control.theme.wellColor
  border.width: control.theme.hairline
  border.color: control.busy ? control.theme.accent
    : switchMouse.containsMouse && !control.stateLayer ? control.theme.alpha(control.theme.accent, control.theme.switchHoverOpacity)
    : control.checked ? control.theme.clearColor : control.theme.edgeLight

  Behavior on color { ColorAnimation { duration: control.theme.durationEffects; easing.type: control.theme.easingEffects; easing.bezierCurve: control.theme.easingEffectsCurve } }
  Behavior on border.color { ColorAnimation { duration: control.theme.durationEffects; easing.type: control.theme.easingEffects; easing.bezierCurve: control.theme.easingEffectsCurve } }

  // The handle rides between the centres of the track's two round ends, and
  // grows as it is set and again while it is held.
  property real handleCentre: control.checked ? control.width - control.height / 2 : control.height / 2
  Behavior on handleCentre { NumberAnimation { duration: control.theme.durationSpatial; easing.type: control.theme.easingSpatial; easing.bezierCurve: control.theme.easingSpatialCurve } }

  Loader {
    active: control.stateLayer && !control.busy
    width: control.theme.switchStateLayerSize
    height: control.theme.switchStateLayerSize
    x: control.handleCentre - width / 2
    y: (control.height - height) / 2
    sourceComponent: Rectangle {
      radius: width / 2
      color: control.theme.alpha(control.theme.hoverColor, 0)
      HoverWash {
        theme: control.theme
        hovered: switchMouse.containsMouse
        tint: control.checked ? control.theme.selectedHoverColor : control.theme.hoverColor
        pressed: switchMouse.pressed
      }
    }
  }

  Rectangle {
    visible: !control.busy
    width: switchMouse.pressed ? control.theme.switchHandlePressedSize : control.checked ? control.theme.switchHandleOnSize : control.theme.switchHandleSize
    height: width
    radius: width / 2
    x: control.handleCentre - width / 2
    y: (control.height - height) / 2
    color: control.checked
      ? (control.active ? control.theme.switchHandleOnActive : control.theme.switchHandleOn)
      : (control.active ? control.theme.switchHandleOffActive : control.theme.switchHandleOff)
    antialiasing: true

    Behavior on width { NumberAnimation { duration: control.theme.durationSpatial; easing.type: control.theme.easingSpatial; easing.bezierCurve: control.theme.easingSpatialCurve } }
    Behavior on color { ColorAnimation { duration: control.theme.durationEffects; easing.type: control.theme.easingEffects; easing.bezierCurve: control.theme.easingEffectsCurve } }
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
