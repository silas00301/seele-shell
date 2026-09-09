pragma ComponentBehavior: Bound
//@ pragma UseQApplication
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import "../shared" as Shared
import "notes.js" as Notes

// Seele Notes is the quick way into one folder of an Obsidian vault. Obsidian
// owns the knowledge base; this owns the thirty seconds between having a
// thought and having it written down.
Shared.Theme {
  id: root

  readonly property var note: store.note
  readonly property var player: playback.item
  readonly property bool narrow: window.width < root.notesNarrowWidth
  // Too narrow to hold both panes, the window becomes one pane at a time: the
  // list takes the whole width when it is asked for and hands it back to the
  // writing area as soon as a note is chosen. Hiding the list outright would
  // leave a tiled window with no way to reach another note.
  readonly property bool folded: narrow ? !listRevealed : sidebarCollapsed
  property bool listRevealed: false
  property bool sidebarCollapsed: false
  property real sidebarWidth: root.notesSidebarWidth
  property bool sourceMode: false
  property bool hintsShown: false
  property string overlayView: ""
  readonly property var rows: Notes.filter(store.trashView ? store.trash : store.notes, search.text)

  function open() {
    window.visible = true
    Qt.callLater(function() {
      var nativeWindow = window.contentItem.Window.window
      if (nativeWindow) nativeWindow.requestActivate()
      if (!store.trashView && !search.activeFocus) editor.focusBody()
    })
  }

  function newNote() {
    store.trashView = false
    search.text = ""
    overlayView = ""
    listRevealed = false
    store.create()
    editor.focusBody()
  }

  function revealList() {
    listRevealed = true
    sidebarCollapsed = false
  }

  function openInObsidian() {
    var uri = Notes.obsidianUri(store.config.vault, store.path)
    if (uri) Qt.openUrlExternally(uri)
  }

  function toggleRecording() {
    audio.playing = ""
    if (root.player) root.player.stop()
    if (store.recording) store.stopRecording()
    else { audio.clear(); store.startRecording() }
  }

  function chooseRow(index) {
    var row = root.rows[index]
    if (!row) return
    if (store.trashView) store.selectTrash(row.id)
    else store.select(row.path)
    listRevealed = false
  }

  IpcHandler { target: "seele-notes"; function open(): void { root.open() } }

  NotesStore {
    id: store

    onLoaded: (text, keepCaret) => editor.load(text, keepCaret)
    onLevel: value => audio.push(value)
    onRecorded: name => {
      // A recording arrives as an embed in the note being written, or as the
      // whole of a new one. Either way it is ordinary Markdown afterwards.
      editor.insertEmbed(name)
      store.flush()
    }
    onUiChanged: {
      if (store.ui.sidebar > 0) root.sidebarWidth = store.ui.sidebar
      root.sidebarCollapsed = !!store.ui.collapsed
    }
  }

  Loader { id: playback; source: "MemoPlayer.qml" }

  FloatingWindow {
    id: window

    title: store.path ? Notes.label(store.note) + " — Seele Notes" : "Seele Notes"
    visible: true
    implicitWidth: root.notesWindowWidth
    implicitHeight: root.notesWindowHeight
    minimumSize: Qt.size(root.notesMinimumWidth, root.notesMinimumHeight)
    color: "transparent"
    onClosed: visible = false
    onVisibleChanged: if (!visible) {
      store.flush()
      store.keepDraft()
      store.stopRecording()
      if (root.player) root.player.stop()
      audio.playing = ""
    }

    Shortcut { sequence: "Ctrl+N"; enabled: window.visible && store.ready; onActivated: root.newNote() }
    Shortcut { sequence: "Ctrl+F"; enabled: window.visible; onActivated: { root.revealList(); search.forceActiveFocus(); search.selectAll() } }
    Shortcut { sequence: "Ctrl+L"; enabled: window.visible; onActivated: { root.revealList(); noteList.forceActiveFocus() } }
    Shortcut { sequence: "Escape"; enabled: window.visible; onActivated: { if (root.overlayView) root.overlayView = ""; else if (root.hintsShown) root.hintsShown = false; else { root.listRevealed = false; editor.focusBody() } } }
    // Asking to save while the file has moved on under you takes the way out
    // that loses nothing: your text becomes its own note and neither version
    // is written over.
    Shortcut {
      sequence: "Ctrl+S"
      enabled: window.visible
      onActivated: store.saveState === "conflict" ? store.resolve("copy") : store.retry()
    }
    Shortcut { sequence: "Ctrl+R"; enabled: window.visible && store.config.configured; onActivated: root.toggleRecording() }
    Shortcut { sequence: "Ctrl+Return"; enabled: window.visible && store.config.configured; onActivated: root.toggleRecording() }
    Shortcut { sequence: "Ctrl+Shift+Delete"; enabled: window.visible; onActivated: store.trashView ? store.restoreNote(store.trashId) : store.trashNote() }
    Shortcut {
      sequence: "Ctrl+Shift+T"
      enabled: window.visible
      onActivated: {
        store.setTrashView(!store.trashView)
        root.revealList()
        noteList.forceActiveFocus()
        noteList.currentIndex = noteList.count ? 0 : -1
      }
    }
    Shortcut { sequence: "Ctrl+Shift+O"; enabled: window.visible && !!store.path; onActivated: root.openInObsidian() }
    Shortcut { sequence: "Ctrl+Shift+M"; enabled: window.visible; onActivated: root.sourceMode = !root.sourceMode }
    Shortcut {
      sequence: "Ctrl+\\"
      enabled: window.visible
      onActivated: {
        if (root.narrow) { root.listRevealed = !root.listRevealed; return }
        root.sidebarCollapsed = !root.sidebarCollapsed
        store.remember(root.sidebarWidth, root.sidebarCollapsed)
      }
    }
    Shortcut { sequence: "Ctrl+P"; enabled: window.visible; onActivated: audio.toggleCurrent() }
    Shortcut { sequence: "Alt+Left"; enabled: window.visible; onActivated: audio.seekBy(-5000) }
    Shortcut { sequence: "Alt+Right"; enabled: window.visible; onActivated: audio.seekBy(5000) }
    Shortcut { sequence: "F1"; enabled: window.visible; onActivated: root.hintsShown = !root.hintsShown }
    Shortcut { sequence: "Ctrl+W"; enabled: window.visible; onActivated: window.visible = false }
    Shortcut { sequence: "Ctrl+B"; enabled: window.visible; onActivated: editor.emphasize("**") }
    Shortcut { sequence: "Ctrl+I"; enabled: window.visible; onActivated: editor.emphasize("*") }
    Shortcut { sequence: "Ctrl+E"; enabled: window.visible; onActivated: editor.emphasize("`") }
    Shortcut { sequence: "Ctrl+K"; enabled: window.visible; onActivated: editor.insertLink() }
    Shortcut { sequence: "Ctrl+Shift+X"; enabled: window.visible; onActivated: editor.toggleTask() }
    Shortcut { sequence: "Ctrl+0"; enabled: window.visible; onActivated: editor.setHeading(0) }
    Shortcut { sequence: "Ctrl+1"; enabled: window.visible; onActivated: editor.setHeading(1) }
    Shortcut { sequence: "Ctrl+2"; enabled: window.visible; onActivated: editor.setHeading(2) }
    Shortcut { sequence: "Ctrl+3"; enabled: window.visible; onActivated: editor.setHeading(3) }

    Shared.PanelSurface {
      id: surface

      theme: root

      ColumnLayout {
        anchors.fill: parent
        anchors.margins: root.panelMargin
        spacing: root.panelSpacing

        Shared.PanelHeader {
          id: header

          theme: root
          Layout.fillWidth: true
          glyph: "󰎞"
          title: root.overlayView === "migration" ? "Migrate" : store.config.configured ? "Notes" : "Choose a folder"

          Shared.ActionButton {
            theme: root
            text: root.folded ? "󰍜" : "󰅁"
            visible: store.config.configured && !root.overlayView
            selected: root.narrow && root.listRevealed
            onClicked: {
              if (root.narrow) { root.listRevealed = !root.listRevealed; return }
              root.sidebarCollapsed = !root.sidebarCollapsed
              store.remember(root.sidebarWidth, root.sidebarCollapsed)
            }
          }

          Shared.ActionButton {
            theme: root
            text: root.sourceMode ? "Source" : "Live"
            visible: store.config.configured && !root.overlayView
            selected: root.sourceMode
            onClicked: root.sourceMode = !root.sourceMode
          }

          Shared.ActionButton {
            theme: root
            text: store.recording ? "Stop" : "Record"
            danger: store.recording
            visible: store.config.configured && !root.overlayView
            enabled: store.ready && !store.stopping
            onClicked: root.toggleRecording()
          }

          Shared.ActionButton {
            theme: root
            text: root.overlayView ? "Done" : "New note"
            selected: !root.overlayView
            visible: store.config.configured
            enabled: store.ready
            onClicked: root.overlayView ? root.overlayView = "" : root.newNote()
          }
        }

        // Setup and migration take the whole window: they are not something to
        // do beside a note, and neither has a note to sit next to.
        Shared.SeeleFlickable {
          theme: root
          Layout.fillWidth: true
          Layout.fillHeight: true
          visible: !store.config.configured || root.overlayView === "migration"
          clip: true
          contentWidth: width
          contentHeight: overlayColumn.implicitHeight

          ColumnLayout {
            id: overlayColumn

            width: parent.width
            spacing: root.panelSpacing

            SetupView {
              Layout.fillWidth: true
              theme: root
              store: store
              visible: !store.config.configured
            }

            MigrationPanel {
              Layout.fillWidth: true
              theme: root
              store: store
              visible: root.overlayView === "migration"
            }
          }

          ScrollBar.vertical: Shared.SlimScrollBar { theme: root; popupHovered: surface.hovered }
        }

        Shared.StatusBanner {
          Layout.fillWidth: true
          theme: root
          visible: store.config.configured && !root.overlayView && store.error !== ""
          glyph: "󰀪"
          title: store.error
          detail: store.saveState === "failed" ? "Your text is still here. Fix the cause and save again." : ""

          Shared.ActionButton { theme: root; text: "Retry"; onClicked: store.retry() }
        }

        Shared.StatusBanner {
          Layout.fillWidth: true
          theme: root
          visible: store.config.configured && !root.overlayView && store.saveState === "conflict"
          tint: root.yellow
          glyph: "󰦒"
          title: "This note changed on disk while you were editing it"
          detail: "Both versions are kept whichever way you go."

          Shared.ActionButton {
            id: conflictCopy

            theme: root
            text: "Save a copy"
            selected: true
            onClicked: store.resolve("copy")

            // The safe resolution takes the keyboard as soon as the conflict
            // appears, so Tab reaches the other two from there.
            Connections {
              target: store
              function onSaveStateChanged() {
                if (store.saveState === "conflict") conflictCopy.forceActiveFocus()
              }
            }
          }

          Shared.ActionButton { theme: root; text: "Keep mine"; onClicked: store.resolve("mine") }
          Shared.ActionButton { theme: root; text: "Use theirs"; onClicked: store.resolve("theirs") }
        }

        Shared.StatusBanner {
          Layout.fillWidth: true
          theme: root
          visible: store.config.configured && !root.overlayView && store.saveState === "gone"
          tint: root.yellow
          glyph: "󰮈"
          title: "This file was moved or deleted somewhere else"
          detail: "Nothing was written back over it. Save your text as a new note."

          Shared.ActionButton {
            theme: root
            text: "Save as new note"
            onClicked: { store.path = ""; store.baseline = ""; store.saveState = "dirty"; store.flush() }
          }
        }

        Shared.StatusBanner {
          Layout.fillWidth: true
          theme: root
          visible: store.config.configured && !root.overlayView && store.ready && !store.writable
          tint: root.red
          glyph: "󰌾"
          title: "The capture folder cannot be written to"
          detail: store.config.path || ""

          Shared.ActionButton { theme: root; text: "Re-check"; onClicked: store.retry() }
        }

        Shared.StatusBanner {
          Layout.fillWidth: true
          theme: root
          visible: store.config.configured && !root.overlayView && store.config.legacy > 0
          tint: root.accent
          glyph: "󰇚"
          title: store.config.legacy + " note(s) are still in the old private library"
          detail: "Bring them into the vault whenever you like."

          Shared.ActionButton { theme: root; text: "Migrate"; onClicked: root.overlayView = "migration" }
        }

        Shared.StatusBanner {
          Layout.fillWidth: true
          theme: root
          visible: store.config.configured && !root.overlayView && store.drafts.length > 0 && store.saveState !== "conflict"
          tint: root.yellow
          glyph: "󰆓"
          title: store.drafts.length + " unsaved draft(s) were recovered from a previous session"
          detail: store.drafts.length ? store.drafts[0].path : ""

          Shared.ActionButton {
            theme: root
            text: "Open"
            onClicked: {
              var draft = store.drafts[0]
              store.select(draft.path)
              Qt.callLater(function() { editor.load(draft.text, false); store.edit(draft.text) })
            }
          }

          Shared.ActionButton {
            theme: root
            text: "Discard"
            danger: true
            onClicked: store.dropDraft(store.drafts[0].path)
          }
        }

        RowLayout {
          Layout.fillWidth: true
          Layout.fillHeight: true
          visible: store.config.configured && !root.overlayView
          spacing: root.panelMargin

          ColumnLayout {
            id: sidebar

            Layout.fillWidth: root.narrow
            Layout.preferredWidth: root.narrow ? -1 : root.sidebarWidth
            Layout.maximumWidth: root.narrow ? Number.POSITIVE_INFINITY : root.sidebarWidth
            Layout.fillHeight: true
            visible: !root.folded
            spacing: root.spaceMedium

            Shared.SearchField {
              id: search

              Layout.fillWidth: true
              theme: root
              placeholderText: "Search this folder…"

              Keys.onDownPressed: { noteList.forceActiveFocus(); if (noteList.currentIndex < 0) noteList.currentIndex = 0; else noteList.incrementCurrentIndex() }
              Keys.onReturnPressed: { root.chooseRow(noteList.currentIndex >= 0 ? noteList.currentIndex : 0); editor.focusBody() }
              onTextChanged: if (noteList.count) noteList.currentIndex = 0
            }

            // Notes and Trash are two views of one folder rather than two
            // errands, so they are a well with the one being read lit inside
            // it instead of two buttons competing for the same width.
            Shared.SegmentWell {
              theme: root
              Layout.fillWidth: true
              implicitHeight: root.controlHeight

              Repeater {
                model: [{ label: "Notes", trash: false }, { label: "Trash", trash: true }]

                Shared.Segment {
                  id: libraryView

                  required property var modelData

                  theme: root
                  width: parent.width / 2
                  selected: store.trashView === libraryView.modelData.trash
                  hovered: libraryViewMouse.containsMouse
                  pressed: libraryViewMouse.pressed

                  Text {
                    anchors.centerIn: parent
                    text: libraryView.modelData.label
                    color: libraryView.selected ? root.text : root.subtext
                    font.family: root.fontFamily
                    font.pixelSize: root.textLabel
                    font.weight: libraryView.selected ? root.weightStrong : root.weightMedium

                    Behavior on color { ColorAnimation { duration: root.durationFast } }
                  }

                  MouseArea {
                    id: libraryViewMouse

                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: store.setTrashView(libraryView.modelData.trash)
                  }
                }
              }
            }

            Shared.SeeleListView {
              id: noteList

              theme: root
              Layout.fillWidth: true
              Layout.fillHeight: true
              clip: true
              spacing: root.spaceTight
              // Ordering follows the disk, and the disk only moves when a note
              // is actually written, so the row under the pointer stays where
              // it was while the note above it is being typed into.
              model: root.rows
              currentIndex: -1
              keyNavigationEnabled: true
              highlightFollowsCurrentItem: true

              Keys.onReturnPressed: { root.chooseRow(noteList.currentIndex); editor.focusBody() }
              Keys.onEscapePressed: editor.focusBody()

              delegate: Rectangle {
                id: row

                required property var modelData
                required property int index
                readonly property bool chosen: store.trashView
                  ? store.trashId === row.modelData.id
                  : store.path === row.modelData.path

                width: noteList.width
                height: rowText.implicitHeight + root.cardPadding * 2
                radius: root.radius
                color: rowMouse.pressed
                  ? root.pressColor
                  : row.chosen ? root.selectedColor : root.cardColor
                antialiasing: true

                Behavior on color { ColorAnimation { duration: root.durationFast } }

                Shared.CardEdge { theme: root }
                Shared.HoverWash { theme: root; hovered: rowHover.hovered }
                HoverHandler { id: rowHover }

                Rectangle {
                  visible: noteList.activeFocus && noteList.currentIndex === row.index
                  anchors.fill: parent
                  radius: parent.radius
                  color: "transparent"
                  border.width: 1
                  border.color: root.accent
                  antialiasing: true
                }

                Column {
                  id: rowText

                  anchors.left: parent.left
                  anchors.right: parent.right
                  anchors.margins: root.cardPadding
                  anchors.verticalCenter: parent.verticalCenter
                  spacing: root.spaceTight

                  Text {
                    width: parent.width
                    text: Notes.label(row.modelData)
                    color: root.text
                    font.family: root.fontFamily
                    font.pixelSize: root.textBody
                    font.weight: root.weightStrong
                    elide: Text.ElideRight
                  }

                  Text {
                    width: parent.width
                    visible: !!row.modelData.excerpt
                    text: row.modelData.excerpt
                    color: root.subtext
                    font.family: root.fontFamily
                    font.pixelSize: root.textCaption
                    elide: Text.ElideRight
                  }

                  Text {
                    width: parent.width
                    text: Notes.when(row.modelData.updated)
                      + (row.modelData.audio ? " · 󰍬 " + row.modelData.audio : "")
                    color: root.overlay
                    font.family: root.fontFamily
                    font.pixelSize: root.textCaption
                  }
                }

                MouseArea {
                  id: rowMouse

                  anchors.fill: parent
                  cursorShape: Qt.PointingHandCursor
                  onClicked: {
                    noteList.currentIndex = row.index
                    root.chooseRow(row.index)
                  }
                }
              }

              Shared.EmptyState {
                anchors.fill: parent
                theme: root
                visible: noteList.count === 0
                glyph: !store.ready ? "󰅐" : search.text ? "󰍉" : store.trashView ? "󰩹" : "󰎞"
                title: !store.ready
                  ? "Connecting…"
                  : search.text
                    ? "Nothing matches"
                    : store.trashView ? "The trash is empty" : "No notes here yet"
                detail: !store.ready || search.text || store.trashView ? "" : "Ctrl+N starts one."
              }

              ScrollBar.vertical: Shared.SlimScrollBar { theme: root; popupHovered: surface.hovered }
            }
          }

          // The divider is the handle: dragging it sizes the list, and the
          // width it settles on is remembered.
          Item {
            Layout.fillHeight: true
            implicitWidth: root.spaceTight
            visible: !root.folded && !root.narrow

            Rectangle {
              anchors.horizontalCenter: parent.horizontalCenter
              width: 1
              height: parent.height
              color: grabHover.hovered || grab.drag.active ? root.accent : root.separatorColor

              Behavior on color { ColorAnimation { duration: root.durationFast } }
            }

            HoverHandler { id: grabHover; cursorShape: Qt.SplitHCursor }

            MouseArea {
              id: grab

              anchors.fill: parent
              anchors.margins: -root.spaceTight
              cursorShape: Qt.SplitHCursor
              onPositionChanged: mouse => {
                if (!pressed) return
                root.sidebarWidth = Math.max(
                  root.notesSidebarMinimum,
                  Math.min(root.notesSidebarMaximum, root.sidebarWidth + mouse.x))
              }
              onReleased: store.remember(root.sidebarWidth, root.sidebarCollapsed)
            }
          }

          ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !(root.narrow && root.listRevealed)
            spacing: root.spaceMedium

            RowLayout {
              Layout.fillWidth: true
              spacing: root.spaceMedium

              Text {
                Layout.fillWidth: true
                text: store.trashView && store.trashId
                  ? Notes.label(store.trash.find(function(item) { return item.id === store.trashId }))
                  : store.path
                    ? Notes.label(store.note)
                    : "New note"
                elide: Text.ElideRight
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: root.textTitle
                font.weight: root.weightStrong
              }

              Shared.ActionButton {
                theme: root
                text: "Obsidian"
                visible: !!store.path && !store.trashView
                onClicked: root.openInObsidian()
              }

              Shared.ActionButton {
                theme: root
                text: store.trashView ? "Restore" : "Trash"
                danger: !store.trashView
                enabled: store.trashView
                  ? !!store.trashId
                  : (!!store.path && !store.recording && store.ready)
                onClicked: store.trashView ? store.restoreNote(store.trashId) : store.trashNote()
              }
            }

            RowLayout {
              Layout.fillWidth: true
              spacing: root.spaceSmall

              Text {
                Layout.fillWidth: true
                text: store.trashView
                  ? "In the trash · restore it to edit"
                  : Notes.status(store.saveState, store.note ? store.note.updated : 0)
                  + (store.path ? " · " + store.path : "")
                elide: Text.ElideMiddle
                color: store.saveState === "failed" || store.saveState === "gone"
                  ? root.red
                  : store.saveState === "conflict" ? root.yellow : root.overlay
                font.family: root.fontFamily
                font.pixelSize: root.textCaption
              }

              Text {
                text: root.hintsShown ? "F1 hides shortcuts" : "F1 shortcuts"
                color: root.overlay
                font.family: root.fontFamily
                font.pixelSize: root.textCaption

                MouseArea {
                  anchors.fill: parent
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.hintsShown = !root.hintsShown
                }
              }
            }

            Rectangle {
              Layout.fillWidth: true
              Layout.preferredHeight: hints.implicitHeight + root.cardPadding * 2
              visible: root.hintsShown
              radius: root.radius
              color: root.cardColor
              antialiasing: true

              Shared.CardEdge { theme: root }

              Flow {
                id: hints

                anchors.fill: parent
                anchors.margins: root.cardPadding
                spacing: root.spaceLarge

                Repeater {
                  model: [
                    "Ctrl+N new", "Ctrl+F search", "Ctrl+L list", "Esc editor",
                    "Ctrl+B bold", "Ctrl+I italic", "Ctrl+E code", "Ctrl+K link",
                    "Ctrl+1…3 heading", "Ctrl+Shift+X task",
                    "Ctrl+R record", "Ctrl+P play", "Alt+←/→ seek",
                    "Ctrl+Shift+Del trash", "Ctrl+Shift+T trash view",
                    "Ctrl+Shift+O Obsidian",
                    "Ctrl+Shift+M source", "Ctrl+\\ list",
                    "Ctrl+S save · resolve",
                  ]

                  Text {
                    required property string modelData

                    text: modelData
                    color: root.subtext
                    font.family: root.fontFamily
                    font.pixelSize: root.textCaption
                  }
                }
              }
            }

            MarkdownEditor {
              id: editor

              theme: root
              Layout.fillWidth: true
              Layout.fillHeight: true
              sourceMode: root.sourceMode
              readOnly: store.trashView || !store.writable
              popupHovered: surface.hovered
              placeholder: store.trashView
                ? "Restore this note to edit it"
                : "Start writing… Markdown formats as you type."

              onEdited: text => store.edit(text)
            }

            AudioStrip {
              id: audio

              theme: root
              Layout.fillWidth: true
              store: store
              player: root.player
              popupHovered: surface.hovered

              onRemoveRequested: name => { editor.removeEmbed(name); store.flush() }
            }

            Text {
              Layout.fillWidth: true
              visible: store.recordingError !== "" && !store.recording
              text: store.recordingError
              color: root.yellow
              font.family: root.fontFamily
              font.pixelSize: root.textCaption
              wrapMode: Text.Wrap
            }

            Text {
              Layout.fillWidth: true
              visible: playback.status === Loader.Error
              text: "Audio playback is unavailable. Writing and recording still work."
              color: root.red
              font.family: root.fontFamily
              font.pixelSize: root.textCaption
              wrapMode: Text.Wrap
            }

            Text {
              Layout.fillWidth: true
              visible: store.warning !== ""
              text: store.warning
              color: root.yellow
              font.family: root.fontFamily
              font.pixelSize: root.textCaption
              wrapMode: Text.Wrap
            }
          }
        }
      }
    }
  }
}
