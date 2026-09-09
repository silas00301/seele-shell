import QtQuick

// One inline strip for the things a surface has to say about itself: a failed
// write, a file that changed underneath, a directory that cannot be written
// to. It carries its own actions, because a message without one is a message
// the reader can do nothing about.
Rectangle {
  id: statusBanner

  required property var theme
  property string glyph: ""
  property string title: ""
  property string detail: ""
  property color tint: statusBanner.theme.red
  default property alias actions: statusBannerActions.data

  implicitHeight: Math.max(
    statusBannerText.implicitHeight,
    statusBannerActions.implicitHeight
  ) + statusBanner.theme.spaceMedium * 2
  radius: statusBanner.theme.radius
  color: statusBanner.theme.alpha(statusBanner.tint, 0.14)
  antialiasing: true

  CardEdge { theme: statusBanner.theme }

  CenteredGlyph {
    id: statusBannerGlyph

    visible: statusBanner.glyph !== ""
    anchors.left: parent.left
    anchors.leftMargin: statusBanner.theme.spaceMedium
    anchors.verticalCenter: parent.verticalCenter
    width: statusBanner.glyph !== "" ? statusBanner.theme.textDisplay : 0
    height: width
    text: statusBanner.glyph
    color: statusBanner.tint
    font.family: statusBanner.theme.fontFamily
    font.pixelSize: statusBanner.theme.textCard
  }

  Column {
    id: statusBannerText

    anchors.left: statusBannerGlyph.right
    anchors.leftMargin: statusBanner.theme.spaceMedium
    anchors.right: statusBannerActions.left
    anchors.rightMargin: statusBannerActions.width > 0 ? statusBanner.theme.spaceMedium : 0
    anchors.verticalCenter: parent.verticalCenter
    spacing: 1

    Text {
      width: parent.width
      text: statusBanner.title
      color: statusBanner.tint
      font.family: statusBanner.theme.fontFamily
      font.pixelSize: statusBanner.theme.textBody
      font.weight: statusBanner.theme.weightMedium
      wrapMode: Text.Wrap
    }

    Text {
      width: parent.width
      visible: statusBanner.detail !== ""
      text: statusBanner.detail
      color: statusBanner.theme.subtext
      font.family: statusBanner.theme.fontFamily
      font.pixelSize: statusBanner.theme.textCaption
      wrapMode: Text.Wrap
    }
  }

  Row {
    id: statusBannerActions

    anchors.right: parent.right
    anchors.rightMargin: statusBanner.theme.spaceMedium
    anchors.verticalCenter: parent.verticalCenter
    spacing: statusBanner.theme.spaceSmall
  }
}
