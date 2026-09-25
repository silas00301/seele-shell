pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// The curated palettes, one family to a row and one variant to a tile, each
// tile a sample of itself: its name, and its mode in its quiet text colour,
// on its own background. Moving to a tile switches the desktop to it at once,
// so the shell repainting around the panel is the preview, and the theme that
// was applied when the panel opened stays one step away. The catalog,
// publication and every reload belong to `seele-theme`; grouping and movement
// belong to `qml-core`.
FocusScope {
  id: panel
  required property var theme
  required property var store
  property bool popupHovered: false
  // Bounds the whole panel, so the window can hand it what its output has room
  // for; the grid takes whatever the header, the controls and the footer leave.
  property real maximumHeight: theme.themesMaximumHeight
  readonly property string hint: "Arrows switch · Type to search · Enter picks · Escape closes"
  readonly property int columns: 4
  readonly property real familyWidth: 96
  readonly property real tileGap: theme.spaceMedium
  readonly property real tileWidth: Math.floor((width - familyWidth - theme.spaceMedium - tileGap * (columns - 1)) / columns)
  readonly property real tileHeight: 64
  // The ring a keyboard tile wears sits outside the tile, so the grid keeps
  // that much room on every side for it to be drawn in.
  readonly property real ringReach: 3
  readonly property real gridHeight: Math.max(tileHeight + ringReach * 2,
    maximumHeight - chrome.height - footer.height - content.spacing * 2)
  readonly property bool canGoBack: store.opened !== "" && store.opened !== store.current && store.desired !== store.opened
  implicitHeight: content.implicitHeight
  signal closeRequested()

  Component.onCompleted: store.columns = columns

  function handleKey(event) {
    if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
    var direction = { [Qt.Key_Left]: "left", [Qt.Key_H]: "left", [Qt.Key_Right]: "right", [Qt.Key_L]: "right",
      [Qt.Key_Up]: "up", [Qt.Key_K]: "up", [Qt.Key_Down]: "down", [Qt.Key_J]: "down" }[event.key]
    if (direction) { store.step(direction); event.accepted = true }
    else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { pick(); event.accepted = true }
    else if (event.key === Qt.Key_Escape) { closeRequested(); event.accepted = true }
    else if (event.key === Qt.Key_R) { store.refresh(); event.accepted = true }
    else if (event.key === Qt.Key_Slash) { search.forceActiveFocus(); event.accepted = true }
  }
  // Enter keeps the tile the ring is on — which a search may have moved it to
  // without switching anything — and closes the panel.
  function pick() {
    store.choose(store.focusedId, true)
    closeRequested()
  }
  // Keeps the ringed tile inside the scrolled grid.
  function reveal(item) {
    var top = item.mapToItem(gridContent, 0, 0).y - ringReach
    var bottom = top + item.height + ringReach * 2
    if (top < grid.contentY) grid.contentY = Math.max(0, top)
    else if (bottom > grid.contentY + grid.height) grid.contentY = bottom - grid.height
  }

  // One preset as a sample of itself. The keyboard's tile wears the shell's own
  // accent as a ring, the way the keyboard is shown everywhere else; the
  // applied one carries a check in its own accent.
  component Tile: Item {
    id: tile
    required property var entry
    readonly property bool highlighted: panel.store.focusedId === tile.entry.id
    readonly property bool applied: !!tile.entry.current
    readonly property color baseColor: tile.entry.base
    readonly property color overlayColor: tile.entry.overlay
    readonly property color textColor: tile.entry.text
    readonly property color subtextColor: tile.entry.subtext
    readonly property color accentColor: tile.entry.accent
    width: panel.tileWidth
    height: panel.tileHeight
    objectName: "tile-" + tile.entry.id

    onHighlightedChanged: if (tile.highlighted) Qt.callLater(function() { panel.reveal(tile) })
    Component.onCompleted: if (tile.highlighted) Qt.callLater(function() { panel.reveal(tile) })

    Rectangle {
      objectName: "ring"
      anchors { fill: parent; margins: -panel.ringReach }
      radius: panel.theme.radius + panel.ringReach
      color: panel.theme.alpha(panel.theme.accent, 0)
      border.width: tile.highlighted ? 2 : 1
      border.color: tile.highlighted ? panel.theme.accent
        : tileMouse.containsMouse ? panel.theme.alpha(panel.theme.text, 0.28)
        : panel.theme.alpha(panel.theme.text, 0)
      antialiasing: true
      Behavior on border.color { ColorAnimation { duration: panel.theme.durationFast } }
    }
    Rectangle {
      anchors.fill: parent
      radius: panel.theme.radius
      color: tile.baseColor
      border.width: 1
      border.color: panel.theme.alpha(tile.overlayColor, 0.7)
      antialiasing: true
      clip: true
      Column {
        x: 10
        y: 9
        width: parent.width - 20 - (check.visible ? check.width + 4 : 0)
        spacing: 1
        Text {
          width: parent.width
          text: tile.entry.variant
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: tile.textColor
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textBody
          font.weight: panel.theme.weightStrong
        }
        Text {
          width: parent.width
          visible: text !== ""
          text: tile.entry.detail || ""
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: tile.subtextColor
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
      }
      Row {
        x: 10
        anchors { bottom: parent.bottom; bottomMargin: 10 }
        spacing: 4
        Repeater {
          model: [tile.entry.accent, tile.entry.red, tile.entry.green, tile.entry.yellow]
          Rectangle {
            required property var modelData
            width: 7
            height: 7
            radius: 3.5
            color: modelData
          }
        }
      }
      Rectangle {
        id: check
        objectName: "appliedMark"
        visible: tile.applied
        anchors { top: parent.top; right: parent.right; margins: 7 }
        width: 16
        height: 16
        radius: 8
        color: tile.accentColor
        Text {
          anchors.centerIn: parent
          text: "󰄬"
          color: tile.baseColor
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
      }
    }
    // Hovering only lights the tile; switching the whole desktop is a click.
    MouseArea {
      id: tileMouse
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: panel.store.choose(tile.entry.id, true)
    }
  }

  Column {
    id: content
    width: parent.width
    spacing: panel.theme.panelSpacing

    Column {
      id: chrome
      width: parent.width
      spacing: panel.theme.panelSpacing

      Shared.PanelHeader {
        width: parent.width
        theme: panel.theme
        glyph: "󰔎"
        title: "Themes"
        detail: panel.hint
        Shared.GlyphButton { objectName: "close"; theme: panel.theme; glyph: "󰅖"; text: "Close themes"; onClicked: panel.closeRequested() }
      }

      Shared.StatusBanner {
        theme: panel.theme
        width: parent.width
        visible: panel.store.error !== ""
        glyph: "󰀦"
        tint: panel.theme.yellow
        title: "The theme catalog is unavailable"
        detail: panel.store.error
        Shared.ActionButton {
          theme: panel.theme
          objectName: "retry"
          text: "Retry"
          enabled: !panel.store.busy
          onClicked: panel.store.refresh()
        }
      }

      Row {
        width: parent.width
        spacing: panel.theme.spaceMedium
        Shared.SearchField {
          id: search
          objectName: "search"
          theme: panel.theme
          width: parent.width - modes.width - parent.spacing
          focus: true
          placeholderText: "Search presets"
          text: panel.store.query
          onTextEdited: panel.store.query = text
          Keys.onUpPressed: event => { panel.store.step("up"); event.accepted = true }
          Keys.onDownPressed: event => { panel.store.step("down"); event.accepted = true }
          // Left and right belong to the caret while there is text to move in.
          Keys.onLeftPressed: event => { event.accepted = text === ""; if (event.accepted) panel.store.step("left") }
          Keys.onRightPressed: event => { event.accepted = text === ""; if (event.accepted) panel.store.step("right") }
          Keys.onReturnPressed: event => { panel.pick(); event.accepted = true }
          Keys.onEnterPressed: event => { panel.pick(); event.accepted = true }
          Keys.onEscapePressed: event => { panel.closeRequested(); event.accepted = true }
        }
        Shared.SegmentWell {
          id: modes
          theme: panel.theme
          width: 180
          height: search.height
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "modeAll"; text: "All"; selected: panel.store.mode === "all"; onClicked: panel.store.mode = "all" }
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "modeDark"; text: "Dark"; selected: panel.store.mode === "dark"; onClicked: panel.store.mode = "dark" }
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "modeLight"; text: "Light"; selected: panel.store.mode === "light"; onClicked: panel.store.mode = "light" }
        }
      }

      Text {
        width: parent.width
        visible: panel.store.actionError !== ""
        text: panel.store.actionError
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: panel.theme.red
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textBody
      }
      // The theme is saved by the time this is read, so what is left is named
      // as something to reload rather than as a switch that failed.
      Shared.StatusBanner {
        theme: panel.theme
        width: parent.width
        visible: panel.store.reloadPending !== ""
        glyph: "󰑐"
        tint: panel.theme.yellow
        title: "Some applications keep the old colors"
        detail: panel.store.reloadPending
      }

      Shared.EmptyState {
        theme: panel.theme
        width: parent.width
        visible: panel.store.layout.count === 0
        glyph: panel.store.filtered ? "󰍉" : "󰔎"
        title: panel.store.filtered ? "No matching preset" : "No themes available"
        detail: panel.store.filtered
          ? "Nothing in the catalog matches this search and mode."
          : "The catalog is generated during a rebuild. Refresh once the desktop has one."
        Shared.ActionButton {
          theme: panel.theme
          objectName: "emptyAction"
          text: panel.store.filtered ? "Show all presets" : "Refresh"
          enabled: panel.store.filtered || !panel.store.busy
          onClicked: {
            if (panel.store.filtered) panel.store.resetFilters()
            else panel.store.refresh()
          }
        }
      }
    }

    Shared.SeeleFlickable {
      id: grid
      objectName: "grid"
      theme: panel.theme
      width: parent.width
      visible: panel.store.layout.count > 0
      height: visible ? Math.min(gridContent.implicitHeight, panel.gridHeight) : 0
      contentHeight: gridContent.implicitHeight
      clip: true
      ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }
      Column {
        id: gridContent
        width: grid.width
        topPadding: panel.ringReach
        bottomPadding: panel.ringReach
        spacing: panel.theme.spaceMedium + panel.ringReach
        Repeater {
          model: panel.store.layout.rows
          Row {
            id: familyRow
            required property var modelData
            spacing: panel.theme.spaceMedium
            Text {
              objectName: "family"
              width: panel.familyWidth
              height: panel.tileHeight
              text: familyRow.modelData.family
              textFormat: Text.PlainText
              elide: Text.ElideRight
              verticalAlignment: Text.AlignVCenter
              color: panel.theme.subtext
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textLabel
              font.weight: panel.theme.weightMedium
            }
            Row {
              spacing: panel.tileGap
              Repeater {
                model: familyRow.modelData.members
                Tile {
                  required property var modelData
                  entry: modelData
                }
              }
            }
          }
        }
      }
    }

    // What the desktop is wearing, and the one step back to where it was.
    Item {
      id: footer
      width: parent.width
      height: Math.max(status.implicitHeight, back.visible ? back.implicitHeight : 0)
      Text {
        id: status
        objectName: "status"
        anchors { left: parent.left; right: back.visible ? back.left : parent.right; rightMargin: panel.theme.spaceMedium; verticalCenter: parent.verticalCenter }
        text: panel.store.switching ? "Switching to " + panel.store.nameOf(panel.store.desired) + "…"
          : panel.store.currentName !== "" ? panel.store.currentName + " is applied" : ""
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }
      Shared.ActionButton {
        id: back
        objectName: "back"
        theme: panel.theme
        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
        visible: panel.canGoBack
        text: "Back to " + panel.store.openedName
        onClicked: panel.store.revert()
      }
    }
  }
  Keys.onPressed: event => panel.handleKey(event)
}
