import QtQuick
import "../shared" as Shared

Column {
  id: panel
  required property var theme
  required property var store
  spacing: theme.panelSpacing
  Shared.SectionRule { theme: panel.theme; width: parent.width; label: "Connection"; detail: panel.store.projection.label }
  Rectangle {
    width: parent.width
    height: connectionBody.implicitHeight + panel.theme.cardPadding * 2
    radius: panel.theme.radius
    color: panel.theme.cardColor
    Column {
      id: connectionBody
      anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
      spacing: panel.theme.spaceMedium
      Text {
        width: parent.width
        text: panel.store.projection.detail
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textBody
      }
      Text {
        width: parent.width
        visible: text !== ""
        text: panel.store.snapshot.session ? "Session · " + panel.store.snapshot.session : ""
        textFormat: Text.PlainText
        elide: Text.ElideMiddle
        color: panel.theme.text
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }
      Shared.ActionButton { theme: panel.theme; width: parent.width; text: "Open Hermes Desktop"; onClicked: panel.store.openDesktop() }
    }
  }
  Shared.SectionRule {
    theme: panel.theme
    width: parent.width
    visible: panel.store.projection.pending.length > 0
    label: "Rebuild requests"
    detail: "Approval required"
    detailColor: panel.theme.yellow
  }
  Repeater {
    model: panel.store.projection.pending
    delegate: Rectangle {
      required property var modelData
      width: panel.width
      height: approvalBody.implicitHeight + panel.theme.cardPadding * 2
      radius: panel.theme.radius
      color: panel.theme.cardColor
      Column {
        id: approvalBody
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
        spacing: panel.theme.spaceMedium
        Text {
          width: parent.width
          text: "Rebuild and switch nerv"
          color: panel.theme.text
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textBody
          font.weight: panel.theme.weightStrong
        }
        Text {
          width: parent.width
          text: "Revision " + modelData.revision.slice(0, 12) + " · expires in " + modelData.remaining + "s"
          textFormat: Text.PlainText
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
        Text {
          width: parent.width
          text: "Approval opens the reviewed revision in a terminal. Its committed submodule revisions are fixed. System authentication still applies."
          wrapMode: Text.Wrap
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
        Row {
          width: parent.width
          spacing: panel.theme.spaceMedium
          Shared.ActionButton {
            theme: panel.theme
            width: (parent.width - parent.spacing) / 2
            text: "Deny"
            enabled: !panel.store.busy
            onClicked: panel.store.send({op: "deny", id: modelData.id})
          }
          Shared.ActionButton {
            theme: panel.theme
            width: (parent.width - parent.spacing) / 2
            text: "Approve rebuild"
            enabled: !panel.store.busy
            onClicked: panel.store.send({op: "approve", id: modelData.id})
          }
        }
      }
    }
  }
  Shared.StatusBanner {
    theme: panel.theme
    width: parent.width
    visible: panel.store.error !== ""
    title: panel.store.error
    Shared.ActionButton { theme: panel.theme; text: "Dismiss"; onClicked: panel.store.error = "" }
  }
}
