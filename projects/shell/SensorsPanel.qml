pragma ComponentBehavior: Bound
import QtQuick
import "../shared" as Shared

// Temperatures and fans as the kernel reports them. Every value, limit, state
// and name arrives formatted from the worker; this file only lays them out and
// picks a tint for the state it was given.
FocusScope {
  id: panel
  required property var theme
  required property var store
  readonly property var snapshot: store.snapshot
  implicitHeight: content.implicitHeight

  function tint(state) {
    if (state === "critical") return panel.theme.red
    if (state === "high" || state === "alarm" || state === "fault") return panel.theme.yellow
    if (state === "normal") return panel.theme.text
    return panel.theme.subtext
  }
  component Caption: Text {
    color: panel.theme.subtext
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textCaption
    textFormat: Text.PlainText
    wrapMode: Text.Wrap
  }

  Column {
    id: content
    width: parent.width
    spacing: panel.theme.panelSpacing
    Shared.StatusBanner {
      width: parent.width
      theme: panel.theme
      visible: panel.store.error !== ""
      title: panel.store.error
      Shared.ActionButton { objectName: "retrySensors"; theme: panel.theme; text: "Retry"; onClicked: panel.store.retry() }
    }
    Shared.EmptyState {
      width: parent.width
      theme: panel.theme
      visible: panel.store.error === "" && panel.store.rows.length === 0
      glyph: "󰔏"
      title: !panel.store.received ? "Reading sensors…" : "No sensors reported"
      detail: !panel.store.received ? "" : "The kernel publishes no temperatures or fans here. Some GPU drivers, NVIDIA's among them, report outside the kernel's sensor interface."
    }
    Repeater {
      model: panel.store.rows
      Column {
        id: device
        required property var modelData
        width: content.width
        spacing: panel.theme.panelSpacing
        Shared.SectionRule { width: parent.width; theme: panel.theme; label: device.modelData.title.toUpperCase(); detail: device.modelData.detail }
        Rectangle {
          width: parent.width
          height: readings.implicitHeight + panel.theme.cardPadding * 2
          radius: panel.theme.radius
          color: panel.theme.cardColor
          Shared.CardEdge { theme: panel.theme }
          Column {
            id: readings
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
            spacing: panel.theme.spaceMedium
            Repeater {
              model: device.modelData.readings
              Column {
                id: reading
                required property var modelData
                required property int index
                objectName: device.modelData.id + "/" + reading.modelData.id
                width: readings.width
                spacing: panel.theme.spaceSmall
                Rectangle { visible: reading.index > 0; width: parent.width; height: panel.theme.hairline; color: panel.theme.separatorColor }
                Item {
                  width: parent.width
                  height: Math.max(names.implicitHeight, figures.implicitHeight)
                  Column {
                    id: names
                    anchors { left: parent.left; right: figures.left; rightMargin: panel.theme.spaceMedium; verticalCenter: parent.verticalCenter }
                    spacing: panel.theme.spaceTight
                    Text { width: parent.width; text: reading.modelData.label; textFormat: Text.PlainText; elide: Text.ElideRight; color: panel.theme.text; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textBody; font.weight: panel.theme.weightMedium }
                    Caption {
                      objectName: "detail"
                      width: parent.width
                      text: "Peak " + reading.modelData.peak + "  ·  " + (reading.modelData.limits !== "" ? reading.modelData.limits : reading.modelData.kind === "fan" ? "No minimum reported" : "No limits reported")
                    }
                  }
                  Row {
                    id: figures
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    spacing: panel.theme.spaceMedium
                    Shared.StatusChip {
                      objectName: "status"
                      visible: reading.modelData.status !== ""
                      anchors.verticalCenter: parent.verticalCenter
                      theme: panel.theme
                      text: reading.modelData.status
                      tint: panel.tint(reading.modelData.state)
                    }
                    Text {
                      objectName: "value"
                      anchors.verticalCenter: parent.verticalCenter
                      text: reading.modelData.value
                      textFormat: Text.PlainText
                      color: panel.tint(reading.modelData.state)
                      font.family: panel.theme.fontFamily
                      font.pixelSize: panel.theme.textLead
                      font.weight: panel.theme.weightStrong
                    }
                  }
                }
                // The meter only exists against a limit the driver states;
                // without one there is nothing to measure the reading against.
                Shared.MeterBar {
                  objectName: "meter"
                  visible: typeof reading.modelData.ratio === "number"
                  width: parent.width
                  theme: panel.theme
                  ratio: visible ? reading.modelData.ratio : 0
                  fill: reading.modelData.state === "critical" ? panel.theme.red : reading.modelData.state === "high" ? panel.theme.yellow : panel.theme.accent
                }
              }
            }
          }
        }
      }
    }
    Shared.SectionRule {
      width: parent.width
      theme: panel.theme
      visible: panel.store.rows.length > 0
      label: "SESSION"
      detail: "Peaks over " + (panel.snapshot.elapsed || 0) + " seconds"
      Shared.ActionButton {
        objectName: "resetSensors"
        theme: panel.theme
        height: panel.theme.chipHeight
        text: "Reset peaks"
        Accessible.name: "Reset every peak and start a new session"
        onClicked: panel.store.reset()
      }
    }
    Caption {
      width: parent.width
      visible: panel.store.received
      text: "Read from the kernel every " + (panel.snapshot.cadenceSeconds || 2) + " seconds while this panel is open. Limits are the driver's own; nothing is kept after closing."
        + (panel.snapshot.limited ? " Only the first " + panel.snapshot.deviceLimit + " devices and their first channels are shown." : "")
    }
    Caption {
      width: parent.width
      visible: (panel.snapshot.skipped || 0) > 0
      text: panel.snapshot.skipped === 1
        ? "One disk is left alone: reading a drive's temperature can keep it from spinning down."
        : panel.snapshot.skipped + " disks are left alone: reading a drive's temperature can keep it from spinning down."
    }
  }
}
