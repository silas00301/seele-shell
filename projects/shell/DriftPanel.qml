pragma ComponentBehavior: Bound
import QtQuick
import "../shared" as Shared

// Before and after for the three checks the flake publishes. Inclusion is a
// switch; the words on each row are the worker's. This panel never names a
// system command.
Column {
  id: panel

  required property var theme
  required property var store
  spacing: panel.theme.panelSpacing
  readonly property string hint: "Escape closes"
  readonly property int selected: {
    var count = 0
    var rows = panel.store.checks || []
    for (var i = 0; i < rows.length; i++) {
      var item = rows[i]
      if (!item || !item.drifted || item.unavailable) continue
      if (panel.store.skipped.indexOf(item.id) >= 0) continue
      count++
    }
    return count
  }

  Shared.StatusBanner {
    theme: panel.theme
    width: parent.width
    visible: panel.store.error !== ""
    glyph: "󰀦"
    tint: panel.theme.yellow
    title: "Fix me could not read the machine"
    detail: panel.store.error
  }

  Shared.StatusBanner {
    theme: panel.theme
    width: parent.width
    visible: panel.store.restored.length > 0 && panel.store.error === ""
    glyph: "󰄬"
    tint: panel.theme.green
    title: "Restored"
    detail: panel.store.restored.join(", ")
  }

  Repeater {
    model: panel.store.checks
    delegate: Rectangle {
      id: row
      required property var modelData
      objectName: "driftRow"
      width: panel.width
      implicitHeight: body.implicitHeight + panel.theme.cardPadding * 2
      radius: panel.theme.radius
      color: panel.theme.cardColor

      Shared.CardEdge { theme: panel.theme }

      Column {
        id: body
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
        spacing: panel.theme.spaceSmall

        Item {
          width: parent.width
          implicitHeight: Math.max(title.implicitHeight, chip.implicitHeight, include.implicitHeight)

          Text {
            id: title
            anchors.left: parent.left
            anchors.right: chip.left
            anchors.rightMargin: panel.theme.spaceSmall
            anchors.verticalCenter: parent.verticalCenter
            text: row.modelData.title
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: panel.theme.text
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textBody
            font.weight: panel.theme.weightMedium
          }

          Shared.StatusChip {
            id: chip
            anchors.right: include.visible ? include.left : parent.right
            anchors.rightMargin: include.visible ? panel.theme.spaceSmall : 0
            anchors.verticalCenter: parent.verticalCenter
            theme: panel.theme
            text: row.modelData.unavailable ? "Unavailable" : row.modelData.drifted ? "Drifted" : "Matches"
            tint: row.modelData.unavailable ? panel.theme.overlay : row.modelData.drifted ? panel.theme.yellow : panel.theme.green
          }

          Shared.ControlSwitch {
            id: include
            objectName: "driftInclude"
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: row.modelData.drifted && !row.modelData.unavailable
            theme: panel.theme
            checked: panel.store.skipped.indexOf(row.modelData.id) < 0
            enabled: !panel.store.busy
            onToggled: panel.store.toggle(row.modelData.id)
          }
        }

        Text {
          width: parent.width
          text: "Now"
          textFormat: Text.PlainText
          color: panel.theme.overlay
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
          font.weight: panel.theme.weightMedium
        }
        Text {
          objectName: "driftBefore"
          width: parent.width
          text: row.modelData.before
          textFormat: Text.PlainText
          wrapMode: Text.Wrap
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
        Text {
          width: parent.width
          text: "Flake"
          textFormat: Text.PlainText
          color: panel.theme.overlay
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
          font.weight: panel.theme.weightMedium
        }
        Text {
          objectName: "driftAfter"
          width: parent.width
          text: row.modelData.after
          textFormat: Text.PlainText
          wrapMode: Text.Wrap
          color: panel.theme.text
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
        Text {
          width: parent.width
          visible: row.modelData.drifted && row.modelData.restore !== ""
          text: row.modelData.restore
          textFormat: Text.PlainText
          wrapMode: Text.Wrap
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
      }
    }
  }

  Shared.EmptyState {
    theme: panel.theme
    width: parent.width
    visible: panel.store.checks.length === 0 && panel.store.error === ""
    glyph: panel.store.busy ? "󰂚" : "󰄬"
    title: panel.store.busy ? "Checking the machine…" : "Nothing to compare yet"
    detail: panel.store.busy ? "" : "Open the panel again to read Quad9, Podman and the remote shell."
  }

  Row {
    spacing: panel.theme.spaceSmall
    Shared.ActionButton {
      objectName: "driftCheckAgain"
      theme: panel.theme
      text: "Check again"
      enabled: !panel.store.busy
      onClicked: panel.store.diff()
    }
    Shared.ActionButton {
      objectName: "driftRestore"
      theme: panel.theme
      text: panel.selected === 1 ? "Restore 1" : "Restore " + panel.selected
      selected: panel.selected > 0
      enabled: panel.selected > 0 && !panel.store.busy
      onClicked: panel.store.apply()
    }
  }
}
