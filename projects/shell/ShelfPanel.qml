import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Quickshell.Io
import "../shared" as Shared

Column {
  id: panel
  required property var theme
  required property var store
  property int cursor: 0
  property bool firstPrefix: false
  property string notice: ""
  signal closeRequested()
  Timer { id: prefixTimer; interval: 800; onTriggered: panel.firstPrefix = false }
  signal previewRequested(string path)
  signal transferRequested(var paths)
  spacing: theme.panelSpacing
  function move(step) {
    cursor = Math.max(0, Math.min(store.items.length - 1, cursor + step))
    list.positionViewAtIndex(cursor, ListView.Contain)
  }
  Keys.onPressed: event => {
    if ((event.modifiers & Qt.ControlModifier) && (event.key === Qt.Key_D || event.key === Qt.Key_U)) {
      move((event.key === Qt.Key_D ? 1 : -1) * Math.max(1, Math.floor(list.height / (theme.controlHeight * 3.2))))
      event.accepted = true; return
    }
    if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
    if (event.key === Qt.Key_Q || event.key === Qt.Key_Escape) { if (!event.isAutoRepeat) closeRequested(); event.accepted = true; return }
    if (!panel.activeFocus) return
    if (event.key === Qt.Key_J || event.key === Qt.Key_Down) { move(1); event.accepted = true }
    else if (event.key === Qt.Key_K || event.key === Qt.Key_Up) { move(-1); event.accepted = true }
    else if (event.key === Qt.Key_G || event.key === Qt.Key_Home || event.key === Qt.Key_End) {
      if (event.key !== Qt.Key_G || (event.modifiers & Qt.ShiftModifier)) {
        move(event.key === Qt.Key_End || (event.modifiers & Qt.ShiftModifier) ? store.items.length : -store.items.length)
        firstPrefix = false
      } else if (!event.isAutoRepeat) {
        if (firstPrefix) { move(-store.items.length); firstPrefix = false; prefixTimer.stop() }
        else { firstPrefix = true; prefixTimer.restart() }
      }
      event.accepted = true
    } else if (!event.isAutoRepeat && event.key === Qt.Key_Space && store.items[cursor]) {
      store.send({op:"select",id:store.items[cursor].id}); event.accepted = true
    } else if (!event.isAutoRepeat && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && store.items[cursor] && store.items[cursor].available) {
      previewRequested(store.items[cursor].path); event.accepted = true
    } else if (!event.isAutoRepeat && (event.key === Qt.Key_Delete || event.key === Qt.Key_D)) {
      store.send({op:"remove"}); event.accepted = true
    }
  }
  Connections { target: panel.store; function onItemsChanged() { panel.cursor = Math.max(0, Math.min(panel.cursor, panel.store.items.length - 1)) } }
  FileDialog { id: picker; title: "Collect files"; fileMode: FileDialog.OpenFiles; onAccepted: panel.store.send({op:"files",paths:selectedFiles.map(function(uri) { return String(uri) })}) }
  Row {
    width: parent.width; spacing: panel.theme.spaceSmall
    Shared.ActionButton { theme: panel.theme; text: "Add files"; onClicked: picker.open() }
    Shared.ActionButton { objectName: "shelfClipboard"; theme: panel.theme; text: "Collect clipboard text"; onClicked: panel.store.send({op:"clipboard"}) }
  }
  Rectangle {
    width: parent.width; height: Math.max(panel.theme.controlHeight * 2, Math.min(list.contentHeight, panel.theme.controlHeight * 7))
    radius: panel.theme.radius; color: panel.theme.cardColor; border.color: drop.containsDrag ? panel.theme.accent : panel.theme.cardBorder
    Text { anchors.centerIn: parent; visible: panel.store.items.length === 0; text: "Drop files or text here"; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody }
    ListView {
      id: list
      anchors.fill: parent; anchors.margins: panel.theme.spaceSmall; clip: true
      model: panel.store.items; spacing: panel.theme.spaceSmall
      delegate: Rectangle {
        id: row
        required property var modelData
        required property int index
        width: list.width; height: panel.theme.controlHeight * 1.6; radius: panel.theme.radius
        color: modelData.selected ? panel.theme.selectedColor : panel.theme.cardColor
        Accessible.role: Accessible.CheckBox
        Accessible.name: modelData.name
        Accessible.checked: modelData.selected
        border.color: modelData.selected ? panel.theme.accent : panel.theme.cardBorder
        Shared.FocusRing { theme: panel.theme; shown: panel.activeFocus && panel.cursor === row.index }
        Image { id: thumbnail; x: panel.theme.spaceSmall; y: panel.theme.spaceSmall; width: parent.height - panel.theme.spaceSmall * 2; height: width; visible: row.modelData.image && row.modelData.available; source: visible ? row.modelData.uri : ""; sourceSize.width: 128; sourceSize.height: 128; fillMode: Image.PreserveAspectFit; asynchronous: true }
        Column {
          x: thumbnail.visible ? thumbnail.x + thumbnail.width + panel.theme.spaceSmall : panel.theme.spaceLarge
          anchors.verticalCenter: parent.verticalCenter
          width: parent.width - x - panel.theme.spaceLarge
          Text { width: parent.width; text: row.modelData.name; textFormat: Text.PlainText; elide: Text.ElideMiddle; color: row.modelData.available ? panel.theme.text : panel.theme.red; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody }
          Text { width: parent.width; text: !row.modelData.available ? "Original file is unavailable" : row.modelData.caption || Math.ceil(row.modelData.bytes / 1024) + " KiB"; textFormat: Text.PlainText; elide: Text.ElideRight; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption }
        }
        MouseArea {
          anchors.fill: parent
          onClicked: { panel.cursor = row.index; panel.forceActiveFocus(); panel.store.send({op:"select",id:row.modelData.id}) }
          onDoubleClicked: if (row.modelData.available) panel.previewRequested(row.modelData.path)
        }
      }
    }
    DropArea {
      id: drop; anchors.fill: parent
      onDropped: event => {
        if (event.hasUrls) panel.store.send({op:"files",paths:event.urls.map(function(uri) { return String(uri) })})
        else if (event.hasText) panel.store.send({op:"text",text:event.text})
        else return
        event.acceptProposedAction()
      }
    }
  }
  Row {
    width: parent.width; spacing: panel.theme.spaceSmall
    Shared.ActionButton { theme: panel.theme; text: "Select all"; enabled: panel.store.items.length > 0; onClicked: panel.store.send({op:"all",selected:true}) }
    Shared.ActionButton { theme: panel.theme; text: "Remove selected"; enabled: panel.store.items.some(function(item) { return item.selected }); onClicked: panel.store.send({op:"remove"}) }
    Shared.ActionButton { theme: panel.theme; text: "Clear"; enabled: panel.store.items.length > 0; onClicked: panel.store.send({op:"clear"}) }
  }
  Shared.ActionButton {
    id: exportButton
    objectName: "shelfDragHandle"
    theme: panel.theme; width: parent.width
    text: "Drag " + panel.store.selected.length + " selected " + (panel.store.selected.length === 1 ? "item" : "items") + " together"
    enabled: panel.store.selected.length > 0
    // A drag has its own handle; changing selection never begins a drag.
    Drag.dragType: Drag.Automatic
    Drag.supportedActions: Qt.CopyAction
    Drag.mimeData: ({"text/uri-list":panel.store.uris})
    Drag.active: dragging.active
    DragHandler { id: dragging; target: null; enabled: exportButton.enabled }
  }
  Row {
    width: parent.width; spacing: panel.theme.spaceSmall
    Shared.ActionButton { theme: panel.theme; text: "Capture in Notes"; enabled: panel.store.paths.length > 0 && !notes.running; onClicked: { panel.notice = ""; notes.command = ["seele-notes-store","capture-files"].concat(panel.store.paths); notes.running = true } }
    Shared.ActionButton { theme: panel.theme; text: "Send with Transfers"; enabled: panel.store.paths.length > 0; onClicked: panel.transferRequested(panel.store.paths) }
  }
  Text { width: parent.width; text: panel.notice; visible: text !== ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: panel.theme.green; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody }
  Text { width: parent.width; text: panel.store.error; visible: text !== ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: panel.theme.red; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody }
  Text { width: parent.width; text: "Until cleared or this shell exits · originals stay where they are"; wrapMode: Text.Wrap; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption }
  Process {
    id: notes
    stdout: StdioCollector { onStreamFinished: {
      try { var value = JSON.parse(text); if (value.ok) { notesOpen.command = ["seele-notes"]; notesOpen.running = true; panel.notice = "Captured in Notes. The new note is in your capture folder." } else panel.store.error = value.error || "Could not capture files in Notes" }
      catch (_) { panel.store.error = "Could not capture files in Notes" }
    } }
  }
  Process { id: notesOpen }
}
