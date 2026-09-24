pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../shared" as Shared

// The clock worker owns dates, timezone rules and availability. This surface
// sends absolute UTC selections; it never interprets a local wall-clock time.
FocusScope {
  id: panel
  required property var theme
  property var plan: ({})
  property string error: ""
  property bool pending: false
  property bool copyPending: false
  property real maximumHeight: theme.meetingMaximumHeight
  property bool popupHovered: false
  readonly property bool ready: !!plan.date
  readonly property bool dateDirty: dateInput.text !== (plan.date || "")
  property int selectedMinute: Number(plan.minute || 0)
  property int selectedDuration: Number(plan.duration || 60)
  readonly property string hint: "← / → 15 min · Page Up / Down day · Ctrl+C copies"
  signal requested(var selection)
  signal copyRequested(string summary)
  signal managePins()
  signal closeRequested()
  implicitHeight: controls.implicitHeight + theme.panelSpacing + zones.height

  function choose(minute, duration, shift, date) {
    selectedMinute = Math.max(0, Math.min(1439, minute))
    selectedDuration = duration
    requested({ date: date === undefined ? (plan.date || "") : date,
      minute: selectedMinute, duration: duration, shift: shift || 0 })
  }
  function resetNow() { choose(0, selectedDuration, 0, "") }
  function copy() {
    if (ready && !dateDirty && !pending && !error && !copyPending) copyRequested(plan.summary)
  }
  onPlanChanged: {
    selectedMinute = Number(plan.minute || 0)
    selectedDuration = Number(plan.duration || 60)
    if (!dateInput.activeFocus) dateInput.text = plan.date || ""
  }
  Keys.onPressed: event => {
    if (event.key === Qt.Key_Escape) { closeRequested(); event.accepted = true }
    else if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_C) { copy(); event.accepted = true }
    else if (!dateInput.activeFocus && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
      if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) choose(selectedMinute + (event.key === Qt.Key_Left ? -15 : 15), selectedDuration)
      else if (event.key === Qt.Key_PageUp || event.key === Qt.Key_PageDown) choose(selectedMinute, selectedDuration, event.key === Qt.Key_PageUp ? -1 : 1)
      else if (event.key === Qt.Key_N) resetNow()
      else return
      event.accepted = true
    }
  }

  component Label: Text {
    textFormat: Text.PlainText
    color: panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
  }
  component Strip: Item {
    id: strip
    required property var slots
    height: panel.theme.spaceLarge
    Repeater {
      model: strip.slots
      Rectangle {
        required property bool modelData
        required property int index
        objectName: "meetingSlot" + index
        x: index * strip.width / 96
        width: strip.width / 96 + panel.theme.hairline / 2
        height: strip.height
        color: modelData ? panel.theme.fillColor : panel.theme.wellColor
      }
    }
    Rectangle {
      x: Math.min(parent.width - width, panel.selectedMinute / 1440 * parent.width)
      width: panel.theme.hairline * 2
      height: parent.height
      color: panel.theme.accent
    }
  }
  component Duration: Shared.SegmentChoice {
    id: durationButton
    required property int minutes
    theme: panel.theme
    width: parent ? parent.width / 3 : 0
    height: parent ? parent.height : panel.theme.chipHeight
    text: minutes + "m"
    selected: minutes === panel.selectedDuration
    Accessible.name: minutes + " minute meeting"
    onClicked: panel.choose(panel.selectedMinute, minutes)
  }

  Column {
    id: controls
    width: parent.width
    spacing: panel.theme.panelSpacing
    RowLayout {
      width: parent.width
      spacing: panel.theme.spaceSmall
      Shared.GlyphButton { theme: panel.theme; glyph: "󰅁"; text: "Previous UTC day · Page Up"; enabled: panel.ready && !panel.pending; onClicked: panel.choose(panel.selectedMinute, panel.selectedDuration, -1) }
      Shared.ValueField {
        id: dateInput
        objectName: "meetingDate"
        theme: panel.theme
        Layout.fillWidth: true
        placeholderText: "YYYY-MM-DD · UTC"
        Accessible.name: "Meeting date in UTC, YYYY-MM-DD"
        horizontalAlignment: Text.AlignHCenter
        onAccepted: { panel.choose(panel.selectedMinute, panel.selectedDuration, 0, text); scrubber.forceActiveFocus() }
      }
      Shared.GlyphButton { theme: panel.theme; glyph: "󰅂"; text: "Next UTC day · Page Down"; enabled: panel.ready && !panel.pending; onClicked: panel.choose(panel.selectedMinute, panel.selectedDuration, 1) }
      Shared.ActionButton { objectName: "meetingNow"; theme: panel.theme; text: "Now"; onClicked: panel.resetNow() }
    }
    Rectangle {
      width: parent.width
      height: timeContent.implicitHeight + panel.theme.cardPadding * 2
      color: panel.theme.cardColor
      radius: panel.theme.radius
      Shared.CardEdge { theme: panel.theme }
      Column {
        id: timeContent
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
        anchors.margins: panel.theme.cardPadding
        spacing: panel.theme.spaceSmall
        RowLayout {
          width: parent.width
          Label { text: panel.ready ? panel.plan.time : "—"; font.pixelSize: panel.theme.textHero; font.weight: panel.theme.weightLight; color: panel.theme.text }
          Label { text: "UTC"; font.pixelSize: panel.theme.textLead; color: panel.theme.accent; Layout.fillWidth: true }
          Shared.SegmentWell {
            theme: panel.theme
            Layout.preferredWidth: panel.theme.controlHeight * 4
            Duration { minutes: 30 }
            Duration { minutes: 60 }
            Duration { minutes: 90 }
          }
        }
        Label { width: parent.width; text: panel.ready ? "Ends " + panel.plan.end : "Loading meeting times…"; elide: Text.ElideRight }
        Slider {
          id: scrubber
          objectName: "meetingTimeline"
          width: parent.width
          height: panel.theme.controlHeight
          from: 0; to: 1439; stepSize: 15
          value: panel.selectedMinute
          snapMode: Slider.SnapAlways
          focusPolicy: Qt.StrongFocus
          Accessible.name: "Meeting start, minutes after midnight UTC"
          Accessible.description: "Arrow keys move fifteen minutes. The lit strip fits the whole meeting into working hours in every zone."
          onMoved: panel.choose(value, panel.selectedDuration)
          background: Strip {
            objectName: "meetingOverlap"
            x: scrubber.leftPadding
            y: (scrubber.height - height) / 2
            width: scrubber.availableWidth
            slots: panel.plan.overlap || []
          }
          handle: Rectangle {
            x: scrubber.leftPadding + scrubber.visualPosition * (scrubber.availableWidth - width)
            y: (scrubber.height - height) / 2
            width: panel.theme.spaceMedium
            height: panel.theme.chipHeight
            radius: panel.theme.radiusSmall
            color: scrubber.pressed ? panel.theme.text : panel.theme.accent
            border.width: panel.theme.hairline
            border.color: scrubber.activeFocus ? panel.theme.text : panel.theme.cardBorder
            Shared.HoverWash { theme: panel.theme; hovered: scrubber.hovered }
          }
          HoverHandler { cursorShape: Qt.PointingHandCursor }
        }
        Item {
          width: parent.width
          height: axisLabel.implicitHeight
          Label { id: axisLabel; anchors.right: parent.right; text: "24 UTC" }
          Repeater {
            model: ["00", "06", "12", "18"]
            Label {
              required property string modelData
              required property int index
              x: index * timeContent.width / 4 - (index ? implicitWidth / 2 : 0)
              text: modelData
            }
          }
        }
        RowLayout {
          width: parent.width
          Label {
            Layout.fillWidth: true
            text: panel.dateDirty ? "Press Enter to apply the UTC date" : panel.pending ? "Updating…" : panel.ready && panel.plan.allWorking ? "Fits every zone’s working hours" : "Outside working hours in some zones"
            color: panel.pending ? panel.theme.subtext : panel.ready && panel.plan.allWorking ? panel.theme.green : panel.theme.yellow
            wrapMode: Text.Wrap
          }
          Shared.ActionButton { objectName: "meetingCopy"; theme: panel.theme; text: "Copy times"; enabled: panel.ready && !panel.dateDirty && !panel.pending && !panel.error && !panel.copyPending; onClicked: panel.copy() }
        }
      }
    }
    Label {
      width: parent.width
      visible: panel.error !== ""
      text: panel.error
      color: panel.theme.red
      wrapMode: Text.Wrap
    }
    Shared.SectionRule {
      theme: panel.theme
      width: parent.width
      label: "LOCAL + PINNED ZONES"
      Shared.ActionButton { theme: panel.theme; text: "Manage pins"; implicitHeight: panel.theme.chipHeight; onClicked: panel.managePins() }
    }
    Label {
      width: parent.width
      text: "Lit = whole meeting within Mon–Fri 09:00–17:00.\nA working-hours guide; no calendars are read."
      wrapMode: Text.Wrap
    }
  }
  Shared.SeeleListView {
    id: zones
    objectName: "meetingZones"
    anchors.top: controls.bottom
    anchors.topMargin: panel.theme.panelSpacing
    width: parent.width
    height: Math.max(0, Math.min(contentHeight, panel.maximumHeight - controls.implicitHeight - panel.theme.panelSpacing))
    model: panel.plan.rows || []
    spacing: panel.theme.spaceSmall
    clip: true
    theme: panel.theme
    delegate: Rectangle {
      id: zone
      required property var modelData
      Accessible.name: modelData.label + " " + modelData.id + " " + modelData.date + " " + modelData.time + " " + modelData.offset
      width: ListView.view.width
      height: zoneContent.implicitHeight + panel.theme.cardPadding * 2
      radius: panel.theme.radius
      color: panel.theme.cardColor
      Shared.CardEdge { theme: panel.theme }
      Column {
        id: zoneContent
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
        anchors.margins: panel.theme.cardPadding
        spacing: panel.theme.spaceSmall
        RowLayout {
          width: parent.width
          ColumnLayout {
            Layout.fillWidth: true
            spacing: panel.theme.spaceTight
            Label { Layout.fillWidth: true; text: zone.modelData.label; color: panel.theme.text; font.pixelSize: panel.theme.textBody; font.weight: panel.theme.weightStrong; elide: Text.ElideRight }
            Label { Layout.fillWidth: true; text: zone.modelData.date + " · " + zone.modelData.abbreviation + " · " + zone.modelData.offset; elide: Text.ElideRight }
          }
          Label { text: zone.modelData.time; color: zone.modelData.working ? panel.theme.accent : panel.theme.subtext; font.pixelSize: panel.theme.textDisplay; font.weight: panel.theme.weightLight }
        }
        Strip { width: parent.width; slots: zone.modelData.slots }
      }
    }
    ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }
  }
}
