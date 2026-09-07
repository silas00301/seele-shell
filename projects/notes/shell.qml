pragma ComponentBehavior: Bound
//@ pragma UseQApplication
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import "../shared" as Shared
import "notes.js" as Notes

Shared.Theme {
  id: root
  property bool trashView: false
  property string playingMemo: ""
  readonly property var note: store.current
  readonly property var player: playback.item

  function open() {
    window.visible = true
    Qt.callLater(function() {
      var nativeWindow = window.contentItem.Window.window
      if (nativeWindow) nativeWindow.requestActivate()
    })
  }
  function newNote() { trashView = false; search.text = ""; store.create() }
  function stopPlayback() { if (player) player.stop(); playingMemo = "" }
  function playMemo(memo) {
    if (!player || store.recording) return
    if (playingMemo === memo.id) {
      if (player.playing) player.pause()
      else player.play()
    } else {
      player.stop()
      playingMemo = memo.id
      player.source = "file://" + memo.path.split("/").map(encodeURIComponent).join("/")
      player.play()
    }
  }

  IpcHandler { target: "seele-notes"; function open(): void { root.open() } }
  NotesStore { id: store; onCreated: { title.forceActiveFocus(); title.selectAll() } }
  Loader { id: playback; source: "MemoPlayer.qml" }
  Connections {
    target: store
    function onSelectedChanged() { root.stopPlayback() }
    function onLevel(value) { waveform.push(value) }
  }

  FloatingWindow {
    id: window
    title: "Seele Notes"
    visible: true
    implicitWidth: root.notesWindowWidth
    implicitHeight: root.notesWindowHeight
    minimumSize: Qt.size(root.notesMinimumWidth, root.notesMinimumHeight)
    color: "transparent"
    onClosed: visible = false
    onVisibleChanged: if (!visible) { store.flush(); store.stopRecording(); root.stopPlayback() }

    Shortcut { sequence: "Ctrl+N"; enabled: window.visible && store.ready; onActivated: root.newNote() }
    Shortcut { sequence: "Ctrl+F"; enabled: window.visible; onActivated: search.forceActiveFocus() }
    Shortcut { sequence: "Ctrl+S"; enabled: window.visible; onActivated: store.retry() }
    Shortcut { sequence: "Ctrl+Return"; enabled: window.visible; onActivated: { root.stopPlayback(); if (store.recording) store.stopRecording(); else store.startRecording() } }
    Shortcut { sequence: "Ctrl+W"; enabled: window.visible; onActivated: window.visible = false }

    Shared.PanelSurface {
      id: surface
      theme: root
      ColumnLayout {
        anchors.fill: parent
        anchors.margins: root.panelMargin
        spacing: root.panelSpacing

        Shared.PanelHeader {
          theme: root
          Layout.fillWidth: true
          glyph: "󰎞"
          title: "Notes"
          detail: "Thoughts, ideas, and voice memos"
          Shared.ActionButton { theme: root; text: "New note"; enabled: store.ready; onClicked: root.newNote() }
        }

        Rectangle {
          Layout.fillWidth: true
          implicitHeight: errorRow.implicitHeight + root.spaceMedium * 2
          visible: store.error !== "" || store.warning !== "" || (root.player && root.player.error !== "")
          radius: root.radius
          color: root.dangerTint
          RowLayout {
            id: errorRow
            anchors.fill: parent
            anchors.margins: root.spaceMedium
            Text {
              Layout.fillWidth: true
              text: store.error || store.warning || (root.player ? root.player.error : "")
              color: root.red; font.family: root.fontFamily; font.pixelSize: root.textBody; wrapMode: Text.Wrap
            }
            Shared.ActionButton { theme: root; text: "Retry"; onClicked: store.retry() }
          }
        }

        RowLayout {
          Layout.fillWidth: true
          Layout.fillHeight: true
          spacing: root.panelMargin

          ColumnLayout {
            Layout.preferredWidth: root.notesSidebarWidth
            Layout.maximumWidth: root.notesSidebarWidth
            Layout.fillHeight: true
            spacing: root.spaceMedium
            TextField {
              id: search
              Layout.fillWidth: true
              implicitHeight: root.controlHeight
              placeholderText: "Search notes…"
              color: root.text; placeholderTextColor: root.subtext
              selectionColor: root.accent; selectedTextColor: root.crust
              font.family: root.fontFamily; font.pixelSize: root.textBody
              leftPadding: root.spaceLarge; rightPadding: root.spaceLarge
              background: Rectangle { color: root.wellColor; radius: root.radius; border.width: 1; border.color: search.activeFocus ? root.accent : root.cardBorder }
            }
            RowLayout {
              Layout.fillWidth: true
              Shared.ActionButton { theme: root; Layout.fillWidth: true; text: "Notes"; selected: !root.trashView; onClicked: root.trashView = false }
              Shared.ActionButton { theme: root; Layout.fillWidth: true; text: "Trash"; selected: root.trashView; onClicked: root.trashView = true }
            }
            Shared.SeeleListView {
              id: noteList
              theme: root
              Layout.fillWidth: true
              Layout.fillHeight: true
              clip: true
              spacing: root.spaceTight
              model: Notes.filter(store.notes, search.text, root.trashView)
              delegate: Rectangle {
                id: row
                required property var modelData
                width: ListView.view.width
                height: rowText.implicitHeight + root.cardPadding * 2
                radius: root.radius
                readonly property color resting: store.selected === modelData.id ? root.selectedColor : root.rowColor
                color: rowMouse.pressed ? root.pressColor : rowHover.hovered ? root.hoveredColor(resting) : resting
                Behavior on color { ColorAnimation { duration: root.durationFast } }
                Column {
                  id: rowText
                  anchors.left: parent.left; anchors.right: parent.right
                  anchors.margins: root.cardPadding
                  anchors.verticalCenter: parent.verticalCenter
                  spacing: root.spaceTight
                  Text { width: parent.width; text: row.modelData.title.trim() || "Untitled note"; color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody; font.weight: root.weightStrong; elide: Text.ElideRight }
                  Text { width: parent.width; text: row.modelData.body.replace(/\s+/g, " ") || "No text yet"; color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption; elide: Text.ElideRight }
                  Text { width: parent.width; text: Qt.formatDateTime(new Date(row.modelData.updated * 1000), "yyyy-MM-dd") + (row.modelData.memos.length ? " · 󰍬 " + row.modelData.memos.length : ""); color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
                }
                HoverHandler { id: rowHover }
                MouseArea { id: rowMouse; anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: store.select(row.modelData.id) }
              }
              Text {
                anchors.centerIn: parent
                width: parent.width - root.cardPadding * 2
                visible: noteList.count === 0
                text: !store.ready ? "Connecting…" : search.text ? "No matching notes" : root.trashView ? "Trash is empty" : "Create your first note"
                color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textBody; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap
              }
              ScrollBar.vertical: Shared.SlimScrollBar { theme: root; popupHovered: surface.hovered }
            }
          }

          Rectangle { Layout.fillHeight: true; implicitWidth: 1; color: root.separatorColor }

          ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: root.spaceMedium

            RowLayout {
              Layout.fillWidth: true
              TextField {
                id: title
                Layout.fillWidth: true
                implicitHeight: root.controlHeight
                text: root.note ? root.note.title : ""
                placeholderText: "Untitled note"
                readOnly: !root.note || root.note.trashed
                color: root.text; placeholderTextColor: root.subtext
                selectionColor: root.accent; selectedTextColor: root.crust
                font.family: root.fontFamily; font.pixelSize: root.textTitle; font.weight: root.weightStrong
                background: Item {}
                onTextEdited: store.edit("title", text)
              }
              Shared.ActionButton {
                theme: root
                text: root.note && root.note.trashed ? "Restore" : "Move to trash"
                enabled: root.note !== null && store.recordingNote !== store.selected && store.ready
                onClicked: { root.stopPlayback(); store.trash(root.note.trashed) }
              }
            }
            Text {
              Layout.fillWidth: true
              text: !root.note ? "Select a note or create one to get started." : root.note.trashed ? "In Trash · Restore to edit" : root.note._dirty ? "Saving…" : "Saved locally · " + Qt.formatDateTime(new Date(root.note.updated * 1000), "yyyy-MM-dd HH:mm")
              color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption
            }
            Shared.SeeleFlickable {
              id: editorScroll
              theme: root
              Layout.fillWidth: true
              Layout.fillHeight: true
              clip: true
              contentWidth: width
              contentHeight: editor.height
              TextArea.flickable: TextArea {
                id: editor
                text: root.note ? root.note.body : ""
                readOnly: !root.note || root.note.trashed
                placeholderText: "Write something…"
                color: root.text; placeholderTextColor: root.subtext
                selectionColor: root.accent; selectedTextColor: root.crust
                font.family: root.fontFamily; font.pixelSize: root.textLead
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                persistentSelection: true
                background: Item {}
                onTextChanged: store.edit("body", text)
              }
              ScrollBar.vertical: Shared.SlimScrollBar { theme: root; popupHovered: surface.hovered }
            }

            RowLayout {
              Layout.fillWidth: true
              Shared.SectionLabel { theme: root; text: "VOICE MEMOS"; Layout.fillWidth: true }
              Text { visible: store.recording; text: Notes.duration(store.recordingDuration); color: root.red; font.family: root.fontFamily; font.pixelSize: root.textBody }
              Shared.ActionButton {
                theme: root
                text: store.stopping ? "Saving…" : store.recording ? "Stop recording" : "Record memo"
                danger: store.recording
                enabled: !store.stopping && (store.recording || (!!root.note && !root.note.trashed && store.ready))
                onClicked: { root.stopPlayback(); if (store.recording) store.stopRecording(); else { waveform.clear(); store.startRecording() } }
              }
            }
            Shared.Waveform {
              id: waveform
              theme: root
              Layout.fillWidth: true
              Layout.preferredHeight: root.rowHeight
              visible: store.recording
              tint: root.red
            }
            Text {
              Layout.fillWidth: true
              visible: store.recording && store.recordingNote !== store.selected
              text: "Recording into “" + ((store.find(store.recordingNote) || {}).title || "Untitled note") + "”"
              color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption; elide: Text.ElideRight
            }
            Shared.SeeleListView {
              theme: root
              Layout.fillWidth: true
              Layout.preferredHeight: Math.min(root.notesMemoListHeight, contentHeight)
              model: root.note ? root.note.memos : []
              spacing: root.spaceTight
              clip: true
              delegate: RowLayout {
                id: memoRow
                required property var modelData
                required property int index
                width: ListView.view.width
                height: root.controlHeight
                Shared.ActionButton {
                  theme: root
                  text: root.playingMemo === memoRow.modelData.id && root.player && root.player.playing ? "Pause" : "Play"
                  enabled: root.player !== null && !store.recording
                  onClicked: root.playMemo(memoRow.modelData)
                }
                Text { Layout.fillWidth: true; text: "Voice memo " + (memoRow.index + 1); color: root.text; font.family: root.fontFamily; font.pixelSize: root.textBody }
                Text { text: Notes.duration(memoRow.modelData.duration); color: root.subtext; font.family: root.fontFamily; font.pixelSize: root.textCaption }
              }
              ScrollBar.vertical: Shared.SlimScrollBar { theme: root; popupHovered: surface.hovered }
            }
            Slider {
              Layout.fillWidth: true
              visible: root.playingMemo !== "" && !!root.player
              from: 0
              to: root.player ? Math.max(1, root.player.duration) : 1
              value: root.player ? root.player.position : 0
              onMoved: if (root.player) root.player.seek(value)
              palette.highlight: root.accent
              palette.button: root.accent
              palette.dark: root.wellColor
            }
            Text {
              visible: playback.status === Loader.Error
              Layout.fillWidth: true
              wrapMode: Text.Wrap
              text: "Audio playback is unavailable. Notes and recording still work."
              color: root.red; font.family: root.fontFamily; font.pixelSize: root.textCaption
            }
          }
        }
      }
    }
  }
}
