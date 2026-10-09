pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../shared" as Shared

// The clock worker owns the local day, every zone's hours, working-hour fit,
// suggestions and the copied text. This surface moves one absolute start
// instant and draws what the worker measured; it never turns a wall-clock
// time into an instant itself.
FocusScope {
  id: panel
  required property var theme
  property var plan: ({})
  // The calendar store once Google Calendar is set up. Its busy time is drawn
  // here; the coordinator sends it to the worker with every request.
  property var calendar: null
  property string error: ""
  property bool pending: false
  property bool copyPending: false
  property bool popupHovered: false
  property real now: Date.now() / 1000
  property real maximumHeight: theme.meetingMaximumHeight
  readonly property bool ready: plan.start !== undefined
  readonly property var day: plan.day || ({})
  readonly property real dayMinutes: day.minutes || 1440
  readonly property var durations: [15, 30, 45, 60, 90, 120]
  readonly property var durationLabels: ["15m", "30m", "45m", "1h", "1½h", "2h"]
  // The selection moves at once; the worker's reading of it follows.
  property real selectedStart: 0
  property int selectedDuration: 60
  // Day moves count from where the last reply put the selection, and add up
  // while one is still being answered, so two Page Downs move two days.
  property real shiftBase: 0
  property int shiftDays: 0
  // The band follows a drag without easing, and eases for every other move.
  property bool dragging: false
  readonly property bool calendarShown: !!calendar && calendar.configured
  readonly property var busyBlocks: calendarShown && day.start !== undefined ? calendar.busy(day.start, day.end) : []
  readonly property var conflictTitles: {
    var titles = []
    ;(plan.conflicts || []).forEach(function(pair) {
      panel.busyBlocks.forEach(function(block) {
        if (block.start === pair[0] && block.end === pair[1] && titles.indexOf(block.title) < 0) titles.push(block.title)
      })
    })
    return titles
  }
  // The busy block under the pointer, named in the calendar's row: a block an
  // hour long is too narrow to carry its own title.
  readonly property var hoveredBlock: {
    if (!grid.containsMouse || day.start === undefined) return null
    var at = day.start + grid.mouseX / Math.max(1, grid.width) * dayMinutes * 60
    for (var i = 0; i < busyBlocks.length; i++) if (busyBlocks[i].start <= at && at < busyBlocks[i].end) return busyBlocks[i]
    return null
  }
  readonly property bool canCopy: ready && !pending && error === "" && !copyPending && !dateField.dirty
  readonly property string hint: "←/→ 15 min · ⇧ hour · PgUp/PgDn day · F next fit · Ctrl+C copy"
  implicitHeight: controls.implicitHeight + zones.height + footer.implicitHeight + theme.panelSpacing * 2
  signal requested(var selection)
  signal copyRequested(string summary)
  signal openRequested(string url)
  signal managePins()
  signal closeRequested()

  function send(selection) {
    requested(Object.assign({duration: selectedDuration}, selection))
  }
  function select(start) {
    if (!ready) return
    selectedStart = start
    shiftDays = 0
    send({start: start})
  }
  function move(slots) { select(selectedStart + slots * 900) }
  function shiftDay(days) {
    if (!ready) return
    if (!pending || shiftDays === 0) { shiftBase = selectedStart; shiftDays = 0 }
    shiftDays += days
    send({start: shiftBase, days: shiftDays})
  }
  function goToDate(text) {
    if (ready) send({start: selectedStart, date: text})
  }
  function resetNow() {
    shiftDays = 0
    send({})
  }
  function setDuration(minutes) {
    selectedDuration = minutes
    send(ready ? {start: selectedStart} : {})
  }
  function lengthen(step) {
    var index = Math.max(0, Math.min(durations.length - 1, durations.indexOf(selectedDuration) + step))
    if (durations[index] !== selectedDuration) setDuration(durations[index])
  }
  function nextFit() { if (plan.next) select(plan.next.start) }
  function suggestion(index) {
    var chosen = (plan.suggestions || [])[index]
    if (chosen) select(chosen.start)
  }
  function copy() { if (canCopy) copyRequested(plan.summary) }
  function openCalendar() { if (canCopy && calendarShown && plan.calendarUrl) openRequested(plan.calendarUrl) }
  // A pointer position across the ribbons, as the start that centres the
  // meeting under it on the day's quarter-hour grid. A pointer keeps the
  // meeting inside the day it can see; the keyboard can step past midnight.
  function startAt(x, width) {
    var last = Math.max(0, Math.floor((dayMinutes - selectedDuration) / 15))
    var slot = Math.round((x / Math.max(1, width) * dayMinutes - selectedDuration / 2) / 15)
    return day.start + Math.max(0, Math.min(last, slot)) * 900
  }
  // Give the keyboard back to the planner. A focus scope hands active focus
  // to whichever child last held it, so the date field lets go explicitly.
  function takeKeys() {
    dateField.focus = false
    forceActiveFocus()
  }
  function position(epoch, width) { return (epoch - day.start) / 60 / dayMinutes * width }
  function clock(epoch) { return Qt.formatTime(new Date(epoch * 1000), "HH:mm") }
  function fitColor(fit) { return fit === "work" ? theme.green : fit === "edge" ? theme.yellow : theme.red }

  // A planner opened again starts on its own keys, not in the date field.
  onActiveFocusChanged: if (!activeFocus) dateField.focus = false
  onPlanChanged: {
    if (!ready) return
    selectedStart = plan.start
    selectedDuration = plan.duration
    shiftDays = 0
    if (!dateField.activeFocus) dateField.text = day.label || ""
  }

  Keys.onPressed: event => {
    var control = event.modifiers & Qt.ControlModifier
    var hour = event.modifiers & Qt.ShiftModifier
    if (event.key === Qt.Key_Escape) closeRequested()
    else if (event.modifiers & (Qt.AltModifier | Qt.MetaModifier)) return
    else if (control && event.key === Qt.Key_C) copy()
    else if (control && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) openCalendar()
    else if (control) return
    else if (event.key === Qt.Key_Left || event.key === Qt.Key_H) move(hour ? -4 : -1)
    else if (event.key === Qt.Key_Right || event.key === Qt.Key_L) move(hour ? 4 : 1)
    else if (event.key === Qt.Key_PageUp) shiftDay(-1)
    else if (event.key === Qt.Key_PageDown) shiftDay(1)
    else if (event.key === Qt.Key_N) resetNow()
    else if (event.key === Qt.Key_F) nextFit()
    else if (event.key === Qt.Key_D) dateField.forceActiveFocus()
    else if (event.key === Qt.Key_Minus) lengthen(-1)
    else if (event.key === Qt.Key_Plus || event.key === Qt.Key_Equal) lengthen(1)
    else if (event.key >= Qt.Key_1 && event.key <= Qt.Key_3) suggestion(event.key - Qt.Key_1)
    else return
    event.accepted = true
  }

  component Caption: Text {
    textFormat: Text.PlainText
    color: panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
    elide: Text.ElideRight
  }

  component Dot: Rectangle {
    property color tint: panel.theme.overlay
    width: panel.theme.spaceSmall
    height: width
    radius: width / 2
    color: tint
  }

  // One participant's hours across the local day: the cells the worker
  // measured, the past dimmed, the present marked and the meeting outlined.
  // The calendar's row lays its busy blocks on the same track instead.
  component Ribbon: Item {
    id: ribbon
    property var cells: []
    property var blocks: []
    readonly property real nowX: panel.position(panel.now, width)
    height: panel.theme.meetingRibbonHeight

    Rectangle {
      visible: ribbon.cells.length === 0
      anchors.fill: parent
      radius: panel.theme.shapeExtraSmall
      color: panel.theme.wellColor
    }
    Repeater {
      model: ribbon.cells
      Rectangle {
        id: cell
        required property var modelData
        readonly property bool work: modelData.kind === "work"
        x: modelData.from / panel.dayMinutes * ribbon.width + panel.theme.hairline / 2
        width: Math.max(1, (modelData.to - modelData.from) / panel.dayMinutes * ribbon.width - panel.theme.hairline)
        height: ribbon.height
        radius: panel.theme.shapeExtraSmall
        color: work ? panel.theme.alpha(panel.theme.accent, 0.34)
          : modelData.kind === "edge" ? panel.theme.alpha(panel.theme.accent, 0.13) : panel.theme.wellColor
        Text {
          anchors.centerIn: parent
          visible: implicitWidth + 2 <= cell.width
          text: cell.modelData.day || cell.modelData.label
          textFormat: Text.PlainText
          color: cell.modelData.day ? panel.theme.accent : cell.work ? panel.theme.text : panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textMicro
          font.weight: cell.modelData.day ? panel.theme.weightStrong : panel.theme.weightRegular
        }
      }
    }
    Repeater {
      model: ribbon.blocks
      Rectangle {
        id: block
        required property var modelData
        readonly property real from: Math.max(0, panel.position(modelData.start, ribbon.width))
        x: from + panel.theme.hairline / 2
        width: Math.max(2, Math.min(ribbon.width, panel.position(modelData.end, ribbon.width)) - from - panel.theme.hairline)
        height: ribbon.height
        radius: panel.theme.shapeExtraSmall
        // Google's colours arrive as strings; Qt.alpha takes either form.
        color: Qt.alpha(modelData.color || panel.theme.accent, 0.72)
        clip: true
        Text {
          anchors { fill: parent; leftMargin: 3; rightMargin: 2 }
          visible: block.width > 24
          verticalAlignment: Text.AlignVCenter
          text: block.modelData.title
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: panel.theme.text
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textMicro
        }
      }
    }
    // The past is dimmed: everything before now on today, all of an earlier day.
    Rectangle {
      visible: panel.day.past === true || panel.day.today === true
      width: panel.day.past ? ribbon.width : Math.max(0, ribbon.nowX)
      height: ribbon.height
      color: panel.theme.alpha(panel.theme.crust, 0.45)
    }
    Rectangle {
      visible: panel.day.today === true
      x: ribbon.nowX
      width: panel.theme.hairline
      height: ribbon.height
      color: panel.theme.red
    }
    Rectangle {
      x: panel.position(panel.selectedStart, ribbon.width) - 1
      y: -2
      width: Math.max(4, panel.selectedDuration / panel.dayMinutes * ribbon.width) + 2
      height: ribbon.height + 4
      radius: panel.theme.shapeExtraSmall
      color: panel.theme.alpha(panel.theme.accent, 0.12)
      border.width: 2
      border.color: panel.theme.accent
      antialiasing: true
      Behavior on x {
        enabled: !panel.dragging
        NumberAnimation { duration: panel.theme.durationFastSpatial; easing.type: Easing.BezierSpline; easing.bezierCurve: panel.theme.springFastSpatial }
      }
    }
  }

  // A participant's name, where the meeting falls for them, and their hours.
  component ZoneRow: Column {
    id: zoneRow
    property string label: ""
    property string caption: ""
    property string range: ""
    property color rangeColor: panel.theme.text
    property alias cells: zoneRibbon.cells
    property alias blocks: zoneRibbon.blocks
    width: parent ? parent.width : 0
    spacing: panel.theme.spaceTight
    Accessible.role: Accessible.StaticText
    Accessible.name: label + ", " + range + ", " + caption
    RowLayout {
      width: parent.width
      spacing: panel.theme.spaceMedium
      Text {
        text: zoneRow.label
        textFormat: Text.PlainText
        color: panel.theme.text
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textBody
        font.weight: panel.theme.weightStrong
        Layout.maximumWidth: zoneRow.width * 0.4
        elide: Text.ElideRight
      }
      Caption { text: zoneRow.caption; Layout.fillWidth: true }
      Text {
        text: zoneRow.range
        textFormat: Text.PlainText
        color: zoneRow.rangeColor
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textBody
        font.weight: panel.theme.weightMedium
      }
    }
    Ribbon { id: zoneRibbon; width: parent.width }
  }

  // A suggested start: its local range over who, if anyone, it is awkward for.
  component Suggestion: Button {
    id: choice
    required property var modelData
    required property int index
    readonly property bool chosen: modelData.start === panel.selectedStart
    objectName: "meetingSuggestion" + index
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    leftPadding: panel.theme.spaceMedium
    rightPadding: panel.theme.spaceMedium
    Accessible.name: "Suggestion " + (index + 1) + ": " + modelData.range + ", " + modelData.caption
    Keys.onReturnPressed: clicked()
    Keys.onEnterPressed: clicked()
    onClicked: { panel.select(modelData.start); panel.takeKeys() }
    contentItem: Column {
      spacing: 1
      Row {
        spacing: panel.theme.spaceSmall
        Dot { anchors.verticalCenter: parent.verticalCenter; tint: panel.fitColor(choice.modelData.fit) }
        Text {
          text: choice.modelData.range
          textFormat: Text.PlainText
          color: choice.chosen ? panel.theme.accent : panel.theme.text
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textBody
          font.weight: panel.theme.weightStrong
        }
      }
      Caption { width: choice.availableWidth; text: choice.modelData.caption }
    }
    background: Rectangle {
      radius: panel.theme.radius
      color: choice.chosen ? panel.theme.selectedColor : panel.theme.cardColor
      Behavior on color { ColorAnimation { duration: panel.theme.durationFast } }
      Shared.HoverWash { theme: panel.theme; hovered: choice.hovered; pressed: choice.down }
      Shared.FocusRing { theme: panel.theme; shown: choice.visualFocus }
      Text {
        anchors { right: parent.right; top: parent.top; margins: panel.theme.spaceSmall }
        text: String(choice.index + 1)
        color: panel.theme.overlay
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textMicro
      }
    }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
  }

  Column {
    id: controls
    width: parent.width
    spacing: panel.theme.panelSpacing

    // The local day, typed as an ISO date or stepped a day at a time.
    RowLayout {
      width: parent.width
      spacing: panel.theme.spaceSmall
      Shared.GlyphButton {
        theme: panel.theme
        glyph: "󰅁"
        text: "Previous day · Page Up"
        enabled: panel.ready
        onClicked: panel.shiftDay(-1)
      }
      Shared.ValueField {
        id: dateField
        objectName: "meetingDate"
        readonly property bool dirty: activeFocus && text !== (panel.day.date || "")
        theme: panel.theme
        Layout.fillWidth: true
        horizontalAlignment: Text.AlignHCenter
        placeholderText: "YYYY-MM-DD"
        Accessible.name: "Meeting day, YYYY-MM-DD in this computer's timezone"
        onActiveFocusChanged: {
          text = activeFocus ? (panel.day.date || "") : (panel.day.label || "")
          if (activeFocus) selectAll()
        }
        Keys.onPressed: event => {
          if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (dirty) panel.goToDate(text.trim())
            panel.takeKeys()
          } else if (event.key === Qt.Key_Escape) panel.takeKeys()
          else return
          event.accepted = true
        }
      }
      Shared.GlyphButton {
        theme: panel.theme
        glyph: "󰅂"
        text: "Next day · Page Down"
        enabled: panel.ready
        onClicked: panel.shiftDay(1)
      }
      Shared.ActionButton {
        objectName: "meetingNow"
        theme: panel.theme
        text: "Now"
        onClicked: panel.resetNow()
      }
    }

    // What the selection is for this computer, and who it suits.
    Rectangle {
      width: parent.width
      height: readout.implicitHeight + panel.theme.cardPadding * 2
      radius: panel.theme.radius
      color: panel.theme.cardColor
      Column {
        id: readout
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
        spacing: panel.theme.spaceTight
        RowLayout {
          width: parent.width
          spacing: panel.theme.spaceMedium
          Text {
            objectName: "meetingRange"
            text: panel.ready ? panel.plan.range : "––:––"
            textFormat: Text.PlainText
            color: panel.theme.text
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textHero
            font.weight: panel.theme.weightLight
          }
          Item { Layout.fillWidth: true }
          Shared.SegmentWell {
            theme: panel.theme
            Layout.preferredWidth: panel.durations.length * (panel.theme.chipHeight + panel.theme.spaceTight)
            Repeater {
              model: panel.durations
              Shared.SegmentChoice {
                required property int modelData
                required property int index
                theme: panel.theme
                objectName: "meetingDuration" + modelData
                width: parent.width / panel.durations.length
                height: parent.height
                text: panel.durationLabels[index]
                selected: modelData === panel.selectedDuration
                Accessible.name: modelData + " minute meeting"
                onClicked: { panel.setDuration(modelData); panel.takeKeys() }
              }
            }
          }
        }
        Caption {
          width: parent.width
          text: panel.ready ? panel.plan.zone + " · " + panel.plan.utc : "Reading this computer's timezone…"
        }
        Row {
          width: parent.width
          spacing: panel.theme.spaceSmall
          Dot {
            anchors.verticalCenter: parent.verticalCenter
            visible: panel.error === "" && panel.ready
            tint: panel.fitColor(panel.plan.fit)
          }
          Text {
            objectName: "meetingStatus"
            width: parent.width - panel.theme.spaceMedium * 2
            text: panel.error || (panel.ready ? panel.plan.status : "")
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: panel.error ? panel.theme.red : panel.theme.text
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textLabel
            font.weight: panel.theme.weightMedium
          }
        }
        Text {
          objectName: "meetingConflict"
          visible: panel.conflictTitles.length > 0
          width: parent.width
          text: "󰃰  Overlaps " + panel.conflictTitles.join(", ")
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: panel.theme.yellow
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textLabel
        }
      }
    }

    // Up to three starts the worker found on this day, and the next opening
    // after the selection when the next two weeks hold one.
    Shared.SectionRule {
      theme: panel.theme
      width: parent.width
      label: "Suggested"
      Shared.ActionButton {
        objectName: "meetingNextFit"
        theme: panel.theme
        visible: !!panel.plan.next
        implicitHeight: panel.theme.chipHeight
        text: panel.plan.next ? (panel.plan.next.fit === "work" ? "Next fit · " : "Closest · ") + panel.plan.next.label + "  󰅂" : ""
        onClicked: { panel.nextFit(); panel.takeKeys() }
      }
    }
    Item {
      width: parent.width
      height: panel.theme.detailRowHeight - panel.theme.spaceMedium
      Row {
        id: suggestions
        anchors.fill: parent
        spacing: panel.theme.spaceSmall
        Repeater {
          model: panel.plan.suggestions || []
          Suggestion { width: (suggestions.width - suggestions.spacing * 2) / 3; height: suggestions.height }
        }
      }
      Caption {
        visible: panel.ready && (panel.plan.suggestions || []).length === 0
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: panel.day.past ? "This day has passed."
          : panel.day.today ? "Nobody is working for the rest of today."
          : "Nobody is working on this day."
      }
    }

    Shared.SectionRule {
      theme: panel.theme
      width: parent.width
      label: "Zones"
      Repeater {
        model: [{tint: panel.theme.alpha(panel.theme.accent, 0.34), text: "Mon–Fri 09–17"},
          {tint: panel.theme.alpha(panel.theme.accent, 0.13), text: "07–09 · 17–20"}]
        Row {
          id: legend
          required property var modelData
          spacing: panel.theme.spaceTight
          Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 10; height: 8; radius: panel.theme.shapeExtraSmall; color: legend.modelData.tint }
          Caption { text: legend.modelData.text; color: panel.theme.overlay }
        }
      }
    }
  }

  // Every row reads the same instants left to right, so one pointer position
  // is one start for all of them.
  Shared.SeeleFlickable {
    id: zones
    objectName: "meetingZones"
    theme: panel.theme
    anchors.top: controls.bottom
    anchors.topMargin: panel.theme.panelSpacing
    width: parent.width
    height: Math.max(0, Math.min(contentHeight, panel.maximumHeight - controls.implicitHeight - footer.implicitHeight - panel.theme.panelSpacing * 2))
    contentWidth: width
    contentHeight: rows.implicitHeight + 4
    clip: true
    Column {
      id: rows
      y: 2
      width: zones.width
      spacing: panel.theme.spaceMedium
      ZoneRow {
        objectName: "meetingCalendar"
        visible: panel.calendarShown
        label: "Your calendar"
        caption: panel.hoveredBlock ? panel.hoveredBlock.title + " · " + panel.clock(panel.hoveredBlock.start) + "–" + panel.clock(panel.hoveredBlock.end)
          : !panel.calendar || !panel.calendar.hasDay(panel.day.date || "")
          ? (panel.calendar && panel.calendar.connected ? "Loading…" : "Not available offline")
          : panel.calendar.stale ? "Offline · may be out of date" : panel.busyBlocks.length === 0 ? "Nothing booked" : ""
        range: !panel.ready ? "" : panel.conflictTitles.length > 0 ? "Busy" : "Free"
        rangeColor: panel.conflictTitles.length > 0 ? panel.theme.yellow : panel.theme.text
        blocks: panel.busyBlocks
      }
      Repeater {
        model: panel.plan.rows || []
        ZoneRow {
          required property var modelData
          label: modelData.label
          caption: (modelData.home ? "This computer · " : "") + modelData.caption
          range: modelData.range
          rangeColor: modelData.fit === "work" ? panel.theme.text : panel.fitColor(modelData.fit)
          cells: modelData.cells
        }
      }
    }
    // A faint column follows the pointer where a click would put the meeting.
    Rectangle {
      visible: grid.containsMouse && !grid.pressed && panel.ready
      x: panel.position(panel.startAt(grid.mouseX, grid.width), grid.width)
      width: Math.max(4, panel.selectedDuration / panel.dayMinutes * grid.width)
      height: rows.implicitHeight + 4
      radius: panel.theme.shapeExtraSmall
      color: panel.theme.alpha(panel.theme.text, 0.05)
      border.width: panel.theme.hairline
      border.color: panel.theme.alpha(panel.theme.accent, 0.4)
    }
    MouseArea {
      id: grid
      objectName: "meetingGrid"
      width: rows.width
      height: rows.implicitHeight + 4
      enabled: panel.ready
      hoverEnabled: true
      preventStealing: true
      cursorShape: Qt.PointingHandCursor
      function place() {
        var start = panel.startAt(mouseX, width)
        if (start !== panel.selectedStart) panel.select(start)
      }
      onPressed: { panel.dragging = true; panel.takeKeys(); place() }
      onPositionChanged: if (pressed) place()
      onReleased: panel.dragging = false
      onCanceled: panel.dragging = false
    }
    ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }
  }

  RowLayout {
    id: footer
    anchors.top: zones.bottom
    anchors.topMargin: panel.theme.panelSpacing
    width: parent.width
    spacing: panel.theme.spaceSmall
    Shared.ActionButton {
      objectName: "meetingEditZones"
      theme: panel.theme
      text: "Edit zones"
      onClicked: panel.managePins()
    }
    Item { Layout.fillWidth: true }
    Shared.ActionButton {
      objectName: "meetingCalendarOpen"
      theme: panel.theme
      visible: panel.calendarShown
      text: "Open in Google Calendar"
      enabled: panel.canCopy
      onClicked: panel.openCalendar()
    }
    Shared.ActionButton {
      objectName: "meetingCopy"
      theme: panel.theme
      text: "Copy"
      selected: true
      enabled: panel.canCopy
      onClicked: panel.copy()
    }
  }
}
