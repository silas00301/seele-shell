import QtQuick

// Where the keyboard is. A control that can be tabbed to and draws nothing for
// it is a control the keyboard cannot actually reach, so the ring is a part
// rather than a rectangle each surface remembers to add. It is laid over the
// control it belongs to and follows that control's own corner, so a picker and
// a transport button are ringed the same way. The ring is `focusRingWidth`
// wide and lies `focusRingGap` outside the control's edge, or inside it where
// the gap is negative; a theme that gives `durationFocusRing` flares it as it
// appears, as Material's focus indicator grows and settles. A ring outside
// the edge needs room: where the control or an ancestor clips and would cut
// the settled ring, as a card or a scrolled list does at its edge, the ring
// lies on the control's own edge instead, so it is always drawn whole. The
// ring rests on
// its colour at zero alpha rather than on `transparent`, because a surface may
// animate it and Qt drags a colour interpolated against black through grey.
Rectangle {
  id: focusRing

  required property var theme
  property bool shown: false
  // The corner of the control the ring follows.
  property real cornerRadius: (parent as Rectangle) ? (parent as Rectangle).radius : focusRing.theme.radius
  // The ring's width while it flares; the rest of the time its theme's.
  property real stroke: focusRing.theme.focusRingWidth
  // Where the ring would be cut, it lies inside the edge.
  property bool cramped: false
  readonly property real outset: focusRing.cramped ? 0 : focusRing.theme.focusRingGap + focusRing.stroke

  anchors.fill: parent
  anchors.margins: -focusRing.outset
  radius: Math.max(0, focusRing.cornerRadius + focusRing.outset)
  visible: focusRing.shown
  color: focusRing.theme.alpha(focusRing.theme.focusRingColor, 0)
  border.width: focusRing.stroke
  border.color: focusRing.theme.focusRingColor
  antialiasing: true

  // Whether the settled ring, outside the edge, would be cut by the control
  // or an ancestor that clips.
  function cut() {
    var control = focusRing.parent
    var reach = focusRing.theme.focusRingGap + focusRing.theme.focusRingWidth
    if (!control || reach <= 0) return false
    for (var item = control; item; item = item.parent) {
      if (!item.clip) continue
      var ring = control.mapToItem(item, -reach, -reach, control.width + reach * 2, control.height + reach * 2)
      if (ring.x < 0 || ring.y < 0 || ring.x + ring.width > item.width || ring.y + ring.height > item.height) return true
    }
    return false
  }
  function place() {
    if (focusRing.shown) focusRing.cramped = focusRing.cut()
  }

  onShownChanged: {
    flare.stop()
    focusRing.stroke = Qt.binding(() => focusRing.theme.focusRingWidth)
    focusRing.place()
    // A list scrolls the item it focused into view after the focus moved.
    if (focusRing.shown) Qt.callLater(focusRing.place)
    if (focusRing.shown && focusRing.theme.durationFocusRing > 0) flare.start()
  }
  Connections {
    target: focusRing.shown ? focusRing.parent : null
    function onXChanged() { focusRing.place() }
    function onYChanged() { focusRing.place() }
    function onWidthChanged() { focusRing.place() }
    function onHeightChanged() { focusRing.place() }
  }

  SequentialAnimation {
    id: flare
    NumberAnimation {
      target: focusRing; property: "stroke"; from: 0; to: focusRing.theme.focusRingFlare
      duration: focusRing.theme.durationFocusRing / 4
      easing.type: focusRing.theme.easingStandard; easing.bezierCurve: focusRing.theme.easingStandardCurve
    }
    NumberAnimation {
      target: focusRing; property: "stroke"; to: focusRing.theme.focusRingWidth
      duration: focusRing.theme.durationFocusRing - focusRing.theme.durationFocusRing / 4
      easing.type: focusRing.theme.easingStandard; easing.bezierCurve: focusRing.theme.easingStandardCurve
    }
    ScriptAction { script: focusRing.stroke = Qt.binding(() => focusRing.theme.focusRingWidth) }
  }
}
