import QtQuick
import QtQuick.Controls
import "../shared" as Shared

Column {
  id: panel

  required property var theme
  required property var store
  property string tab: "integrations"
  property Component maintenanceContent: null
  property int maintenanceCount: 0
  property string confirmation: ""
  property string diagnostics: ""

  readonly property int healthyCount: panel.store.rows.length - panel.store.attentionCount

  spacing: theme.spaceMedium
  onMaintenanceContentChanged: if (!panel.maintenanceContent) panel.tab = "integrations"

  Shared.PanelHeader {
    theme: panel.theme
    width: parent.width
    glyph: "󰅚"
    title: "System Health"
    detail: panel.store.attentionCount + " integrations need attention"
  }

  // Which of the two views is being read is one exclusive choice, so it is one
  // well with the chosen side lit, not two outlined buttons side by side each
  // spending an outline to say what the lit fill already says.
  Shared.SegmentWell {
    theme: panel.theme
    width: parent.width
    visible: panel.maintenanceContent !== null

    Repeater {
      model: [
        { label: "Integrations", tab: "integrations" },
        { label: "Maintenance", tab: "maintenance" }
      ]

      Shared.Segment {
        id: healthView

        required property var modelData

        theme: panel.theme
        width: parent.width / 2
        selected: panel.tab === healthView.modelData.tab
        hovered: healthViewMouse.containsMouse
        pressed: healthViewMouse.pressed

        Text {
          anchors.centerIn: parent
          text: healthView.modelData.tab === "maintenance" && panel.maintenanceCount
            ? healthView.modelData.label + " · " + panel.maintenanceCount
            : healthView.modelData.label
          color: healthView.selected ? panel.theme.accent : panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textLabel
          font.weight: healthView.selected ? panel.theme.weightStrong : panel.theme.weightRegular
        }

        MouseArea {
          id: healthViewMouse

          anchors.fill: parent
          enabled: !healthView.selected
          hoverEnabled: true
          cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
          onClicked: panel.tab = healthView.modelData.tab
        }
      }
    }
  }

  Loader {
    width: parent.width
    active: panel.tab === "maintenance"
    visible: active
    sourceComponent: panel.maintenanceContent
  }

  Column {
    width: parent.width
    visible: panel.tab === "integrations"
    spacing: panel.theme.spaceSmall

    Shared.EmptyState {
      theme: panel.theme
      width: parent.width
      visible: panel.store.rows.length === 0
      glyph: "󰌘"
      title: "No integrations configured"
      detail: "An integration appears here once it registers itself through seele.health.providers."
    }

    Repeater {
      model: panel.store.model

      delegate: Column {
        id: entry

        required property var modelData
        required property int index

        readonly property bool healthy: entry.modelData.state === "healthy"
        // The model is sorted with everything needing attention first, so the
        // row that opens a group is simply the one whose predecessor sits on
        // the other side of that boundary.
        readonly property bool opensGroup: entry.index === 0
          || (panel.store.rows[entry.index - 1].state === "healthy") !== entry.healthy

        width: parent.width
        spacing: panel.theme.spaceSmall

        Shared.SectionRule {
          theme: panel.theme
          width: parent.width
          visible: entry.opensGroup
          label: entry.healthy ? "HEALTHY" : "NEEDS ATTENTION"
          detail: String(entry.healthy ? panel.healthyCount : panel.store.attentionCount)
          detailColor: entry.healthy ? panel.theme.overlay : panel.theme.yellow
        }

        Rectangle {
          id: card

          // A healthy entry is kept compact; one that needs attention carries
          // its detail and actions, so it takes the full card inset.
          readonly property real inset: entry.healthy ? panel.theme.spaceSmall : panel.theme.cardPadding

          width: parent.width
          implicitHeight: body.implicitHeight + card.inset * 2
          radius: panel.theme.radius
          color: panel.theme.cardColor

          Shared.CardEdge { theme: panel.theme }

          Column {
            id: body

            anchors { left: parent.left; right: parent.right; top: parent.top; margins: card.inset }
            spacing: panel.theme.spaceSmall

            Text {
              width: parent.width
              text: entry.modelData.name + " · " + entry.modelData.state
              textFormat: Text.PlainText
              wrapMode: Text.Wrap
              color: entry.healthy ? panel.theme.text : panel.theme.yellow
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textBody
              font.weight: panel.theme.weightStrong
            }

            Text {
              width: parent.width
              text: entry.modelData.summary
              textFormat: Text.PlainText
              wrapMode: Text.Wrap
              color: panel.theme.subtext
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }

            Text {
              width: parent.width
              text: entry.modelData.lastSuccess
                ? "Last successful update · " + Qt.formatDateTime(new Date(entry.modelData.lastSuccess), "yyyy-MM-dd HH:mm")
                : "No successful update yet"
              textFormat: Text.PlainText
              wrapMode: Text.Wrap
              color: panel.theme.overlay
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textMicro
            }

            Flow {
              width: parent.width
              spacing: panel.theme.spaceSmall

              Repeater {
                model: entry.modelData.actions

                delegate: Shared.ActionButton {
                  required property string modelData

                  theme: panel.theme
                  text: modelData === "settings"
                    ? "Open settings"
                    : modelData.charAt(0).toUpperCase() + modelData.slice(1)
                  enabled: !panel.store.pending[entry.modelData.id]
                  onClicked: {
                    var key = entry.modelData.id + ":" + modelData
                    if (modelData === "diagnostics")
                      panel.diagnostics = panel.diagnostics === entry.modelData.id ? "" : entry.modelData.id
                    else if (panel.store.registrations[entry.modelData.id].disruptive.indexOf(modelData) >= 0
                      && panel.confirmation !== key)
                      panel.confirmation = key
                    else {
                      panel.store.act(entry.modelData.id, modelData, panel.confirmation === key)
                      panel.confirmation = ""
                    }
                  }
                }
              }
            }

            Text {
              width: parent.width
              visible: panel.confirmation.indexOf(entry.modelData.id + ":") === 0
              text: "This may interrupt active work. Select the action again to confirm."
              wrapMode: Text.Wrap
              color: panel.theme.yellow
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }

            Text {
              width: parent.width
              visible: !!panel.store.pending[entry.modelData.id] || !!panel.store.errors[entry.modelData.id]
              text: panel.store.pending[entry.modelData.id]
                ? "Working…"
                : panel.store.errors[entry.modelData.id] || ""
              color: panel.theme.yellow
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }

            Text {
              width: parent.width
              visible: panel.diagnostics === entry.modelData.id
              text: entry.modelData.detail || "No additional diagnostics"
              textFormat: Text.PlainText
              wrapMode: Text.Wrap
              color: panel.theme.subtext
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }
          }
        }
      }
    }
  }
}
