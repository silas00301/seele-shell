import QtQuick

// What a surface says when it has nothing to show. Each of these is a distinct
// situation with its own way out, so the state carries its own mark, its own
// sentence and its own action rather than all of them sharing one grey line.
// The mark sits in one of Material 3 Expressive's shapes, the way an empty
// state on Android leads with an illustration rather than a bare icon.
Item {
  id: emptyState

  required property var theme
  property string glyph: ""
  property string title: ""
  property string detail: ""
  property color tint: emptyState.theme.subtext
  property string markShape: "softBurst"
  default property alias action: emptyStateAction.data

  implicitHeight: emptyStateColumn.implicitHeight

  Column {
    id: emptyStateColumn

    anchors.centerIn: parent
    width: Math.min(parent.width - emptyState.theme.cardPadding * 2, 320)
    spacing: emptyState.theme.spaceSmall

    Item {
      visible: emptyState.glyph !== ""
      width: parent.width
      height: emptyState.glyph !== "" ? emptyState.theme.emptyMarkSize : 0

      MaterialShape {
        theme: emptyState.theme
        anchors.horizontalCenter: parent.horizontalCenter
        width: parent.height
        height: width
        shape: emptyState.markShape
        color: Qt.tint(emptyState.theme.surfaceContainerHighest, emptyState.theme.alpha(emptyState.tint, 0.16))
      }

      CenteredGlyph {
        anchors.fill: parent
        text: emptyState.glyph
        color: emptyState.tint
        font.family: emptyState.theme.fontFamily
        font.pixelSize: emptyState.theme.textDisplay
      }
    }

    Text {
      width: parent.width
      visible: emptyState.title !== ""
      text: emptyState.title
      textFormat: Text.PlainText
      color: emptyState.theme.text
      font.family: emptyState.theme.fontFamily
      font.pixelSize: emptyState.theme.textBody
      font.weight: emptyState.theme.weightStrong
      horizontalAlignment: Text.AlignHCenter
      wrapMode: Text.Wrap
    }

    Text {
      width: parent.width
      visible: emptyState.detail !== ""
      text: emptyState.detail
      textFormat: Text.PlainText
      color: emptyState.theme.subtext
      font.family: emptyState.theme.fontFamily
      font.pixelSize: emptyState.theme.textCaption
      horizontalAlignment: Text.AlignHCenter
      wrapMode: Text.Wrap
    }

    Row {
      id: emptyStateAction

      anchors.horizontalCenter: parent.horizontalCenter
      topPadding: emptyStateAction.children.length ? emptyState.theme.spaceSmall : 0
      spacing: emptyState.theme.spaceMedium
    }
  }
}
