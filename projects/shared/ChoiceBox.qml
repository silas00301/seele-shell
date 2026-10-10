pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

// Native ComboBox navigation and accessibility, drawn as Material's exposed
// dropdown menu: the filled field the value sits in, and a menu on the
// extra-large surface step with the current choice in the secondary container.
ComboBox {
  id: choice
  required property var theme
  implicitHeight: theme.controlHeight
  focusPolicy: Qt.StrongFocus
  hoverEnabled: true
  leftPadding: theme.spaceLarge
  rightPadding: theme.controlHeight
  contentItem: Text {
    text: choice.displayText
    textFormat: Text.PlainText
    color: choice.enabled ? choice.theme.text : choice.theme.subtext
    font.family: choice.theme.fontFamily
    font.pixelSize: choice.theme.textBody
    verticalAlignment: Text.AlignVCenter
    elide: Text.ElideRight
  }
  indicator: Text {
    anchors { right: parent.right; rightMargin: choice.theme.spaceLarge; verticalCenter: parent.verticalCenter }
    text: choice.popup.visible ? "󰅃" : "󰅀"
    color: choice.activeFocus ? choice.theme.primary : choice.theme.subtext
    font.family: choice.theme.fontFamily
    font.pixelSize: choice.theme.textIcon
  }
  background: Rectangle {
    topLeftRadius: choice.theme.shapeExtraSmall
    topRightRadius: choice.theme.shapeExtraSmall
    bottomLeftRadius: 0
    bottomRightRadius: 0
    color: choice.theme.surfaceContainerHighest
    HoverWash { theme: choice.theme; hovered: choice.hovered && choice.enabled }
    Rectangle {
      anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
      height: choice.activeFocus || choice.popup.visible ? choice.theme.focusWidth : choice.theme.hairline
      color: choice.activeFocus || choice.popup.visible ? choice.theme.primary : choice.theme.subtext
    }
  }
  delegate: ItemDelegate {
    id: option
    required property int index
    required property var modelData
    width: choice.width
    height: choice.theme.controlHeight
    padding: choice.theme.spaceMedium
    hoverEnabled: true
    highlighted: choice.highlightedIndex === index
    contentItem: Text {
      text: choice.textRole ? option.modelData[choice.textRole] : option.modelData
      textFormat: Text.PlainText
      color: option.highlighted ? choice.theme.textOnSecondaryContainer : choice.theme.text
      font.family: choice.theme.fontFamily
      font.pixelSize: choice.theme.textBody
      verticalAlignment: Text.AlignVCenter
      elide: Text.ElideRight
    }
    background: Rectangle {
      radius: choice.theme.shapeMedium
      color: option.highlighted ? choice.theme.secondaryContainer : choice.theme.clearColor
      HoverWash { theme: choice.theme; hovered: option.hovered }
    }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
  }
  popup: Popup {
    y: choice.height + choice.theme.spaceTight
    width: choice.width
    padding: choice.theme.spaceTight
    implicitHeight: Math.min(contentItem.implicitHeight + padding * 2, choice.theme.controlHeight * 6)
    contentItem: SeeleListView {
      theme: choice.theme
      implicitHeight: contentHeight
      clip: true
      model: choice.popup.visible ? choice.delegateModel : null
      currentIndex: choice.highlightedIndex
      ScrollBar.vertical: SlimScrollBar { theme: choice.theme; popupHovered: true }
    }
    background: Rectangle {
      color: choice.theme.surfaceContainerHigh
      radius: choice.theme.shapeLarge
      border.width: choice.theme.hairline
      border.color: choice.theme.panelBorder
    }
  }
  HoverHandler { cursorShape: choice.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor }
}
