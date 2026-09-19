pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// The curated palettes, previewed in their own colours, and the one thing that
// can be done with one: apply it. The catalog, publication and every reload
// belong to `seele-theme`; the panel decides nothing about what a theme is and
// writes nothing itself.
FocusScope {
  id: panel
  required property var theme
  required property var store
  property bool popupHovered: false
  // Bounds the whole panel, so the window can hand it what its output has room
  // for; the list takes whatever the search, the heading and any banner leave.
  property real maximumHeight: theme.themesMaximumHeight
  readonly property real listHeight: Math.max(theme.detailRowHeight, maximumHeight - chrome.height - content.spacing)
  readonly property string hint: "J / K moves · Enter applies · R refreshes · Escape closes"
  implicitHeight: content.implicitHeight

  function applyRow(index) {
    if (index < 0 || index >= panel.store.model.count) return
    panel.store.apply(panel.store.model.get(index).entry.id)
  }
  function handleKey(event) {
    if (event.modifiers & Qt.ControlModifier) return
    if (event.key === Qt.Key_J || event.key === Qt.Key_Down) { presets.incrementCurrentIndex(); event.accepted = true }
    else if (event.key === Qt.Key_K || event.key === Qt.Key_Up) { presets.decrementCurrentIndex(); event.accepted = true }
    else if (event.key === Qt.Key_R) { panel.store.refresh(); event.accepted = true }
    else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { panel.applyRow(presets.currentIndex); event.accepted = true }
  }

  // A palette shown as the thing it themes: a bar over a window on a desktop,
  // drawn in the preset's own colours rather than described in hex. Every
  // measurement here is material geometry for this one mark.
  component Preview: Rectangle {
    id: preview
    required property var entry
    readonly property color baseColor: preview.entry.base
    readonly property color mantleColor: preview.entry.mantle
    readonly property color surfaceColor: preview.entry.surface
    readonly property color overlayColor: preview.entry.overlay
    readonly property color textColor: preview.entry.text
    readonly property color subtextColor: preview.entry.subtext
    readonly property color accentColor: preview.entry.accent

    implicitWidth: 64
    implicitHeight: 42
    radius: panel.theme.radiusSmall
    color: preview.baseColor
    border.width: 1
    border.color: panel.theme.alpha(preview.overlayColor, 0.55)
    antialiasing: true
    clip: true

    // The bar, with one lit entry in it.
    Rectangle {
      width: parent.width
      height: 8
      color: preview.mantleColor
      Rectangle {
        x: parent.width - width - 3
        anchors.verticalCenter: parent.verticalCenter
        width: 8
        height: 3
        radius: 1.5
        color: preview.accentColor
      }
    }
    // A window with two lines of text in it.
    Rectangle {
      x: 5
      y: 14
      width: 34
      height: 22
      radius: 2
      color: preview.surfaceColor
      border.width: 1
      border.color: preview.accentColor
      Column {
        x: 5
        y: 6
        spacing: 3
        Rectangle { width: 22; height: 2; radius: 1; color: preview.textColor }
        Rectangle { width: 14; height: 2; radius: 1; color: preview.subtextColor }
      }
    }
    // The three colours a terminal spends its state on.
    Column {
      x: 45
      y: 16
      spacing: 4
      Repeater {
        model: [preview.entry.red, preview.entry.green, preview.entry.yellow]
        Rectangle {
          required property var modelData
          width: 12
          height: 3
          radius: 1.5
          color: modelData
        }
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
      Shared.SearchField {
        id: search
        objectName: "search"
        theme: panel.theme
        width: parent.width
        focus: true
        placeholderText: "Family, variant, light or dark"
        text: panel.store.query
        onTextEdited: panel.store.query = text
        Keys.onDownPressed: event => { presets.incrementCurrentIndex(); event.accepted = true }
        Keys.onUpPressed: event => { presets.decrementCurrentIndex(); event.accepted = true }
        Keys.onReturnPressed: event => { panel.applyRow(presets.currentIndex); event.accepted = true }
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
      // The theme is saved by the time this is read, so what is left is named as
      // something to reload rather than as a switch that failed.
      Shared.StatusBanner {
        theme: panel.theme
        width: parent.width
        visible: panel.store.reloadPending !== ""
        glyph: "󰑐"
        tint: panel.theme.yellow
        title: "Applied; some applications keep the old colors"
        detail: panel.store.reloadPending
      }
      Shared.SectionRule {
        theme: panel.theme
        width: parent.width
        label: "PRESETS"
        detail: panel.store.detail
      }
      Shared.EmptyState {
        theme: panel.theme
        width: parent.width
        visible: panel.store.model.count === 0
        glyph: panel.store.query !== "" ? "󰍉" : "󰔎"
        title: panel.store.query !== "" ? "No matching palette" : "No themes available"
        detail: panel.store.query !== ""
          ? "Search a family such as Catppuccin or Gruvbox, a variant such as Mocha, or light and dark."
          : "The catalog is generated during a rebuild. Refresh once the desktop has one."
        Shared.ActionButton {
          theme: panel.theme
          objectName: "emptyAction"
          text: panel.store.query !== "" ? "Clear search" : "Refresh"
          enabled: panel.store.query !== "" || !panel.store.busy
          onClicked: {
            if (panel.store.query !== "") panel.store.query = ""
            else panel.store.refresh()
          }
        }
      }
    }
    Shared.SeeleListView {
      id: presets
      objectName: "presets"
      theme: panel.theme
      width: parent.width
      visible: panel.store.model.count > 0
      height: visible ? Math.min(contentHeight, panel.listHeight) : 0
      clip: true
      spacing: panel.theme.spaceSmall
      model: panel.store.model
      currentIndex: count ? 0 : -1
      // A click assigns the index and ends the binding above. Keep a row that
      // exists selected as a search narrows or refills the list, the same rule
      // Ports keeps, so Enter's target never depends on the view's defaults.
      onCountChanged: currentIndex = count ? Math.max(0, Math.min(currentIndex, count - 1)) : -1
      ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }
      delegate: Column {
        id: group
        required property var entry
        required property int index
        readonly property bool selected: presets.currentIndex === group.index
        readonly property bool applying: panel.store.applying === group.entry.id
        width: presets.width - panel.theme.scrollGutter
        spacing: panel.theme.spaceTight

        // A group says what it is once, above the first row a search left in it.
        Shared.SectionLabel {
          objectName: "groupHeading"
          theme: panel.theme
          visible: !!group.entry.first
          height: visible ? implicitHeight + panel.theme.spaceTight : 0
          text: group.entry.section
          verticalAlignment: Text.AlignBottom
        }
        Rectangle {
          objectName: "card"
          width: parent.width
          height: Math.max(preview.height, title.implicitHeight) + panel.theme.cardPadding * 2
          radius: panel.theme.radius
          color: rowMouse.pressed ? panel.theme.pressColor
            : group.selected ? panel.theme.selectedColor
            : panel.theme.cardColor
          antialiasing: true
          Behavior on color { ColorAnimation { duration: panel.theme.durationFast } }
          Shared.CardEdge { theme: panel.theme }
          HoverHandler { id: rowHover }
          Shared.HoverWash { theme: panel.theme; hovered: rowHover.hovered }

          Preview {
            id: preview
            anchors { left: parent.left; leftMargin: panel.theme.cardPadding; verticalCenter: parent.verticalCenter }
            entry: group.entry
          }
          Column {
            id: title
            anchors { left: preview.right; right: chip.left; leftMargin: panel.theme.spaceLarge; rightMargin: panel.theme.spaceMedium; verticalCenter: parent.verticalCenter }
            spacing: panel.theme.spaceTight
            Text {
              width: parent.width
              text: group.entry.name
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: panel.theme.text
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textBody
              font.weight: panel.theme.weightStrong
            }
            Text {
              width: parent.width
              text: group.entry.modeLabel
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: panel.theme.subtext
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }
          }
          Shared.StatusChip {
            id: chip
            objectName: "stateChip"
            theme: panel.theme
            anchors { right: parent.right; rightMargin: panel.theme.cardPadding; verticalCenter: parent.verticalCenter }
            visible: group.applying || !!group.entry.current
            width: visible ? implicitWidth : 0
            text: group.applying ? "Applying" : "Current"
            tint: group.applying ? panel.theme.accent : panel.theme.green
          }
          MouseArea {
            id: rowMouse
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: {
              presets.currentIndex = group.index
              panel.store.apply(group.entry.id)
            }
          }
        }
      }
    }
  }
  Keys.onPressed: event => panel.handleKey(event)
}
