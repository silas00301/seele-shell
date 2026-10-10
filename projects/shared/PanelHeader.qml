import QtQuick

Item {
  required property var theme
  id: panelHeader

  property string glyph: ""
  // A panel whose subject is drawn rather than typed -- the headphone
  // silhouette -- hands its mark over instead of a glyph.
  property Component mark: null
  property string title: ""
  property string detail: ""
  property color detailColor: panelHeader.theme.subtext
  default property alias trailing: panelHeaderTrailing.data

  // The shape the mark sits in. Every panel takes the same one unless it has
  // a reason not to, so the shape reads as the shell's signature rather
  // than as decoration.
  property string markShape: "cookie9"

  height: panelHeader.detail !== "" ? panelHeader.theme.panelHeaderDetailHeight : panelHeader.theme.panelHeaderHeight

  // The panel's mark sits in one of Material 3 Expressive's shapes, filled
  // with the primary container. The shape is what carries the weight in the
  // row, so the glyph inside it takes a step below the title instead of the
  // step above it a glyph beside text would take, and a panel whose subject
  // is drawn rather than typed lands in the same shape.
  Item {
    id: panelHeaderGlyph

    anchors.left: parent.left
    anchors.verticalCenter: parent.verticalCenter
    width: panelHeader.theme.panelMarkSize
    height: width

    MaterialShape {
      theme: panelHeader.theme
      anchors.fill: parent
      shape: panelHeader.markShape
      color: panelHeader.theme.primaryContainer
    }

    CenteredGlyph {
      visible: panelHeader.mark === null
      anchors.fill: parent
      text: panelHeader.glyph
      color: panelHeader.theme.textOnPrimaryContainer
      font.family: panelHeader.theme.fontFamily
      font.pixelSize: panelHeader.theme.textSubhead
    }

    Loader {
      anchors.centerIn: parent
      sourceComponent: panelHeader.mark
    }
  }

  Column {
    anchors.left: panelHeaderGlyph.right
    anchors.leftMargin: panelHeader.theme.spaceLarge
    anchors.right: panelHeaderTrailing.left
    anchors.rightMargin: panelHeaderTrailing.width > 0 ? panelHeader.theme.spaceLarge : 0
    anchors.verticalCenter: parent.verticalCenter
    spacing: 1

    Text {
      width: parent.width
      text: panelHeader.title
      elide: Text.ElideRight
      color: panelHeader.theme.text
      font.family: panelHeader.theme.fontFamily
      font.pixelSize: panelHeader.theme.textTitle
      font.weight: panelHeader.theme.weightStrong
    }

    Text {
      visible: panelHeader.detail !== ""
      width: parent.width
      text: panelHeader.detail
      elide: Text.ElideRight
      color: panelHeader.detailColor
      font.family: panelHeader.theme.fontFamily
      font.pixelSize: panelHeader.theme.textCaption
    }
  }

  Row {
    id: panelHeaderTrailing

    anchors.right: parent.right
    anchors.verticalCenter: parent.verticalCenter
    spacing: panelHeader.theme.spaceMedium
  }
}
