pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../shared" as Shared

// Bringing the old private library into the vault. The preview is the same
// walk the run performs, so the counts shown are the counts acted on, and the
// originals are left where they are until the result has been looked at.
ColumnLayout {
  id: migration

  required property var theme
  required property var store
  readonly property var report: migration.store.migration

  spacing: migration.theme.panelSpacing

  // There is nothing to preview until there is a folder to migrate into, so
  // the walk is asked for when this panel is actually being looked at.
  function preview() {
    if (migration.visible && migration.store.ready && migration.store.config.configured)
      migration.store.previewMigration()
  }

  onVisibleChanged: migration.preview()

  Connections {
    target: migration.store

    function onReadyChanged() { migration.preview() }
    function onConfigChanged() { migration.preview() }
  }

  Text {
    Layout.fillWidth: true
    text: "Seele Notes used to keep notes and voice memos in its own private folder. Each of them becomes a Markdown file in the capture folder, with its recordings copied beside it. Nothing in the old folder is removed."
    color: migration.theme.subtext
    font.family: migration.theme.fontFamily
    font.pixelSize: migration.theme.textBody
    wrapMode: Text.Wrap
  }

  Shared.SectionRule {
    Layout.fillWidth: true
    theme: migration.theme
    label: migration.report && migration.report.ran ? "MIGRATED" : "TO MIGRATE"
    detail: migration.report
      ? (migration.report.ran ? migration.report.migrated + " written" : migration.report.pending + " notes · " + migration.report.memos + " recordings")
      : "Reading…"

    Shared.ActionButton {
      theme: migration.theme
      text: migration.report && migration.report.pending ? "Migrate now" : "Re-check"
      selected: !!(migration.report && migration.report.pending)
      enabled: migration.store.ready
      onClicked: migration.report && migration.report.pending
        ? migration.store.runMigration()
        : migration.store.previewMigration()
    }
  }

  Text {
    Layout.fillWidth: true
    visible: !!(migration.report && migration.report.malformed)
    text: migration.report ? migration.report.malformed + " entr(y/ies) could not be read and were left untouched." : ""
    color: migration.theme.yellow
    font.family: migration.theme.fontFamily
    font.pixelSize: migration.theme.textCaption
    wrapMode: Text.Wrap
  }

  Shared.DeviceListCard {
    Layout.fillWidth: true
    theme: migration.theme
    visible: !!(migration.report && migration.report.items.length)
    listHeight: Math.min(migration.theme.notesPickerHeight, items.contentHeight)

    Shared.SeeleListView {
      id: items

      theme: migration.theme
      anchors.fill: parent
      anchors.margins: migration.theme.cardPadding
      clip: true
      spacing: migration.theme.spaceTight
      model: migration.report ? migration.report.items : []

      delegate: Rectangle {
        id: item

        required property var modelData

        width: items.width
        height: migration.theme.rowHeight
        radius: migration.theme.radiusSmall
        color: migration.theme.rowColor
        antialiasing: true

        RowLayout {
          anchors.fill: parent
          anchors.leftMargin: migration.theme.spaceMedium
          anchors.rightMargin: migration.theme.spaceMedium
          spacing: migration.theme.spaceMedium

          Text {
            text: item.modelData.state === "done" ? "󰄬" : item.modelData.state === "malformed" || item.modelData.state === "failed" ? "󰀪" : "󰁔"
            color: item.modelData.state === "done"
              ? migration.theme.green
              : item.modelData.state === "pending" ? migration.theme.overlay : migration.theme.red
            font.family: migration.theme.fontFamily
            font.pixelSize: migration.theme.textIcon
          }

          Text {
            Layout.fillWidth: true
            text: item.modelData.name || item.modelData.id
            elide: Text.ElideRight
            color: migration.theme.text
            font.family: migration.theme.fontFamily
            font.pixelSize: migration.theme.textBody
          }

          Text {
            visible: !!item.modelData.memos
            text: "󰍬 " + item.modelData.memos
            color: migration.theme.subtext
            font.family: migration.theme.fontFamily
            font.pixelSize: migration.theme.textCaption
          }

          Text {
            visible: !!item.modelData.trashed
            text: "Trash"
            color: migration.theme.overlay
            font.family: migration.theme.fontFamily
            font.pixelSize: migration.theme.textCaption
          }
        }
      }

      ScrollBar.vertical: Shared.SlimScrollBar { theme: migration.theme; popupHovered: true }
    }
  }

  Shared.EmptyState {
    Layout.fillWidth: true
    theme: migration.theme
    visible: !!(migration.report && !migration.report.present)
    glyph: "󰗠"
    title: "Nothing to migrate"
    detail: "There is no old Seele Notes library on this machine."
  }
}
