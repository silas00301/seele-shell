import QtQuick
import QtQuick.Controls
import "../shared" as Shared

Column {
  id: panel

  required property var theme
  required property var store
  property bool showSnoozed: false
  property bool showHistory: false

  spacing: theme.spaceMedium

  function urgencyLabel(value) {
    return ({
      now: "Action required now",
      soon: "Action required soon",
      eventually: "Action required eventually",
      informational: "Informational"
    })[value] || value
  }

  // The maintenance service being unreachable is the panel's own bad news
  // rather than any one finding's, so it is a banner carrying the way out of
  // it instead of a loose red line above the list.
  Shared.StatusBanner {
    theme: panel.theme
    width: parent.width
    visible: panel.store.error !== ""
    glyph: "󰀪"
    title: panel.store.error
    detail: "Findings shown here are unavailable until the service answers."

    Shared.ActionButton {
      theme: panel.theme
      text: "Retry"
      onClicked: panel.store.refresh()
    }
  }

  Shared.EmptyState {
    theme: panel.theme
    width: parent.width
    visible: panel.store.error === "" && panel.store.activeModel.count === 0
    glyph: "󰄬"
    title: "No active maintenance findings"
    detail: "Everything the maintenance service checks is currently in order."
  }

  Repeater {
    model: Object.keys(panel.store.snapshot.checkErrors || {})

    Text {
      required property string modelData

      width: panel.width
      text: modelData + " · " + panel.store.snapshot.checkErrors[modelData]
      textFormat: Text.PlainText
      wrapMode: Text.Wrap
      color: panel.theme.yellow
      font.family: panel.theme.fontFamily
      font.pixelSize: panel.theme.textCaption
    }
  }

  component FindingCard: Rectangle {
    id: card

    required property var finding

    readonly property bool expanded: !!panel.store.expandedIds[finding.id]

    width: panel.width
    implicitHeight: body.implicitHeight + panel.theme.cardPadding * 2
    radius: panel.theme.radius
    color: panel.theme.cardColor

    Shared.CardEdge { theme: panel.theme }

    Column {
      id: body

      anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
      spacing: panel.theme.spaceSmall

      Text {
        width: parent.width
        text: card.finding.title
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: panel.theme.text
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textBody
        font.weight: panel.theme.weightStrong
      }

      // Urgency is graded rather than binary: what has to be done now is red,
      // what has to be done soon is yellow, and the rest stays quiet.
      Text {
        width: parent.width
        text: panel.urgencyLabel(card.finding.urgency) + " · " + card.finding.source
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: card.finding.urgency === "now"
          ? panel.theme.red
          : card.finding.urgency === "soon" ? panel.theme.yellow : panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }

      Text {
        width: parent.width
        text: card.finding.explanation
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }

      Shared.ActionButton {
        theme: panel.theme
        text: card.expanded ? "Less" : "Details"
        onClicked: {
          var ids = Object.assign({}, panel.store.expandedIds)
          ids[card.finding.id] = !card.expanded
          panel.store.expandedIds = ids
        }
      }

      Text {
        width: parent.width
        visible: card.expanded
        text: card.finding.details
          + "\nFirst seen · " + Qt.formatDateTime(new Date(card.finding.firstSeen * 1000), "yyyy-MM-dd HH:mm")
          + "\nUpdated · " + Qt.formatDateTime(new Date(card.finding.updated * 1000), "yyyy-MM-dd HH:mm")
          + (card.finding.recurrence ? "\nRecurrences · " + card.finding.recurrence : "")
          + (card.finding.resolved ? "\nResolved · " + Qt.formatDateTime(new Date(card.finding.resolved * 1000), "yyyy-MM-dd HH:mm") : "")
          + (card.finding.snoozedUntil > Date.now() / 1000
            ? "\nSnoozed until · " + Qt.formatDateTime(new Date(card.finding.snoozedUntil * 1000), "yyyy-MM-dd HH:mm") : "")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }

      Flow {
        width: parent.width
        spacing: panel.theme.spaceSmall
        visible: !card.finding.resolved
        enabled: !card.finding.busy && panel.store.pendingId === "" && panel.store.error === ""

        Repeater {
          model: card.finding.actions

          Shared.ActionButton {
            required property var modelData

            theme: panel.theme
            text: modelData.label
            onClicked: panel.store.repair(card.finding, modelData, false)
          }
        }

        Shared.ActionButton {
          theme: panel.theme
          visible: card.finding.canAnalyze
          text: "Analyze with AI"
          onClicked: panel.store.request(card.finding, "analyze")
        }

        Shared.ActionButton {
          theme: panel.theme
          visible: card.finding.lifecycle === "notice"
          text: "Done"
          onClicked: panel.store.request(card.finding, "done")
        }

        Shared.ActionButton {
          theme: panel.theme
          visible: card.finding.snoozedUntil > Date.now() / 1000
          text: "Unsnooze"
          onClicked: panel.store.request(card.finding, "unsnooze")
        }
      }

      Flow {
        width: parent.width
        spacing: panel.theme.spaceSmall
        visible: card.expanded && !card.finding.resolved
        enabled: !card.finding.busy && panel.store.pendingId === "" && panel.store.error === ""

        Repeater {
          model: [
            { label: "1 hour", seconds: 3600 },
            { label: "1 day", seconds: 86400 },
            { label: "1 week", seconds: 604800 }
          ]

          Shared.ActionButton {
            required property var modelData

            theme: panel.theme
            text: "Snooze " + modelData.label
            onClicked: panel.store.request(card.finding, "snooze", { seconds: modelData.seconds })
          }
        }

        TextField {
          id: customMinutes

          // The field takes the same control height as the buttons it sits
          // beside, so the row of snooze controls keeps one line.
          width: panel.theme.controlHeight * 4
          height: panel.theme.controlHeight
          placeholderText: "Snooze minutes"
          validator: IntValidator { bottom: 1; top: 43200 }
          color: panel.theme.text
          placeholderTextColor: panel.theme.overlay
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption

          // A field holds a query rather than content, so it is cut back to
          // the ink like every other well in the shell.
          background: Rectangle {
            radius: panel.theme.radius
            color: panel.theme.wellColor
            border.width: 1
            border.color: customMinutes.activeFocus ? panel.theme.accent : panel.theme.cardBorder
          }
        }

        Shared.ActionButton {
          theme: panel.theme
          text: "Snooze"
          enabled: customMinutes.acceptableInput
          onClicked: panel.store.request(card.finding, "snooze", { seconds: Number(customMinutes.text) * 60 })
        }
      }

      Column {
        width: parent.width
        spacing: panel.theme.spaceSmall
        visible: !!panel.store.confirmation && panel.store.confirmation.id === card.finding.id

        Text {
          width: parent.width
          text: panel.store.confirmation && panel.store.confirmation.proposed
            ? "Review this AI-proposed action before allowing it to run."
            : "Confirm this repair before it runs. It may interrupt active work."
          wrapMode: Text.Wrap
          color: panel.theme.yellow
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }

        Row {
          spacing: panel.theme.spaceSmall
          enabled: !card.finding.busy && panel.store.pendingId === "" && panel.store.error === ""

          Shared.ActionButton {
            theme: panel.theme
            text: "Confirm " + (panel.store.confirmation ? panel.store.confirmation.label : "")
            danger: true
            onClicked: panel.store.confirm()
          }

          Shared.ActionButton {
            theme: panel.theme
            text: "Cancel"
            onClicked: panel.store.confirmation = null
          }
        }
      }

      Text {
        width: parent.width
        visible: !!card.finding.busy || panel.store.pendingId === card.finding.id
          || panel.store.actionErrorId === card.finding.id
        text: card.finding.busy || panel.store.pendingId === card.finding.id
          ? "Working…"
          : panel.store.actionError
        wrapMode: Text.Wrap
        color: panel.theme.yellow
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }

      Text {
        width: parent.width
        visible: card.finding.outcomes.length > 0
        text: card.finding.outcomes.length
          ? card.finding.outcomes[card.finding.outcomes.length - 1].action
            + " · " + card.finding.outcomes[card.finding.outcomes.length - 1].result
          : ""
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }

      Column {
        width: parent.width
        spacing: panel.theme.spaceSmall
        visible: !!card.finding.analysis

        Text {
          width: parent.width
          text: card.finding.analysis
            ? (card.finding.analysisStale ? "Previous analysis · finding changed\n" : "AI analysis\n")
              + card.finding.analysis.cause
              + "\n" + card.finding.analysis.evidence.join("\n")
              + "\n" + card.finding.analysis.nextSteps.join("\n")
            : ""
          textFormat: Text.PlainText
          wrapMode: Text.Wrap
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }

        Flow {
          width: parent.width
          spacing: panel.theme.spaceSmall
          visible: !card.finding.analysisStale && !card.finding.resolved
          enabled: !card.finding.busy && panel.store.pendingId === "" && panel.store.error === ""

          Repeater {
            model: card.finding.analysis ? card.finding.analysis.actions : []

            Shared.ActionButton {
              required property string modelData

              readonly property var proposed: card.finding.actions.find(function(a) { return a.id === modelData })

              theme: panel.theme
              text: proposed ? "Review " + proposed.label : "Unavailable action"
              enabled: !!proposed
              onClicked: panel.store.repair(card.finding, proposed, true)
            }
          }
        }
      }
    }
  }

  Shared.SectionRule {
    theme: panel.theme
    width: parent.width
    visible: panel.store.activeModel.count > 0
    label: "ACTIVE"
    detail: String(panel.store.activeModel.count)
  }

  Repeater {
    model: panel.store.activeModel

    FindingCard { required property var modelData; finding: modelData }
  }

  // A fold is opened by its own rule, with the count of what it holds at the
  // far end, rather than by a button that has to borrow `selected` to say
  // whether it is open.
  Shared.SectionRule {
    theme: panel.theme
    width: parent.width
    visible: panel.store.snoozedModel.count > 0
    label: "SNOOZED"
    detail: String(panel.store.snoozedModel.count)
    collapsible: true
    expanded: panel.showSnoozed
    onToggled: panel.showSnoozed = !panel.showSnoozed
  }

  Column {
    width: parent.width
    visible: panel.showSnoozed && panel.store.snoozedModel.count > 0
    spacing: panel.theme.spaceSmall

    Repeater {
      model: panel.store.snoozedModel

      FindingCard { required property var modelData; finding: modelData }
    }
  }

  Shared.SectionRule {
    theme: panel.theme
    width: parent.width
    visible: panel.store.historyModel.count > 0
    label: "HISTORY"
    detail: String(panel.store.historyModel.count)
    collapsible: true
    expanded: panel.showHistory
    onToggled: panel.showHistory = !panel.showHistory
  }

  Column {
    width: parent.width
    visible: panel.showHistory && panel.store.historyModel.count > 0
    spacing: panel.theme.spaceSmall

    Repeater {
      model: panel.store.historyModel

      FindingCard { required property var modelData; finding: modelData }
    }
  }
}
