import QtQuick
import "../shared" as Shared

// A readout with one control. Starting a session is the launcher's job, so the
// panel says what is running and offers the way out of it, nothing else.
Column {
  id: panel
  required property var theme
  required property var store
  width: parent.width
  spacing: theme.panelSpacing

  Rectangle {
    width: parent.width
    visible: panel.store.active
    height: body.implicitHeight + panel.theme.cardPadding * 2
    radius: panel.theme.radius
    color: panel.theme.cardColor
    Shared.CardEdge { theme: panel.theme }
    Column {
      id: body
      anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
      spacing: panel.theme.spaceSmall
      Item {
        width: parent.width
        height: Math.max(mode.height, summary.implicitHeight)
        Text {
          id: summary
          anchors { left: parent.left; right: mode.left; rightMargin: panel.theme.spaceMedium; verticalCenter: parent.verticalCenter }
          text: panel.store.detail
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: panel.theme.text
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textBody
          font.weight: panel.theme.weightMedium
        }
        Shared.StatusChip {
          id: mode
          theme: panel.theme
          anchors { right: parent.right; verticalCenter: parent.verticalCenter }
          text: panel.store.headline
          tint: panel.theme.accent
        }
      }
      // The tracked task's own line: what it is, where it runs and which
      // process it is, so a row in the launcher and a row here are the same row.
      Text {
        width: parent.width
        visible: text !== ""
        text: !panel.store.task ? "" : [panel.store.task.project ? "Project " + panel.store.task.project : "", panel.store.task.pid ? "PID " + panel.store.task.pid : ""].filter(part => part !== "").join(" · ")
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }
      Shared.ActionButton {
        theme: panel.theme
        width: parent.width
        text: "Stop"
        enabled: !panel.store.busy
        onClicked: panel.store.stop()
      }
    }
  }
  Shared.EmptyState {
    theme: panel.theme
    width: parent.width
    visible: !panel.store.active
    glyph: "󰅶"
    title: "No Caffeinate session"
    detail: "Start one from Seele Caffeinate in the launcher."
  }
  Text {
    width: parent.width
    visible: text !== ""
    text: panel.store.actionError || panel.store.failure(panel.store.error)
    textFormat: Text.PlainText
    wrapMode: Text.Wrap
    color: panel.theme.red
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
  }
}
