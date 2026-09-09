pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../shared" as Shared

// Choosing where captures land. The app never assumes a path: it offers the
// vaults it can find, lets any directory be walked to, and says plainly what
// it will create before it creates anything.
ColumnLayout {
  id: setup

  required property var theme
  required property var store
  property string folder: "Inbox"
  property bool chosen: false

  spacing: setup.theme.panelSpacing

  function refresh() {
    setup.store.findVaults()
    setup.store.listDirectory("")
  }

  // The window is up before the worker has answered, so the first questions
  // are asked once there is something to answer them.
  Component.onCompleted: if (setup.store.ready) setup.refresh()

  Connections {
    target: setup.store

    function onReadyChanged() { if (setup.store.ready) setup.refresh() }

    // One vault on the machine is not a choice worth making. The picker opens
    // inside it with the caret in the folder name, so the whole of setup is
    // one word and Return.
    function onVaultsChanged() {
      if (setup.store.vaults.length !== 1 || setup.chosen) return
      setup.chosen = true
      setup.store.listDirectory(setup.store.vaults[0].path)
      folderName.forceActiveFocus()
      folderName.selectAll()
    }
  }

  Text {
    Layout.fillWidth: true
    text: "Seele Notes writes ordinary Markdown files into one folder of an Obsidian vault. Obsidian stays the place everything else lives."
    color: setup.theme.subtext
    font.family: setup.theme.fontFamily
    font.pixelSize: setup.theme.textBody
    wrapMode: Text.Wrap
  }

  // The old library is mentioned rather than acted on: there is nowhere to
  // migrate it to until this question has been answered.
  Text {
    Layout.fillWidth: true
    visible: setup.store.config.legacy > 0
    text: setup.store.config.legacy + " note(s) from the old private library can be brought in once you have chosen a folder."
    color: setup.theme.accent
    font.family: setup.theme.fontFamily
    font.pixelSize: setup.theme.textCaption
    wrapMode: Text.Wrap
  }

  Shared.SectionRule {
    Layout.fillWidth: true
    theme: setup.theme
    label: "VAULTS FOUND"
    detail: String(setup.store.vaults.length)
    visible: setup.store.vaults.length > 0
  }

  Flow {
    Layout.fillWidth: true
    visible: setup.store.vaults.length > 0
    spacing: setup.theme.spaceSmall

    Repeater {
      model: setup.store.vaults

      Shared.ActionButton {
        id: vaultChoice

        required property var modelData

        theme: setup.theme
        text: vaultChoice.modelData.name
        selected: setup.store.browse && setup.store.browse.path === vaultChoice.modelData.path
        onClicked: setup.store.listDirectory(vaultChoice.modelData.path)
      }
    }
  }

  Shared.SectionRule {
    Layout.fillWidth: true
    theme: setup.theme
    label: "VAULT FOLDER"
    detail: setup.store.browse ? (setup.store.browse.vault ? "Obsidian vault" : "Not a vault") : ""
    detailColor: setup.store.browse && setup.store.browse.vault ? setup.theme.green : setup.theme.overlay

    Shared.ActionButton {
      theme: setup.theme
      text: "Up"
      enabled: !!(setup.store.browse && setup.store.browse.parent)
      onClicked: setup.store.listDirectory(setup.store.browse.parent)
    }
  }

  Text {
    Layout.fillWidth: true
    text: setup.store.browse ? setup.store.browse.path : "…"
    color: setup.theme.text
    font.family: setup.theme.fontFamily
    font.pixelSize: setup.theme.textCaption
    elide: Text.ElideMiddle
  }

  Shared.DeviceListCard {
    Layout.fillWidth: true
    theme: setup.theme
    listHeight: setup.theme.notesPickerHeight

    Shared.SeeleListView {
      id: directories

      theme: setup.theme
      anchors.fill: parent
      anchors.margins: setup.theme.cardPadding
      clip: true
      spacing: setup.theme.spaceTight
      keyNavigationEnabled: true
      model: setup.store.browse ? setup.store.browse.entries : []

      delegate: Rectangle {
        id: directoryRow

        required property var modelData
        required property int index

        width: directories.width
        height: setup.theme.rowHeight
        radius: setup.theme.radiusSmall
        color: directories.currentIndex === directoryRow.index ? setup.theme.selectedColor : setup.theme.rowColor
        antialiasing: true

        Shared.HoverWash { theme: setup.theme; hovered: directoryHover.hovered }
        HoverHandler { id: directoryHover }

        RowLayout {
          anchors.fill: parent
          anchors.leftMargin: setup.theme.spaceMedium
          anchors.rightMargin: setup.theme.spaceMedium
          spacing: setup.theme.spaceMedium

          Text {
            text: directoryRow.modelData.vault ? "󰠮" : "󰉋"
            color: directoryRow.modelData.vault ? setup.theme.accent : setup.theme.overlay
            font.family: setup.theme.fontFamily
            font.pixelSize: setup.theme.textIcon
          }

          Text {
            Layout.fillWidth: true
            text: directoryRow.modelData.name
            elide: Text.ElideRight
            color: setup.theme.text
            font.family: setup.theme.fontFamily
            font.pixelSize: setup.theme.textBody
          }
        }

        MouseArea {
          anchors.fill: parent
          cursorShape: Qt.PointingHandCursor
          onClicked: {
            directories.currentIndex = directoryRow.index
            setup.store.listDirectory(directoryRow.modelData.path)
          }
        }
      }

      Shared.EmptyState {
        anchors.fill: parent
        theme: setup.theme
        visible: directories.count === 0
        glyph: "󰉋"
        title: "No folders here"
        detail: "Go up, or use this folder as the vault."
      }

      ScrollBar.vertical: Shared.SlimScrollBar { theme: setup.theme; popupHovered: true }
    }
  }

  Shared.SectionRule {
    Layout.fillWidth: true
    theme: setup.theme
    label: "CAPTURE FOLDER"
    detail: setup.folder.trim() ? "" : "A name is required"
    detailColor: setup.theme.red
  }

  RowLayout {
    Layout.fillWidth: true
    spacing: setup.theme.spaceMedium

    Shared.SearchField {
      id: folderName

      Layout.fillWidth: true
      theme: setup.theme
      glyph: "󰝰"
      text: setup.folder
      placeholderText: "Inbox"
      onTextEdited: setup.folder = text
      onAccepted: if (confirm.enabled) confirm.clicked()
    }

    Shared.ActionButton {
      id: confirm

      theme: setup.theme
      text: "Use this folder"
      selected: true
      enabled: !!(setup.store.browse && setup.store.browse.vault && setup.folder.trim())
      onClicked: setup.store.configure(setup.store.browse.path, setup.folder.trim())
    }
  }

  Text {
    Layout.fillWidth: true
    visible: !!(setup.store.browse && !setup.store.browse.vault)
    text: "That folder has no .obsidian directory, so it is not a vault. Open the folder Obsidian points at."
    color: setup.theme.yellow
    font.family: setup.theme.fontFamily
    font.pixelSize: setup.theme.textCaption
    wrapMode: Text.Wrap
  }

  Text {
    Layout.fillWidth: true
    visible: !!(setup.store.browse && setup.store.browse.vault && setup.folder.trim())
    text: setup.store.browse
      ? "Captures will be written to " + setup.store.browse.path + "/" + setup.folder.trim() + ", and recordings to its Attachments folder."
      : ""
    color: setup.theme.overlay
    font.family: setup.theme.fontFamily
    font.pixelSize: setup.theme.textCaption
    wrapMode: Text.Wrap
  }
}
