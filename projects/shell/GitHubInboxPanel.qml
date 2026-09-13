pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

FocusScope {
  id: panel
  required property var theme
  required property var store
  property bool popupHovered: false
  readonly property var snapshot: store.snapshot
  readonly property var selected: snapshot.detail
  readonly property bool detailOpen: !!selected
  implicitHeight: content.implicitHeight

  function handleKey(event) {
    if (event.modifiers & Qt.ControlModifier) return
    if (event.key === Qt.Key_Escape && detailOpen) { store.send("back"); event.accepted = true }
    else if (event.key === Qt.Key_R) { store.send(detailOpen ? "retry" : "refresh", snapshot.selected); event.accepted = true }
    else if (event.key === Qt.Key_O && detailOpen) { store.send("open", snapshot.selected); event.accepted = true }
    else if (event.key === Qt.Key_D && detailOpen) { store.send("done", snapshot.selected); event.accepted = true }
    else if (!detailOpen && (event.key === Qt.Key_J || event.key === Qt.Key_Down)) { inbox.incrementCurrentIndex(); event.accepted = true }
    else if (!detailOpen && (event.key === Qt.Key_K || event.key === Qt.Key_Up)) { inbox.decrementCurrentIndex(); event.accepted = true }
    else if (!detailOpen && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && inbox.currentIndex >= 0 && inbox.currentIndex < store.model.count) { store.send("select", store.model.get(inbox.currentIndex).entry.id); event.accepted = true }
  }
  Keys.onPressed: event => handleKey(event)
  Column {
    id: content
    width: parent.width
    spacing: panel.theme.panelSpacing
    Shared.StatusBanner {
      theme: panel.theme
      width: parent.width
      visible: !!(panel.store.connectionError || panel.snapshot.error)
      title: panel.store.connectionError || panel.snapshot.error
      tint: panel.theme.yellow
      Shared.ActionButton { theme: panel.theme; text: "Retry"; onClicked: panel.store.send("refresh") }
    }
    Text {
      width: parent.width
      visible: !!panel.snapshot.notice
      text: panel.snapshot.notice
      textFormat: Text.PlainText
      color: panel.theme.subtext
      font.family: panel.theme.fontFamily
      font.pixelSize: panel.theme.textCaption
      wrapMode: Text.Wrap
    }
    Flow {
      width: parent.width
      spacing: panel.theme.spaceSmall
      Shared.ActionButton { theme: panel.theme; visible: panel.detailOpen; text: "← Inbox"; onClicked: panel.store.send("back") }
      Shared.ActionButton {
        theme: panel.theme; visible: panel.detailOpen
        text: panel.selected && panel.selected.pendingDone ? "Marking Done…" : "Done · D"
        enabled: panel.detailOpen && !panel.selected.pendingDone && !panel.store.connectionError
        onClicked: panel.store.send("done", panel.snapshot.selected)
      }
      Shared.ActionButton {
        theme: panel.theme; visible: panel.detailOpen
        text: "Open on GitHub · O"
        enabled: panel.detailOpen && (!!panel.selected.thread.url || (panel.selected.detail && !!panel.selected.detail.url))
        onClicked: panel.store.send("open", panel.snapshot.selected)
      }
      Shared.ActionButton {
        theme: panel.theme; visible: !panel.detailOpen || (!panel.selected.thread.url && (!panel.selected.detail || !panel.selected.detail.url)); text: "GitHub inbox"
        onClicked: panel.store.send("inbox")
      }
      Shared.ActionButton {
        theme: panel.theme; visible: panel.detailOpen && panel.selected.state === "failed"
        text: "Retry triage · R"; onClicked: panel.store.send("retry", panel.snapshot.selected)
      }
    }
    Text {
      width: parent.width
      text: panel.detailOpen ? "Save and restoring Done are available in GitHub's web inbox." : (panel.snapshot.complete ? "" : "Loading inbox · count may be incomplete\n") + "↑/↓ select · Enter read · R refresh"
      textFormat: Text.PlainText
      color: panel.theme.subtext
      font.family: panel.theme.fontFamily
      font.pixelSize: panel.theme.textCaption
      wrapMode: Text.Wrap
    }
    Shared.EmptyState {
      theme: panel.theme
      width: parent.width
      visible: !panel.detailOpen && panel.store.model.count === 0
      title: panel.snapshot.refreshing ? "Loading notifications…" : panel.snapshot.complete ? "Your GitHub inbox is clear" : "Inbox unavailable"
      detail: "Notifications stay unread when opened here."
    }
    Shared.SeeleListView {
      id: inbox
      theme: panel.theme
      width: parent.width
      visible: !panel.detailOpen && panel.store.model.count > 0
      height: visible ? Math.min(contentHeight, panel.theme.notificationRowHeight * 7) : 0
      clip: true
      spacing: panel.theme.spaceSmall
      model: panel.store.model
      currentIndex: count ? 0 : -1
      ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }
      delegate: Rectangle {
        id: row
        required property var entry
        required property int index
        width: inbox.width - panel.theme.scrollGutter
        height: rowText.implicitHeight + panel.theme.cardPadding * 2
        radius: panel.theme.radius
        color: inbox.currentIndex === index ? panel.theme.selectedColor : panel.theme.cardColor
        Shared.CardEdge { theme: panel.theme }
        HoverHandler { id: hover }
        Shared.HoverWash { theme: panel.theme; hovered: hover.hovered }
        Column {
          id: rowText
          anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
          anchors.margins: panel.theme.cardPadding
          spacing: panel.theme.spaceTight
          Text {
            width: parent.width; text: row.entry.title; textFormat: Text.PlainText
            color: panel.theme.text; font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textBody; font.weight: panel.theme.weightStrong
            wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
          }
          Text {
            width: parent.width; text: row.entry.repository + " · " + row.entry.kind + " · " + row.entry.reason
            textFormat: Text.PlainText; color: panel.theme.subtext
            font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption
            elide: Text.ElideRight
          }
          Text {
            width: parent.width
            text: row.entry.triage ? row.entry.triage.priority : row.entry.state === "failed" ? "Triage failed · open to retry" : "Triage pending"
            textFormat: Text.PlainText
            color: row.entry.triage && row.entry.triage.priority === "Immediate Action required" ? panel.theme.red : row.entry.triage && row.entry.triage.priority === "Action required soon" ? panel.theme.yellow : panel.theme.subtext
            font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption
            wrapMode: Text.Wrap
          }
          Text {
            width: parent.width; visible: !!row.entry.triage
            text: row.entry.triage ? row.entry.triage.summary : ""
            textFormat: Text.PlainText; color: panel.theme.subtext
            font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption
            wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
          }
        }
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { inbox.currentIndex = row.index; panel.store.send("select", row.entry.id) } }
      }
    }
    Shared.SeeleListView {
      id: detailList
      theme: panel.theme
      width: parent.width
      visible: panel.detailOpen
      height: visible ? Math.min(contentHeight, panel.theme.notificationRowHeight * 9) : 0
      clip: true
      spacing: panel.theme.spaceSmall
      model: panel.store.detailModel
      ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }
      delegate: Rectangle {
        id: detailBlock
        required property var block
        width: detailList.width - panel.theme.scrollGutter
        height: blockText.implicitHeight + panel.theme.cardPadding * 2
        color: panel.theme.cardColor
        radius: panel.theme.radius
        Shared.CardEdge { theme: panel.theme }
        Column {
          id: blockText
          anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
          anchors.margins: panel.theme.cardPadding
          spacing: panel.theme.spaceSmall
          Text { width: parent.width; text: detailBlock.block.label; textFormat: Text.PlainText; color: panel.theme.accent; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody; font.weight: panel.theme.weightStrong; wrapMode: Text.Wrap }
          Text { width: parent.width; text: detailBlock.block.meta || ""; visible: text !== ""; textFormat: Text.PlainText; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption; wrapMode: Text.WrapAnywhere }
          TextEdit { width: parent.width; text: detailBlock.block.body; textFormat: TextEdit.PlainText; readOnly: true; selectByMouse: true; color: panel.theme.text; selectionColor: panel.theme.selectedColor; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody; wrapMode: TextEdit.WrapAnywhere }
        }
      }
    }
  }
}
