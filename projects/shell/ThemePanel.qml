pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// Choosing a palette is a visual decision, so the panel leads with the
// palette in use rather than with its name: a small desktop drawn in the
// highlighted preset follows the keyboard and the pointer before anything is
// applied. Below it, each family is one row and each variant one tile, and a
// tile is itself a sample: its name in its own text colour on its own
// background. The catalog, publication and every reload belong to
// `seele-theme`; grouping and movement belong to `qml-core`.
FocusScope {
  id: panel
  required property var theme
  required property var store
  property bool popupHovered: false
  // Bounds the whole panel, so the window can hand it what its output has room
  // for; the grid takes whatever the preview and the controls leave.
  property real maximumHeight: theme.themesMaximumHeight
  readonly property string hint: "Arrows move · Enter applies · Type to search · Escape closes"
  readonly property var preview: store.preview
  readonly property bool previewApplied: !!preview && preview.id === store.current
  readonly property int columns: 4
  readonly property real familyWidth: 84
  readonly property real tileGap: theme.spaceSmall
  readonly property real tileWidth: Math.floor((width - familyWidth - theme.spaceMedium - tileGap * (columns - 1)) / columns)
  readonly property real tileHeight: 54
  // The ring a highlighted tile wears sits outside the tile, so the grid keeps
  // that much room on every side for it to be drawn in.
  readonly property real ringReach: 3
  readonly property real gridHeight: Math.max(tileHeight + ringReach * 2, maximumHeight - chrome.height - content.spacing)
  // Where the pointer last really was, in panel coordinates, or (-1, -1) before
  // it has been seen since the panel opened.
  property point pointer: Qt.point(-1, -1)
  implicitHeight: content.implicitHeight

  Component.onCompleted: store.columns = columns
  // A pointer resting where the panel opens has not chosen anything.
  Connections {
    target: panel.store
    ignoreUnknownSignals: true
    function onPanelOpenChanged() { panel.pointer = Qt.point(-1, -1) }
  }

  // Only a pointer that actually moved expresses a choice. Tiles are created
  // and scrolled under a resting pointer whenever the search changes or the
  // keyboard moves, and Qt reports that as the pointer entering them; taking
  // it as intent would let a mouse left over the grid take the preview, and
  // then Enter, away from the keyboard. The first event only records where
  // the pointer is.
  function pointerMoved(item, x, y) {
    var at = item.mapToItem(panel, x, y)
    var seen = panel.pointer.x >= 0
    var moved = seen && (at.x !== panel.pointer.x || at.y !== panel.pointer.y)
    panel.pointer = at
    return moved
  }

  function handleKey(event) {
    if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
    var direction = { [Qt.Key_Left]: "left", [Qt.Key_H]: "left", [Qt.Key_Right]: "right", [Qt.Key_L]: "right",
      [Qt.Key_Up]: "up", [Qt.Key_K]: "up", [Qt.Key_Down]: "down", [Qt.Key_J]: "down" }[event.key]
    if (direction) { store.step(direction); event.accepted = true }
    else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { store.applyFocused(); event.accepted = true }
    else if (event.key === Qt.Key_R) { store.refresh(); event.accepted = true }
    else if (event.key === Qt.Key_Slash) { search.forceActiveFocus(); event.accepted = true }
  }
  // Keeps the highlighted tile, and its ring, inside the scrolled grid.
  function reveal(item) {
    var top = item.mapToItem(gridContent, 0, 0).y - ringReach
    var bottom = top + item.height + ringReach * 2
    if (top < grid.contentY) grid.contentY = Math.max(0, top)
    else if (bottom > grid.contentY + grid.height) grid.contentY = bottom - grid.height
  }

  // A line of the scene's terminal, and one of its status dots.
  component SceneLine: Text {
    textFormat: Text.StyledText
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
    elide: Text.ElideRight
  }
  component SceneDot: Rectangle {
    width: 6
    height: 6
    radius: 3
    Behavior on color { ColorAnimation { duration: panel.theme.durationNormal } }
  }

  // A preset shown as the thing it themes: the bar, a focused terminal carrying
  // the accent on its border the way the compositor draws one, a notification
  // on the surface colour with an accent action, and a slider in the accent.
  // Every colour is one of the preset's own roles; every measurement is
  // material geometry for this one scene.
  component Scene: Rectangle {
    id: scene
    required property var preset
    readonly property color baseColor: scene.preset.base
    readonly property color mantleColor: scene.preset.mantle
    readonly property color surfaceColor: scene.preset.surface
    readonly property color overlayColor: scene.preset.overlay
    readonly property color textColor: scene.preset.text
    readonly property color subtextColor: scene.preset.subtext
    readonly property color accentColor: scene.preset.accent
    // One run of terminal text in one role. The roles are native-validated
    // `#rrggbb`, and the words are fixed here, so nothing outside the panel
    // reaches the rich-text parser.
    function tint(role, words) {
      return "<font color=\"" + scene.preset[role] + "\">" + words + "</font>"
    }

    implicitHeight: 156
    radius: panel.theme.radius
    color: scene.baseColor
    border.width: 1
    border.color: panel.theme.alpha(scene.overlayColor, 0.6)
    antialiasing: true
    clip: true
    Behavior on color { ColorAnimation { duration: panel.theme.durationNormal } }

    Rectangle {
      id: bar
      x: scene.border.width
      y: scene.border.width
      width: parent.width - scene.border.width * 2
      height: 22
      topLeftRadius: scene.radius - scene.border.width
      topRightRadius: scene.radius - scene.border.width
      color: scene.mantleColor
      Behavior on color { ColorAnimation { duration: panel.theme.durationNormal } }
      Row {
        x: 10
        anchors.verticalCenter: parent.verticalCenter
        spacing: 5
        Rectangle {
          width: 16
          height: 6
          radius: 3
          color: scene.accentColor
          Behavior on color { ColorAnimation { duration: panel.theme.durationNormal } }
        }
        Repeater {
          model: 3
          SceneDot { color: scene.overlayColor }
        }
      }
      Text {
        anchors.centerIn: parent
        text: "Wed 24  12:00"
        color: scene.textColor
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
        font.weight: panel.theme.weightMedium
      }
      Row {
        anchors { right: parent.right; rightMargin: 10; verticalCenter: parent.verticalCenter }
        spacing: 6
        SceneDot { anchors.verticalCenter: parent.verticalCenter; color: scene.preset.green }
        Text {
          text: "84%"
          color: scene.subtextColor
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
      }
    }

    Rectangle {
      id: terminal
      x: 12
      y: bar.height + 12
      width: Math.round(scene.width * 0.6)
      height: scene.height - y - 12
      radius: 6
      color: scene.baseColor
      border.width: 2
      border.color: scene.accentColor
      Behavior on border.color { ColorAnimation { duration: panel.theme.durationNormal } }
      Column {
        x: 10
        y: 9
        width: parent.width - 20
        spacing: 3
        SceneLine { width: parent.width; text: scene.tint("accent", "~/seele") + " " + scene.tint("overlay", "on") + " " + scene.tint("green", "main") }
        SceneLine { width: parent.width; text: scene.tint("accent", "❯") + " " + scene.tint("text", "seele-theme set " + (scene.preset.id || "")) }
        SceneLine { width: parent.width; text: scene.tint("green", "✓") + " " + scene.tint("subtext", "published · 5 apps reloaded") }
        SceneLine { width: parent.width; text: scene.tint("yellow", "!") + " " + scene.tint("subtext", "Ghostty: Reload Configuration") }
        SceneLine { width: parent.width; text: scene.tint("red", "✗") + " " + scene.tint("text", "one unit failed") + " " + scene.tint("overlay", "# 2m ago") }
        Row {
          spacing: 5
          SceneLine { text: scene.tint("accent", "❯") }
          Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 6
            height: 11
            color: scene.textColor
            Behavior on color { ColorAnimation { duration: panel.theme.durationNormal } }
          }
        }
      }
    }

    Rectangle {
      id: card
      x: terminal.x + terminal.width + 10
      y: terminal.y
      width: scene.width - x - 12
      height: 72
      radius: 6
      color: scene.surfaceColor
      border.width: 1
      border.color: panel.theme.alpha(scene.overlayColor, 0.5)
      Behavior on color { ColorAnimation { duration: panel.theme.durationNormal } }
      Column {
        x: 9
        y: 8
        width: parent.width - 18
        spacing: 2
        Text {
          width: parent.width
          text: "Build finished"
          elide: Text.ElideRight
          color: scene.textColor
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textLabel
          font.weight: panel.theme.weightStrong
        }
        Text {
          width: parent.width
          text: "nerv · just now"
          elide: Text.ElideRight
          color: scene.subtextColor
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
      }
      Rectangle {
        anchors { right: parent.right; bottom: parent.bottom; margins: 8 }
        width: openLabel.implicitWidth + 14
        height: 16
        radius: 4
        color: scene.accentColor
        Behavior on color { ColorAnimation { duration: panel.theme.durationNormal } }
        Text {
          id: openLabel
          anchors.centerIn: parent
          text: "Open"
          color: scene.baseColor
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
          font.weight: panel.theme.weightStrong
        }
      }
    }

    Item {
      x: card.x
      y: card.y + card.height + 12
      width: card.width
      height: 12
      Rectangle {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width
        height: 4
        radius: 2
        color: scene.surfaceColor
      }
      Rectangle {
        id: fill
        anchors.verticalCenter: parent.verticalCenter
        width: Math.round(parent.width * 0.62)
        height: 4
        radius: 2
        color: scene.accentColor
      }
      Rectangle {
        x: fill.width - width / 2
        anchors.verticalCenter: parent.verticalCenter
        width: 12
        height: 12
        radius: 6
        color: scene.textColor
      }
    }
  }

  // One preset as a sample of itself. The highlighted tile wears the shell's
  // own accent as a ring, the way the keyboard is shown everywhere else; the
  // applied one carries a check in its own accent.
  component Tile: Item {
    id: tile
    required property var entry
    readonly property bool highlighted: panel.store.focusedId === tile.entry.id
    readonly property color baseColor: tile.entry.base
    readonly property color overlayColor: tile.entry.overlay
    readonly property color textColor: tile.entry.text
    readonly property color accentColor: tile.entry.accent
    width: panel.tileWidth
    height: panel.tileHeight
    objectName: "tile-" + tile.entry.id

    onHighlightedChanged: if (tile.highlighted) Qt.callLater(function() { panel.reveal(tile) })
    Component.onCompleted: if (tile.highlighted) Qt.callLater(function() { panel.reveal(tile) })

    Rectangle {
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
      Text {
        x: 9
        y: 8
        width: parent.width - 18 - (check.visible ? check.width + 4 : 0)
        text: tile.entry.variant
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: tile.textColor
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textLabel
        font.weight: panel.theme.weightStrong
      }
      Row {
        x: 9
        anchors { bottom: parent.bottom; bottomMargin: 9 }
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
        visible: !!tile.entry.current
        anchors { top: parent.top; right: parent.right; margins: 6 }
        width: 15
        height: 15
        radius: 7.5
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
    MouseArea {
      id: tileMouse
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onPositionChanged: mouse => { if (panel.pointerMoved(tileMouse, mouse.x, mouse.y)) panel.store.highlight(tile.entry.id) }
      onClicked: {
        panel.store.highlight(tile.entry.id)
        if (!tile.entry.current) panel.store.apply(tile.entry.id)
      }
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

      Column {
        id: hero
        objectName: "hero"
        width: parent.width
        visible: !!panel.preview
        spacing: panel.theme.spaceMedium
        // The scene exists only while there is a preset to draw: a search that
        // matches nothing leaves no palette, and an empty one is not a scene.
        Loader {
          width: parent.width
          active: !!panel.preview
          sourceComponent: Scene {
            objectName: "scene"
            preset: panel.preview
          }
        }
        Item {
          width: parent.width
          height: Math.max(heroTitle.implicitHeight, applyButton.implicitHeight)
          Column {
            id: heroTitle
            anchors { left: parent.left; right: applyButton.left; rightMargin: panel.theme.spaceMedium; verticalCenter: parent.verticalCenter }
            spacing: 1
            Text {
              objectName: "previewName"
              width: parent.width
              text: panel.preview ? panel.preview.name : ""
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: panel.theme.text
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textSubhead
              font.weight: panel.theme.weightStrong
            }
            Text {
              objectName: "previewCaption"
              width: parent.width
              text: !panel.preview ? ""
                : panel.previewApplied ? panel.preview.modeLabel + " · Applied now"
                : panel.preview.modeLabel + (panel.store.currentName !== "" ? " · Currently " + panel.store.currentName : "")
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: panel.theme.subtext
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }
          }
          Shared.ActionButton {
            id: applyButton
            objectName: "apply"
            theme: panel.theme
            anchors { right: parent.right; verticalCenter: parent.verticalCenter }
            // Not the selected style: its accent-on-accent label falls near 3:1
            // under light presets with pale accents, and this is the one action
            // the panel exists for.
            enabled: !panel.store.busy && !!panel.preview
            text: panel.preview && panel.store.applying === panel.preview.id ? "Applying…"
              : panel.previewApplied ? "Reapply" : "Apply"
            onClicked: panel.store.applyFocused()
          }
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
        title: "Applied; some applications keep the old colors"
        detail: panel.store.reloadPending
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
          Keys.onReturnPressed: event => { panel.store.applyFocused(); event.accepted = true }
          Keys.onEnterPressed: event => { panel.store.applyFocused(); event.accepted = true }
        }
        Shared.SegmentWell {
          id: modes
          theme: panel.theme
          width: 162
          height: search.height
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "modeAll"; text: "All"; selected: panel.store.mode === "all"; onClicked: panel.store.mode = "all" }
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "modeDark"; text: "Dark"; selected: panel.store.mode === "dark"; onClicked: panel.store.mode = "dark" }
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "modeLight"; text: "Light"; selected: panel.store.mode === "light"; onClicked: panel.store.mode = "light" }
        }
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
  }
  Keys.onPressed: event => panel.handleKey(event)
}
