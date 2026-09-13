import QtQuick

// What a surface says when it has nothing to show. Each of these is a distinct
// situation with its own way out, so the state carries its own mark, its own
// sentence and its own action rather than all of them sharing one grey line.
Item {
  id: emptyState

  required property var theme
  property string glyph: ""
  property string title: ""
  property string detail: ""
  property color tint: emptyState.theme.overlay
  default property alias action: emptyStateAction.data

  implicitHeight: emptyStateColumn.implicitHeight

  Column {
    id: emptyStateColumn

    anchors.centerIn: parent
    width: Math.min(parent.width - emptyState.theme.cardPadding * 2, 320)
    spacing: emptyState.theme.spaceSmall

    CenteredGlyph {
      visible: emptyState.glyph !== ""
      width: parent.width
      height: emptyState.glyph !== "" ? emptyState.theme.textHero : 0
      text: emptyState.glyph
      color: emptyState.tint
      font.family: emptyState.theme.fontFamily
      font.pixelSize: emptyState.theme.textHero
    }

    Text {
      width: parent.width
      visible: emptyState.title !== ""
      text: emptyState.title
      textFormat: Text.PlainText
      color: emptyState.theme.subtext
      font.family: emptyState.theme.fontFamily
      font.pixelSize: emptyState.theme.textBody
      font.weight: emptyState.theme.weightMedium
      horizontalAlignment: Text.AlignHCenter
      wrapMode: Text.Wrap
    }

    Text {
      width: parent.width
      visible: emptyState.detail !== ""
      text: emptyState.detail
      textFormat: Text.PlainText
      color: emptyState.theme.overlay
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
