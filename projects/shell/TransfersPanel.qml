import QtQuick
import QtQuick.Dialogs
import "../shared" as Shared

Column {
  id: panel
  required property var theme
  required property var store
  property string moveGroup: ""
  property int moveIndex: -1
  function revealGroup() {
    for (var i = 0; i < cards.children.length; i++) {
      var child = cards.children[i]
      if (child.entry && child.entry.id === store.expanded) {
        viewport.contentY = Math.max(0, Math.min(child.y, viewport.contentHeight - viewport.height))
        return
      }
    }
  }
  Connections {
    target: panel.store
    function onExpandedChanged() { Qt.callLater(panel.revealGroup) }
  }
  width: parent.width
  spacing: theme.panelSpacing

  FileDialog {
    id: picker
    title: "Send files"
    fileMode: FileDialog.OpenFiles
    onAccepted: panel.store.selectUrls(selectedFiles)
  }
  FolderDialog {
    id: folder
    title: "Move received file"
    onAccepted: {
      var url = String(selectedFolder)
      if (url.indexOf("file:///") === 0) panel.store.enqueue({ op: "move", id: panel.moveGroup, file: panel.moveIndex, directory: decodeURIComponent(url.slice(7)) })
    }
  }
  Shared.ActionButton {
    theme: panel.theme
    width: parent.width
    text: panel.store.selection.length ? panel.store.selection.length + " files selected · change" : "Choose files or drop them here"
    enabled: !panel.store.busy
    onClicked: picker.open()
    DropArea {
      anchors.fill: parent
      onDropped: drop => { if (drop.hasUrls) { panel.store.selectUrls(drop.urls); drop.acceptProposedAction() } }
    }
  }
  Text {
    width: parent.width
    visible: text !== ""
    text: panel.store.actionError || panel.store.failure(panel.store.error)
    textFormat: Text.PlainText
    wrapMode: Text.Wrap
    color: panel.theme.red
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textBody
  }
  Column {
    width: parent.width
    visible: panel.store.selection.length > 0
    spacing: panel.theme.spaceSmall
    Shared.SectionRule { theme: panel.theme; width: parent.width; label: "Send to" }
    Text {
      visible: !panel.store.targets.length
      text: "No personal devices are available."
      color: panel.theme.subtext
      font.family: panel.theme.fontFamily
      font.pixelSize: panel.theme.textBody
    }
    Repeater {
      model: panel.store.targets
      Shared.ActionButton {
        required property var modelData
        theme: panel.theme
        width: parent.width
        text: modelData.name
        enabled: !panel.store.busy
        onClicked: panel.store.enqueue({ op: "send", target: modelData.id })
      }
    }
  }
  Shared.EmptyState {
    theme: panel.theme
    width: parent.width
    visible: !panel.store.groups.length
    glyph: "󰇚"
    title: "No transfers yet"
  }
  Shared.SeeleFlickable {
    id: viewport
    theme: panel.theme
    width: parent.width
    height: Math.min(contentHeight, panel.theme.rowHeight * 8)
    contentHeight: cards.implicitHeight
    clip: true
    Column {
      id: cards
      width: parent.width
      spacing: panel.theme.spaceMedium
      Repeater {
        model: panel.store.model
        Rectangle {
          id: card
          required property var entry
          width: cards.width
          height: body.implicitHeight + panel.theme.cardPadding * 2
          radius: panel.theme.radius
          color: panel.theme.cardColor
          Shared.CardEdge { theme: panel.theme }
          Column {
            id: body
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
            spacing: panel.theme.spaceSmall
            Shared.ActionButton {
              theme: panel.theme
              width: parent.width
              text: (card.entry.direction === "incoming" ? "↓ " : "↑ ") + card.entry.device + " · " + card.entry.files.length + " file(s)"
              selected: panel.store.expanded === card.entry.id
              onClicked: panel.store.expanded = panel.store.expanded === card.entry.id ? "" : card.entry.id
            }
            Text {
              width: parent.width
              text: card.entry.state + " · " + Math.round(card.entry.bytes / 1024) + " / " + Math.round(card.entry.size / 1024) + " KiB"
              textFormat: Text.PlainText
              color: card.entry.state === "failed" ? panel.theme.red : panel.theme.subtext
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }
            Shared.MeterBar { theme: panel.theme; width: parent.width; ratio: card.entry.size > 0 ? card.entry.bytes / card.entry.size : 0 }
            Text {
              width: parent.width
              visible: card.entry.error !== ""
              text: panel.store.failure(card.entry.error)
              wrapMode: Text.Wrap
              color: panel.theme.red
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }
            Flow {
              width: parent.width
              spacing: panel.theme.spaceSmall
              Shared.ActionButton {
                theme: panel.theme
                text: "Cancel"
                visible: ["sending", "retrying", "receiving"].indexOf(card.entry.state) >= 0
                enabled: !panel.store.busy
                onClicked: panel.store.enqueue({ op: "cancel", id: card.entry.id })
              }
              Shared.ActionButton {
                theme: panel.theme
                text: "Retry"
                visible: ["failed", "cancelled"].indexOf(card.entry.state) >= 0
                enabled: !panel.store.busy
                onClicked: panel.store.enqueue({ op: "retry", id: card.entry.id })
              }
              Shared.ActionButton {
                theme: panel.theme
                text: "Clear history"
                visible: ["completed", "failed", "cancelled"].indexOf(card.entry.state) >= 0
                enabled: !panel.store.busy
                onClicked: panel.store.enqueue({ op: "dismiss", id: card.entry.id })
              }
            }
            Column {
              width: parent.width
              visible: panel.store.expanded === card.entry.id
              spacing: panel.theme.spaceMedium
              Repeater {
                model: card.entry.files
                Column {
                  required property var modelData
                  required property int index
                  id: fileRow
                  width: parent.width
                  spacing: panel.theme.spaceSmall
                  Text {
                    width: parent.width
                    text: fileRow.modelData.name + " · " + fileRow.modelData.state + " · " + Math.round(fileRow.modelData.bytes / 1024) + " / " + Math.round(fileRow.modelData.size / 1024) + " KiB"
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: panel.theme.text
                    font.family: panel.theme.fontFamily
                    font.pixelSize: panel.theme.textBody
                  }
                  Text {
                    width: parent.width
                    visible: !!fileRow.modelData.error
                    text: panel.store.failure(fileRow.modelData.error)
                    wrapMode: Text.Wrap
                    color: panel.theme.red
                    font.family: panel.theme.fontFamily
                    font.pixelSize: panel.theme.textCaption
                  }
                  Flow {
                    width: parent.width
                    visible: card.entry.direction === "incoming" && fileRow.modelData.state === "completed"
                    spacing: panel.theme.spaceSmall
                    Repeater {
                      model: ["open", "reveal", "move", "trash"]
                      Shared.ActionButton {
                        required property string modelData
                        theme: panel.theme
                        text: ({ open: "Open", reveal: "Reveal", move: "Move…", trash: "Move to Trash" })[modelData]
                        danger: modelData === "trash"
                        enabled: !panel.store.busy
                        onClicked: {
                          if (modelData === "move") { panel.moveGroup = card.entry.id; panel.moveIndex = fileRow.index; folder.open() }
                          else panel.store.enqueue({ op: modelData, id: card.entry.id, file: fileRow.index })
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
