import QtQuick

// A persistent on/off state, drawn as Material 3's switch. Off, the track is
// the highest surface step inside an outline with a small handle in the
// outline's colour; on, the track fills with the primary colour and the
// handle grows and carries a check. Held, the handle grows again, so the
// press is felt before the state flips. Work that takes a moment shows the
// loading indicator in the handle rather than replacing the switch.
Rectangle {
  id: control
  required property var theme

  property bool checked: false
  property bool busy: false
  signal toggled()

  readonly property bool held: switchMouse.pressed
  readonly property real handleSize: control.held ? control.height - control.theme.switchInset
    : control.checked || control.busy ? control.height - control.theme.switchInset * 2
    : control.height / 2
  readonly property color handleColor: control.checked ? control.theme.textOnPrimary
    : control.held || switchMouse.containsMouse ? control.theme.subtext
    : control.theme.outline

  implicitWidth: control.theme.switchWidth
  implicitHeight: control.theme.switchHeight
  opacity: enabled ? 1 : control.theme.disabledOpacity
  radius: height / 2
  antialiasing: true
  color: control.checked ? control.theme.primary : control.theme.surfaceContainerHighest
  border.width: control.checked ? 0 : control.theme.switchOutline
  border.color: control.theme.outline

  Behavior on color { ColorAnimation { duration: control.theme.durationFast } }

  // The state layer is a disc around the handle, as Material draws it, so
  // the pointer is reported where the switch will move.
  Rectangle {
    width: control.height + control.theme.spaceMedium
    height: width
    radius: width / 2
    anchors.centerIn: switchHandle
    color: control.held ? control.theme.alpha(control.checked ? control.theme.primary : control.theme.text, control.theme.statePressed)
      : switchMouse.containsMouse ? control.theme.alpha(control.checked ? control.theme.primary : control.theme.text, control.theme.stateHover)
      : control.theme.alpha(control.theme.text, 0)
    antialiasing: true
    Behavior on color { ColorAnimation { duration: control.theme.durationFast } }
  }

  Rectangle {
    id: switchHandle

    width: control.handleSize
    height: width
    radius: width / 2
    anchors.verticalCenter: parent.verticalCenter
    x: control.checked ? control.width - control.height / 2 - width / 2 : control.height / 2 - width / 2
    color: control.handleColor
    antialiasing: true

    Behavior on x { NumberAnimation { duration: control.theme.durationFastSpatial; easing.type: Easing.BezierSpline; easing.bezierCurve: control.theme.springFastSpatial } }
    Behavior on width { NumberAnimation { duration: control.theme.durationFastSpatial; easing.type: Easing.BezierSpline; easing.bezierCurve: control.theme.springFastSpatial } }
    Behavior on color { ColorAnimation { duration: control.theme.durationFast } }

    CenteredGlyph {
      visible: control.checked && !control.busy
      anchors.fill: parent
      text: "󰄬"
      color: control.theme.textOnPrimaryContainer
      font.family: control.theme.fontFamily
      font.pixelSize: control.theme.textLabel
    }

    RefreshGlyph {
      theme: control.theme
      visible: control.busy
      anchors.fill: parent
      anchors.margins: control.theme.spaceTight / 2
      spinning: visible
      color: control.checked ? control.theme.primary : control.theme.surfaceContainerHighest
      font.pixelSize: control.theme.textBody
    }
  }

  MouseArea { id: switchMouse; anchors.fill: parent; enabled: control.enabled && !control.busy; hoverEnabled: true; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: control.toggled() }
}
