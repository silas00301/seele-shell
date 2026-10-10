import QtQuick

// One choice inside a SegmentWell. A choice is its own container, a step up
// the surface ramp; the chosen one takes the secondary container and rounds
// its inner corners fully, so it stands out of the group as a pill of its
// own. Held, its inner corners pinch to the extra-small corner. The hairline
// between two choices is drawn inside each, so a caller that divides the
// group's width evenly still fills it exactly.
Item {
  id: segment

  required property var theme
  property bool selected: false
  property bool hovered: false
  property bool pressed: false
  // Set by the group.
  property bool groupStart: false
  property bool groupEnd: false
  // The container. A caller acknowledging a result for the moment it lasts
  // -- a mode that failed where it was pressed -- passes its own.
  property color fill: segment.selected ? segment.theme.secondaryContainer : segment.theme.surfaceContainerHighest
  // Lets content be laid over the container, such as a label or a hit area.
  default property alias content: segmentFace.data
  readonly property alias face: segmentFace

  height: parent ? parent.height : segment.theme.chipHeight

  Rectangle {
    id: segmentFace

    readonly property real outer: height / 2
    property real inner: segment.selected ? height / 2
      : segment.pressed ? segment.theme.shapeExtraSmall
      : segment.theme.shapeSmall

    anchors.fill: parent
    anchors.leftMargin: segment.groupStart ? 0 : segment.theme.segmentGap / 2
    anchors.rightMargin: segment.groupEnd ? 0 : segment.theme.segmentGap / 2
    topLeftRadius: segment.groupStart ? segmentFace.outer : segmentFace.inner
    bottomLeftRadius: segment.groupStart ? segmentFace.outer : segmentFace.inner
    topRightRadius: segment.groupEnd ? segmentFace.outer : segmentFace.inner
    bottomRightRadius: segment.groupEnd ? segmentFace.outer : segmentFace.inner
    color: segment.fill
    antialiasing: true

    Behavior on color { ColorAnimation { duration: segment.theme.durationFast } }
    Behavior on inner {
      NumberAnimation {
        duration: segment.theme.durationFastSpatial
        easing.type: Easing.BezierSpline
        easing.bezierCurve: segment.theme.springFastSpatial
      }
    }

    HoverWash { theme: segment.theme; hovered: segment.hovered; pressed: segment.pressed }
  }
}
