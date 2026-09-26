pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// The Control Center's Themes panel: Light, Dark or Auto, when Auto switches,
// and which preset each mode wears. Choosing among the presets themselves is
// the floating switcher's job, one click away. Every change is sent to
// `seele-theme` through the same store the switcher uses, so the two never
// disagree about what is applied.
FocusScope {
  id: panel
  required property var theme
  required property var store
  implicitHeight: content.implicitHeight
  signal browseRequested()

  // A schedule time, sent once when it is a real `HH:MM` and has changed:
  // on Enter or when the field is left. Escape puts the saved time back and
  // hands the keyboard to the panel, so only a second Escape closes it.
  component TimeField: TextField {
    id: field
    property string committed: ""
    property string sent: ""
    signal chosen(string value)
    width: 64
    implicitHeight: panel.theme.chipHeight
    text: field.committed
    horizontalAlignment: Text.AlignHCenter
    maximumLength: 5
    validator: RegularExpressionValidator { regularExpression: /^([01][0-9]|2[0-3]):[0-5][0-9]$/ }
    color: panel.theme.text
    selectionColor: panel.theme.selectedColor
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textBody
    function commit() {
      if (!acceptableInput || text === committed || text === sent) return
      sent = text
      chosen(text)
    }
    // Typing leaves the binding to the saved time in place, so a saved time
    // that changes replaces the edit by itself; what was sent is forgotten, so
    // the same time can be chosen again later.
    onCommittedChanged: sent = ""
    onEditingFinished: commit()
    Keys.onEscapePressed: event => { text = committed; content.forceActiveFocus(); event.accepted = true }
    background: Rectangle {
      radius: panel.theme.radiusSmall
      color: panel.theme.wellColor
      border.width: 1
      border.color: field.activeFocus ? panel.theme.accent : field.acceptableInput ? panel.theme.cardBorder : panel.theme.red
    }
  }

  // A preset's colours as a small chip: its base, carrying its accent and
  // state colours.
  component Swatch: Rectangle {
    id: swatch
    required property var preset
    width: 58
    height: 24
    radius: panel.theme.radiusSmall
    color: swatch.preset ? swatch.preset.base : panel.theme.wellColor
    border.width: 1
    border.color: panel.theme.alpha(panel.theme.text, 0.14)
    antialiasing: true
    Row {
      anchors.centerIn: parent
      spacing: 4
      visible: !!swatch.preset
      Repeater {
        model: swatch.preset ? [swatch.preset.accent, swatch.preset.red, swatch.preset.green, swatch.preset.yellow] : []
        Rectangle {
          required property var modelData
          width: 8
          height: 8
          radius: 4
          color: modelData
        }
      }
    }
  }

  // The preset one mode wears, and the way to give it the one on screen
  // instead, which is how a mode wears a preset of the other kind.
  component SlotRow: Item {
    id: slotRow
    required property string slot
    readonly property string chosen: panel.store.slots[slotRow.slot] || ""
    readonly property bool inUse: panel.store.mode === slotRow.slot
    width: parent ? parent.width : 0
    height: 40
    Swatch {
      id: chip
      anchors.verticalCenter: parent.verticalCenter
      preset: panel.store.find(slotRow.chosen)
    }
    Column {
      anchors { left: chip.right; leftMargin: panel.theme.spaceMedium; right: use.left; rightMargin: panel.theme.spaceMedium; verticalCenter: parent.verticalCenter }
      spacing: 1
      Text {
        objectName: slotRow.slot + "Theme"
        width: parent.width
        text: slotRow.chosen !== "" ? panel.store.nameOf(slotRow.chosen) : "None"
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: panel.theme.text
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textBody
        font.weight: panel.theme.weightMedium
      }
      Text {
        objectName: slotRow.slot + "Caption"
        width: parent.width
        text: (slotRow.slot === "light" ? "󰖙  Light mode" : "󰖔  Dark mode") + (slotRow.inUse ? " · in use" : "")
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: slotRow.inUse ? panel.theme.accent : panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }
    }
    Shared.ActionButton {
      id: use
      objectName: slotRow.slot + "UseCurrent"
      theme: panel.theme
      anchors { right: parent.right; verticalCenter: parent.verticalCenter }
      implicitHeight: panel.theme.chipHeight
      text: "Use current"
      enabled: panel.store.current !== "" && panel.store.current !== slotRow.chosen
      onClicked: panel.store.useCurrentFor(slotRow.slot)
    }
  }

  Column {
    id: content
    width: parent.width
    spacing: panel.theme.spaceMedium

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

    // Light and Dark hold the mode; Auto hands it to the schedule.
    Shared.SectionRule { theme: panel.theme; width: parent.width; label: "Appearance" }
    Shared.SegmentWell {
      theme: panel.theme
      width: parent.width
      height: panel.theme.controlHeight
      Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "appearanceLight"; text: "󰖙  Light"; selected: panel.store.appearanceChoice === "light"; onClicked: panel.store.setAppearance("light") }
      Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "appearanceDark"; text: "󰖔  Dark"; selected: panel.store.appearanceChoice === "dark"; onClicked: panel.store.setAppearance("dark") }
      Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "appearanceAuto"; text: "󰔎  Auto"; selected: panel.store.appearanceChoice === "auto"; onClicked: panel.store.setAppearance("auto") }
    }

    // When Auto switches: at sunrise and sunset, or at two fixed times.
    Item {
      width: parent.width
      height: panel.theme.controlHeight
      visible: panel.store.autoSource !== "off"
      Text {
        anchors.verticalCenter: parent.verticalCenter
        text: "Switch at"
        color: panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textLabel
        font.weight: panel.theme.weightMedium
      }
      Shared.SegmentWell {
        theme: panel.theme
        anchors.right: parent.right
        width: Math.round(parent.width * 0.6)
        height: parent.height
        Shared.SegmentChoice {
          theme: panel.theme
          width: parent.width / 2
          height: parent.height
          objectName: "autoSun"
          text: "Sunrise, sunset"
          selected: panel.store.autoSource === "sun"
          // Sunrise and sunset need the timezone's city; without one the
          // choice is shown but cannot be made.
          enabled: !!panel.store.appearance.place
          onClicked: panel.store.setAuto("sun")
        }
        Shared.SegmentChoice { theme: panel.theme; width: parent.width / 2; height: parent.height; objectName: "autoSchedule"; text: "Set times"; selected: panel.store.autoSource === "schedule"; onClicked: panel.store.setAuto("schedule") }
      }
    }
    // The schedule's two times, while it keeps them.
    Item {
      width: parent.width
      height: panel.theme.chipHeight
      visible: panel.store.autoSource === "schedule"
      Row {
        anchors.right: parent.right
        spacing: panel.theme.spaceMedium
        Text {
          anchors.verticalCenter: parent.verticalCenter
          text: "󰖙  from"
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
        TimeField {
          objectName: "lightAt"
          committed: (panel.store.appearance.auto || {}).lightAt || "07:00"
          onChosen: value => panel.store.setAuto("schedule", value, (panel.store.appearance.auto || {}).darkAt)
        }
        Text {
          anchors.verticalCenter: parent.verticalCenter
          text: "󰖔  from"
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
        TimeField {
          objectName: "darkAt"
          committed: (panel.store.appearance.auto || {}).darkAt || "19:00"
          onChosen: value => panel.store.setAuto("schedule", (panel.store.appearance.auto || {}).lightAt, value)
        }
      }
    }
    // What the schedule will do next.
    Text {
      objectName: "schedule"
      width: parent.width
      visible: panel.store.autoSource !== "off" && text !== ""
      text: panel.store.scheduleText
      textFormat: Text.PlainText
      horizontalAlignment: Text.AlignRight
      wrapMode: Text.Wrap
      color: panel.theme.subtext
      font.family: panel.theme.fontFamily
      font.pixelSize: panel.theme.textCaption
    }

    // Which preset each mode wears, and the switcher that chooses them.
    Shared.SectionRule {
      theme: panel.theme
      width: parent.width
      label: "Theme for each mode"
      Shared.ActionButton {
        objectName: "browse"
        theme: panel.theme
        implicitHeight: panel.theme.chipHeight
        text: "Browse themes"
        onClicked: panel.browseRequested()
      }
    }
    SlotRow { slot: "light" }
    SlotRow { slot: "dark" }
    Text {
      width: parent.width
      text: "A preset picked in the switcher becomes the theme for its own mode. Use current gives the one on screen to the other mode as well."
      textFormat: Text.PlainText
      wrapMode: Text.Wrap
      color: panel.theme.overlay
      font.family: panel.theme.fontFamily
      font.pixelSize: panel.theme.textCaption
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
  }
}
