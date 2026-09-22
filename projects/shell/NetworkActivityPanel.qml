pragma ComponentBehavior: Bound
import QtQuick
import "../shared" as Shared

FocusScope {
  id: panel
  required property var theme
  required property var store
  readonly property var entry: store.selected
  implicitHeight: content.implicitHeight
  onActiveFocusChanged: if (activeFocus) selector.forceActiveFocus()

  Column {
    id: content
    width: parent.width
    spacing: panel.theme.panelSpacing
    Shared.StatusBanner {
      width: parent.width
      theme: panel.theme
      visible: panel.store.error !== ""
      title: panel.store.error
      Shared.ActionButton { theme: panel.theme; text: "Retry"; onClicked: panel.store.retry() }
    }
    Shared.SectionRule { width: parent.width; theme: panel.theme; label: "INTERFACE"; detail: panel.store.snapshot.limited ? "Up to " + panel.store.snapshot.interfaceLimit + " interfaces" : "Local kernel counters" }
    Rectangle {
      width: parent.width
      height: interfaceRow.implicitHeight + panel.theme.cardPadding * 2
      radius: panel.theme.radius
      color: panel.theme.cardColor
      Shared.CardEdge { theme: panel.theme }
      Row {
        id: interfaceRow
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
        spacing: panel.theme.spaceMedium
        Shared.ChoiceBox {
          id: selector
          objectName: "interfaceSelector"
          theme: panel.theme
          width: parent.width - connection.width - parent.spacing
          model: panel.store.rows
          textRole: "name"
          currentIndex: panel.store.selectedIndex
          displayText: panel.entry ? panel.entry.name : "Choose an interface"
          enabled: panel.store.rows.length > 0
          Accessible.name: "Network interface"
          onActivated: index => panel.store.select(index)
        }
        Shared.StatusChip {
          id: connection
          anchors.verticalCenter: parent.verticalCenter
          theme: panel.theme
          text: panel.entry ? panel.entry.state : "—"
          tint: panel.entry && panel.entry.state === "Up" ? panel.theme.green : panel.theme.subtext
        }
      }
    }
    Shared.EmptyState {
      width: parent.width
      theme: panel.theme
      visible: !panel.entry && panel.store.error === ""
      glyph: "󰛳"
      title: !panel.store.received ? "Reading interfaces…" : panel.store.rows.length ? "The selected interface disappeared" : "No interfaces available"
      detail: panel.store.rows.length ? "Choose an interface to view its activity." : "Interfaces will appear here when the kernel reports them."
    }
    Column {
      width: parent.width
      spacing: panel.theme.panelSpacing
      visible: !!panel.entry
      Shared.SectionRule { width: parent.width; theme: panel.theme; label: "LIVE ACTIVITY"; detail: "Receive ↓  ·  Send ↑" }
      Rectangle {
        width: parent.width
        height: activityContent.implicitHeight + panel.theme.cardPadding * 2
        radius: panel.theme.radius
        color: panel.theme.cardColor
        Shared.CardEdge { theme: panel.theme }
        Column {
          id: activityContent
          anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
          spacing: panel.theme.spaceLarge
          Row {
            width: parent.width
            spacing: panel.theme.spaceLarge
            Repeater {
              model: [ {title: "↓ RECEIVING", value: panel.entry ? panel.entry.rxLabel : "—", tint: panel.theme.green}, {title: "↑ SENDING", value: panel.entry ? panel.entry.txLabel : "—", tint: panel.theme.accent} ]
              Column {
                id: rate
                required property var modelData
                width: (parent.width - panel.theme.spaceLarge) / 2
                spacing: panel.theme.spaceTight
                Text { text: rate.modelData.title; color: rate.modelData.tint; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textLabel; font.weight: panel.theme.weightMedium; font.letterSpacing: panel.theme.trackingLabel }
                Text { text: rate.modelData.value; color: panel.theme.text; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textDisplay; font.weight: panel.theme.weightLight }
              }
            }
          }
          Row {
            width: parent.width
            Text { width: parent.width / 2; text: "SCALE  " + (panel.entry ? panel.entry.scaleLabel : "—"); color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption }
            Text { width: parent.width / 2; text: panel.store.snapshot.historyCapacity + " samples · newest at right"; horizontalAlignment: Text.AlignRight; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption }
          }
          Shared.HistoryChart {
            objectName: "trafficChart"
            width: parent.width
            height: panel.theme.controlHeight * 3
            theme: panel.theme
            capacity: panel.store.snapshot.historyCapacity
            maximum: panel.entry ? panel.entry.maximum : 1024
            series: panel.entry ? [{values: panel.entry.rx, color: panel.theme.green}, {values: panel.entry.tx, color: panel.theme.accent}] : []
          }
          Text {
            width: parent.width
            text: panel.entry ? panel.entry.status : ""
            wrapMode: Text.Wrap
            color: panel.entry && panel.entry.rxRate === null ? panel.theme.yellow : panel.theme.subtext
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textCaption
          }
        }
      }
      Shared.SectionRule {
        width: parent.width
        theme: panel.theme
        label: "SESSION TOTALS"
        detail: String(panel.store.snapshot.elapsed || 0) + " seconds"
        Shared.ActionButton {
          objectName: "resetActivity"
          theme: panel.theme
          height: panel.theme.chipHeight
          text: "Reset"
          Accessible.name: "Reset all interface totals and history"
          onClicked: panel.store.reset()
        }
      }
      Rectangle {
        width: parent.width
        height: totals.implicitHeight + panel.theme.cardPadding * 2
        radius: panel.theme.radius
        color: panel.theme.cardColor
        Shared.CardEdge { theme: panel.theme }
        Column {
          id: totals
          anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
          spacing: panel.theme.spaceMedium
          Row {
            width: parent.width
            Repeater {
              model: [{title: "Received", value: panel.entry ? panel.entry.rxTotal : "—"}, {title: "Sent", value: panel.entry ? panel.entry.txTotal : "—"}]
              Column {
                id: total
                required property var modelData
                width: parent.width / 2
                spacing: panel.theme.spaceTight
                Text { text: total.modelData.title; color: panel.theme.subtext; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textCaption }
                Text { text: total.modelData.value; color: panel.theme.text; font.family: panel.theme.fontFamily; font.pixelSize: panel.theme.textLead; font.weight: panel.theme.weightStrong }
              }
            }
          }
          Text {
            width: parent.width
            text: panel.entry && panel.entry.incomplete ? "Observed bytes only · counters were interrupted" : "Since this panel opened or the last Reset"
            color: panel.entry && panel.entry.incomplete ? panel.theme.yellow : panel.theme.subtext
            wrapMode: Text.Wrap
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textCaption
          }
        }
      }
    }
  }
}
