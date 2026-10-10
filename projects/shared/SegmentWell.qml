import QtQuick

// A set of exclusive choices, drawn as Material 3 Expressive's connected
// button group: the choices sit side by side with a hairline of space between
// them, the group's two ends are fully round and the corners where two
// choices meet are small, so the set reads as one control broken into parts.
// The group tells each choice whether it opens or closes the row, because a
// choice cannot know what was laid out beside it.
Item {
  id: segmentWell

  required property var theme
  default property alias content: segmentWellRow.data

  implicitHeight: segmentWell.theme.chipHeight

  function placeSegments() {
    var shown = []
    for (var index = 0; index < segmentWellRow.children.length; ++index) {
      var child = segmentWellRow.children[index]
      if (child.visible && child.width > 0 && child.groupStart !== undefined) shown.push(child)
    }
    for (var position = 0; position < shown.length; ++position) {
      shown[position].groupStart = position === 0
      shown[position].groupEnd = position === shown.length - 1
    }
  }

  Row {
    id: segmentWellRow

    anchors.fill: parent
    onPositioningComplete: segmentWell.placeSegments()
  }
}
