import QtQuick
import "../shared" as Shared
import "focus.js" as Focus

Column {
  id: panel
  required property var theme
  required property var timer
  signal closeRequested()
  spacing: panel.theme.panelSpacing
  Keys.onEscapePressed: panel.closeRequested()
  Keys.onPressed: event => {
    if (customMinutes.activeFocus) return
    if (event.isAutoRepeat || event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
    if (event.key === Qt.Key_1) panel.timer.command("start", 25)
    else if (event.key === Qt.Key_2) panel.timer.command("start", 50)
    else if (event.key === Qt.Key_3) panel.timer.command("start", 5)
    else if (event.key === Qt.Key_Plus) panel.timer.command("extend")
    else if (event.key === Qt.Key_Delete) panel.timer.command("cancel")
    else if (event.key === Qt.Key_Space) {
      if (panel.timer.timerState.status === "running") panel.timer.command("pause")
      else if (panel.timer.timerState.status === "paused") panel.timer.command("resume")
      else panel.timer.command("start", 25)
    } else return
    event.accepted = true
  }
  Shared.PanelHeader {
    theme: panel.theme
    width: parent.width
    glyph: "󰔟"
    title: "Focus timer"
    detail: panel.timer.timerState.status === "idle" ? "Focus, then take a break"
      : panel.timer.timerState.status === "done" ? "Time is up"
        : panel.timer.timerState.status === "paused" ? "Paused"
          : "In progress · " + Math.round(panel.timer.timerState.duration / 60) + " min"
    detailColor: panel.timer.timerState.status === "done" ? panel.theme.green : panel.theme.subtext
  }

  // The remaining time is the panel's subject, so it is read off one
  // hero numeral with the meter beneath it saying how much of the
  // session is left. Idle has nothing to report and draws neither.
  Rectangle {
    width: parent.width
    height: focusReadout.implicitHeight + panel.theme.cardPadding * 2
    radius: panel.theme.radius
    color: panel.theme.cardColor
    antialiasing: true

    Shared.CardEdge { theme: panel.theme }

    Column {
      id: focusReadout

      anchors.left: parent.left
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      anchors.margins: panel.theme.cardPadding
      spacing: panel.theme.spaceMedium

      Text {
        width: parent.width
        text: panel.timer.label
        color: panel.timer.timerState.status === "done" ? panel.theme.green : panel.theme.accent
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textHero
        font.weight: panel.theme.weightLight
        horizontalAlignment: Text.AlignHCenter
      }

      Shared.MeterBar {
        theme: panel.theme
        width: parent.width
        visible: panel.timer.timerState.status !== "idle"
        ratio: panel.timer.timerState.duration > 0
          ? panel.timer.timerState.remaining / panel.timer.timerState.duration : 0
        fill: panel.timer.timerState.status === "done" ? panel.theme.green : panel.theme.accent

        Behavior on ratio { NumberAnimation { duration: panel.theme.durationFast } }
      }
    }
  }

  Shared.SectionRule {
    theme: panel.theme
    width: parent.width
    label: "SESSION"
    detail: "1 / 2 / 3"
  }

  // The three durations are one choice, and while a session runs the
  // one it was started from stays lit, so the well doubles as the
  // report of what is running.
  Shared.SegmentWell {
    theme: panel.theme
    width: parent.width
    height: panel.theme.controlHeight

    Repeater {
      model: [{ minutes: 25, label: "25 min" }, { minutes: 50, label: "50 min" }, { minutes: 5, label: "5 min break" }]

      Shared.SegmentChoice {
        theme: panel.theme
        id: focusPreset

        required property var modelData

        objectName: "focusPreset" + modelData.minutes
        width: parent.width / 3
        height: parent.height
        text: modelData.label
        selected: panel.timer.timerState.status !== "idle"
          && panel.timer.timerState.duration === modelData.minutes * 60
        onClicked: panel.timer.command("start", focusPreset.modelData.minutes)
      }
    }
  }

  Row {
    width: parent.width
    spacing: panel.theme.spaceSmall
    Shared.ValueField {
      id: customMinutes
      objectName: "focusCustomMinutes"
      theme: panel.theme
      width: parent.width - customStart.width - parent.spacing
      placeholderText: "Custom minutes"
      Accessible.name: "Custom focus duration in minutes"
      inputMethodHints: Qt.ImhDigitsOnly
      maximumLength: 16
      Keys.onPressed: event => {
        if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
        if (!event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) && customInput.valid) panel.startCustom()
        event.accepted = true
      }
    }
    Shared.ActionButton {
      id: customStart
      objectName: "focusCustomStart"
      theme: panel.theme
      text: "Start custom"
      enabled: customInput.valid
      onClicked: panel.startCustom()
    }
  }
  readonly property var customInput: Focus.customInput(customMinutes.text)
  function startCustom() {
    panel.timer.command("custom", customMinutes.text)
    panel.forceActiveFocus()
  }
  Text {
    width: parent.width
    text: customInput.hint
    color: customMinutes.text.trim() !== "" && !customInput.valid ? panel.theme.red : panel.theme.overlay
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
  }

  Row {
    width: parent.width
    spacing: panel.theme.spaceSmall
    Shared.ActionButton {
      theme: panel.theme
      width: (parent.width - parent.spacing * 2) / 3
      text: panel.timer.timerState.status === "running" ? "Pause"
        : panel.timer.timerState.status === "paused" ? "Resume" : "Start"
      onClicked: {
        if (panel.timer.timerState.status === "running") panel.timer.command("pause")
        else if (panel.timer.timerState.status === "paused") panel.timer.command("resume")
        else panel.timer.command("start", 25)
      }
    }
    Shared.ActionButton {
      objectName: "focusExtend"
      theme: panel.theme
      width: (parent.width - parent.spacing * 2) / 3
      text: "+5 min"
      enabled: panel.timer.canExtend
      onClicked: panel.timer.command("extend")
    }
    Shared.ActionButton {
      theme: panel.theme
      width: (parent.width - parent.spacing * 2) / 3
      text: panel.timer.timerState.status === "done" ? "Done" : "Cancel"
      danger: panel.timer.timerState.status !== "done"
      onClicked: panel.timer.command("cancel")
    }
  }
  Text {
    width: parent.width
    text: panel.timer.extensionHint || "Space pauses and resumes · + adds 5 min · Delete cancels"
    color: panel.theme.overlay
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
    wrapMode: Text.Wrap
  }
}
