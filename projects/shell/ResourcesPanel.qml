pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

FocusScope {
  id: panel
  required property var theme
  required property var store
  property real maximumHeight: theme.resourcesMaximumHeight
  property bool popupHovered: false
  readonly property string hint: "Live · " + (snapshot.cadenceSeconds || 1) + " second · history stays in this panel"
  readonly property var snapshot: store.snapshot
  readonly property var memory: snapshot.memory
  readonly property var selected: snapshot.selected || null
  implicitHeight: Math.min(content.implicitHeight, maximumHeight)
  function percent(value) { return typeof value === "number" && isFinite(value) ? value.toFixed(1) + "%" : "—" }
  function bytes(value) {
    if (typeof value !== "number" || !isFinite(value)) return "—"
    if (value >= 1073741824) return (value / 1073741824).toFixed(1) + " GiB"
    if (value >= 1048576) return (value / 1048576).toFixed(1) + " MiB"
    return Math.round(value / 1024) + " KiB"
  }
  function chooseCurrent() {
    if (processes.currentIndex >= 0 && processes.currentIndex < store.model.count)
      store.select(store.model.get(processes.currentIndex).entry.id)
  }
  function focusSearch() { search.forceActiveFocus() }
  component Caption: Text {
    color: panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
    textFormat: Text.PlainText
    wrapMode: Text.Wrap
  }
  component Metric: Column {
    id: metric
    required property string label
    required property string value
    required property string detail
    required property var values
    required property color tint
    spacing: panel.theme.spaceSmall
    Text { text: metric.label; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textLabel; font.weight: panel.theme.weightMedium }
    Text { text: metric.value; color: panel.theme.text; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textDisplay; font.weight: panel.theme.weightLight }
    Caption { width: parent.width; text: metric.detail; elide: Text.ElideRight; wrapMode: Text.NoWrap }
    Shared.HistoryChart { theme: panel.theme; width: parent.width; capacity: panel.snapshot.historyCapacity || 60; series: [{values: metric.values, color: metric.tint}] }
    Row {
      width: parent.width
      Caption { width: parent.width / 2; text: (panel.snapshot.historyCapacity || 60) * (panel.snapshot.cadenceSeconds || 1) + " seconds" }
      Caption { width: parent.width / 2; text: "now · 0–100%"; horizontalAlignment: Text.AlignRight }
    }
  }
  Shared.SeeleFlickable {
    theme: panel.theme
    anchors.fill: parent
    contentHeight: content.implicitHeight
    clip: true
    ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }
    Column {
      id: content
      width: parent.width
      spacing: panel.theme.panelSpacing
      Shared.StatusBanner {
        theme: panel.theme; width: parent.width
        visible: panel.store.error !== ""
        title: "Resource readings unavailable"; detail: panel.store.error
        tint: panel.theme.yellow; glyph: "󰀦"
      }
      Rectangle {
        width: parent.width
        height: metrics.implicitHeight + panel.theme.cardPadding * 2
        radius: panel.theme.radius
        color: panel.theme.cardColor
        Shared.CardEdge { theme: panel.theme }
        Row {
          id: metrics
          anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
          spacing: panel.theme.cardPadding
          Metric {
            width: (parent.width - panel.theme.cardPadding * 2 - panel.theme.hairline) / 2
            label: "CPU"
            value: panel.percent(panel.snapshot.cpu)
            detail: panel.snapshot.cores ? panel.snapshot.cores + " logical CPUs · total capacity" : "Waiting for CPU readings"
            values: panel.snapshot.cpuHistory || []
            tint: panel.theme.accent
          }
          Rectangle { width: panel.theme.hairline; height: metrics.implicitHeight; color: panel.theme.separatorColor }
          Metric {
            width: (parent.width - panel.theme.cardPadding * 2 - panel.theme.hairline) / 2
            label: "MEMORY"
            value: panel.memory ? panel.bytes(panel.memory.used) : "—"
            detail: panel.memory ? panel.bytes(panel.memory.available) + " available / " + panel.bytes(panel.memory.total) : "Memory readings unavailable"
            values: panel.snapshot.memoryHistory || []
            tint: panel.theme.green
          }
        }
      }
      Caption {
        width: parent.width
        text: panel.memory ? "Used = total − available. Swap: " + panel.bytes(panel.memory.swapUsed) + " / " + panel.bytes(panel.memory.swapTotal) : "Reading local CPU and memory…"
      }
      Shared.SearchField {
        id: search
        objectName: "resourcesSearch"
        theme: panel.theme
        width: parent.width
        focus: true
        text: panel.store.query
        placeholderText: "Search process name or PID"
        onTextEdited: panel.store.query = text
        Keys.onDownPressed: event => { processes.incrementCurrentIndex(); event.accepted = true }
        Keys.onUpPressed: event => { processes.decrementCurrentIndex(); event.accepted = true }
        Keys.onReturnPressed: event => { panel.chooseCurrent(); event.accepted = true }
      }
      Shared.SectionRule { width: parent.width; theme: panel.theme; label: "PROCESSES"; detail: panel.snapshot.matched + " of " + panel.snapshot.total }
      Shared.SegmentWell {
        theme: panel.theme
        width: parent.width
        Shared.SegmentChoice { objectName: "resourcesCpuSort"; theme: panel.theme; width: parent.width / 2; height: parent.height; text: "CPU ↓"; selected: panel.store.sort === "cpu"; onClicked: panel.store.sort = "cpu" }
        Shared.SegmentChoice { objectName: "resourcesMemorySort"; theme: panel.theme; width: parent.width / 2; height: parent.height; text: "Memory ↓"; selected: panel.store.sort === "memory"; onClicked: panel.store.sort = "memory" }
      }
      Caption {
        width: parent.width
        text: "Process CPU: 100% = one logical CPU. RSS counts shared pages in each process."
      }
      Caption {
        width: parent.width
        visible: !!panel.snapshot.limited || panel.snapshot.matched > panel.store.model.count
        text: (panel.snapshot.limited ? "Some processes changed or could not be read. " : "") + (panel.snapshot.matched > panel.store.model.count ? "Showing the first " + panel.store.model.count + " matches. Narrow the search to find another process." : "Only available readings are shown.")
      }
      Rectangle {
        width: parent.width
        visible: panel.store.selected !== ""
        height: visible ? processDetail.implicitHeight + panel.theme.cardPadding * 2 : 0
        color: panel.theme.activeTint
        radius: panel.theme.radius
        Shared.CardEdge { theme: panel.theme }
        Column {
          id: processDetail
          anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
          spacing: panel.theme.spaceSmall
          Row {
            width: parent.width
            Text {
              width: parent.width - closeDetail.width - panel.theme.spaceSmall
              text: panel.selected ? panel.selected.name + " · " + panel.selected.pid : "Process is no longer available"
              textFormat: Text.PlainText; elide: Text.ElideRight
              color: panel.theme.text; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textLead; font.weight: panel.theme.weightStrong
            }
            Shared.ActionButton { id: closeDetail; theme: panel.theme; text: "Close"; onClicked: panel.store.select(panel.store.selected) }
          }
          Caption {
            width: parent.width
            text: panel.selected ? panel.selected.state + " · " + panel.selected.threads + " threads · CPU " + panel.percent(panel.selected.cpu) : "It exited or cannot be read. A reused PID will never replace this selection."
          }
          Caption {
            width: parent.width; visible: !!panel.selected
            text: panel.selected ? "Resident " + panel.bytes(panel.selected.rss) + " · Virtual " + panel.bytes(panel.selected.virtualBytes) : ""
          }
          Caption { width: parent.width; visible: !!panel.selected && panel.selected.cpu === null; text: "CPU needs two consecutive readings of this process." }
        }
      }
      Shared.EmptyState {
        theme: panel.theme; width: parent.width
        visible: panel.store.model.count === 0
        glyph: "󰍉"
        title: panel.store.query !== "" ? "No matching process" : "Waiting for processes"
        detail: panel.store.query !== "" ? "Try a process name or its numeric PID." : "Only process names are read. Command lines remain private."
      }
      Shared.SeeleListView {
        id: processes
        objectName: "resourcesProcesses"
        theme: panel.theme
        width: parent.width
        height: Math.min(contentHeight, panel.theme.rowHeight * 7)
        clip: true
        model: panel.store.model
        currentIndex: count ? 0 : -1
        ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }
        delegate: Item {
          id: row
          required property var entry
          required property int index
          width: processes.width - panel.theme.scrollGutter
          height: panel.theme.rowHeight
          Rectangle {
            anchors.fill: parent
            anchors.bottomMargin: panel.theme.spaceTight
            radius: panel.theme.radius
            color: rowMouse.pressed ? panel.theme.pressColor : panel.store.selected === row.entry.id ? panel.theme.selectedColor : row.index === processes.currentIndex ? panel.theme.rowColor : panel.theme.cardColor
            Shared.HoverWash { theme: panel.theme; hovered: rowHover.hovered }
            Row {
              anchors { fill: parent; leftMargin: panel.theme.cardPadding; rightMargin: panel.theme.cardPadding }
              spacing: panel.theme.spaceSmall
              Column {
                width: parent.width - cpuValue.width - memoryValue.width - panel.theme.spaceSmall * 2
                anchors.verticalCenter: parent.verticalCenter
                Text { width: parent.width; text: row.entry.name; textFormat: Text.PlainText; elide: Text.ElideRight; color: panel.theme.text; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody }
                Caption { text: row.entry.pid + " · " + row.entry.state }
              }
              Text { id: cpuValue; width: panel.theme.levelValueWidth; anchors.verticalCenter: parent.verticalCenter; text: panel.percent(row.entry.cpu); horizontalAlignment: Text.AlignRight; color: panel.theme.accent; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody }
              Text { id: memoryValue; width: panel.theme.levelValueWidth * 1.5; anchors.verticalCenter: parent.verticalCenter; text: panel.bytes(row.entry.rss); horizontalAlignment: Text.AlignRight; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody }
            }
            HoverHandler { id: rowHover }
            MouseArea { id: rowMouse; anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { processes.currentIndex = row.index; panel.store.select(row.entry.id) } }
          }
        }
      }
      Caption { width: parent.width; text: "↑ / ↓ moves · Enter shows details · Ctrl + F searches · Escape closes" }
    }
  }
  Keys.onPressed: event => {
    if (event.key === Qt.Key_F && event.modifiers & Qt.ControlModifier) { panel.focusSearch(); event.accepted = true }
    else if (event.key === Qt.Key_Down || event.key === Qt.Key_J) { processes.incrementCurrentIndex(); event.accepted = true }
    else if (event.key === Qt.Key_Up || event.key === Qt.Key_K) { processes.decrementCurrentIndex(); event.accepted = true }
    else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { panel.chooseCurrent(); event.accepted = true }
  }
}
