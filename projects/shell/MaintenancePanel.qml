import QtQuick
import QtQuick.Controls
import "../shared" as Shared

Column {
  id: panel

  required property var theme
  required property var store
  property bool showSnoozed: false
  property bool showHistory: false

  // Each check that could not run is the service's own bad news rather than
  // any one finding's, so they are gathered into one banner instead of a run
  // of loose yellow lines above the list.
  readonly property var checkErrors: Object.keys(panel.store.snapshot.checkErrors || {})
    .map(function(name) { return name + " · " + panel.store.snapshot.checkErrors[name] })

  spacing: theme.spaceMedium

  function urgencyLabel(value) {
    return ({
      now: "Action required now",
      soon: "Action required soon",
      eventually: "Action required eventually",
      informational: "Informational"
    })[value] || value
  }

  // Urgency is graded rather than binary, so one mark carries every finding
  // that asks for something and its colour says how soon: red now, yellow
  // soon, and quiet for what can wait. Only a finding that asks for nothing
  // takes a different mark, and a resolved one is simply done.
  function urgencyGlyph(finding) {
    return finding.resolved ? "󰄬" : finding.urgency === "informational" ? "󰋼" : "󰀪"
  }

  function urgencyTint(finding) {
    if (finding.resolved) return panel.theme.green
    return finding.urgency === "now"
      ? panel.theme.red
      : finding.urgency === "soon" ? panel.theme.yellow : panel.theme.subtext
  }

  function stamp(seconds) {
    return Qt.formatDateTime(new Date(seconds * 1000), "yyyy-MM-dd HH:mm")
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

  Shared.StatusBanner {
    theme: panel.theme
    width: parent.width
    visible: panel.checkErrors.length > 0
    tint: panel.theme.yellow
    glyph: "󰀪"
    title: panel.checkErrors.length === 1
      ? "A maintenance check could not run"
      : panel.checkErrors.length + " maintenance checks could not run"
    detail: panel.checkErrors.join("\n")
  }

  Shared.EmptyState {
    theme: panel.theme
    width: parent.width
    visible: panel.store.error === "" && panel.store.activeModel.count === 0
    glyph: "󰄬"
    title: "No active maintenance findings"
    detail: "Everything the maintenance service checks is currently in order."
  }

  // Machine output — the check's own detail, an AI reading of it — is cut back
  // to the ink like every other well, so it reads as something quoted into the
  // card rather than as more of the card's own text.
  component Well: Rectangle {
    default property alias content: wellColumn.data

    width: parent ? parent.width : 0
    implicitHeight: wellColumn.implicitHeight + panel.theme.spaceMedium * 2
    radius: panel.theme.radiusSmall
    color: panel.theme.wellColor
    antialiasing: true

    Column {
      id: wellColumn

      anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.spaceMedium }
      spacing: panel.theme.spaceTight
    }
  }

  component FindingCard: Rectangle {
    id: card

    required property var finding

    readonly property bool expanded: !!panel.store.expandedIds[finding.id]
    readonly property bool working: !!card.finding.busy || panel.store.pendingId === card.finding.id
    readonly property bool actionable: !card.finding.busy && panel.store.pendingId === "" && panel.store.error === ""
    readonly property var metadata: {
      var rows = [
        { label: "First seen", value: panel.stamp(card.finding.firstSeen) },
        { label: "Updated", value: panel.stamp(card.finding.updated) }
      ]
      if (card.finding.recurrence) rows.push({ label: "Recurrences", value: String(card.finding.recurrence) })
      if (card.finding.resolved) rows.push({ label: "Resolved", value: panel.stamp(card.finding.resolved) })
      if (card.finding.snoozedUntil > Date.now() / 1000)
        rows.push({ label: "Snoozed until", value: panel.stamp(card.finding.snoozedUntil) })
      return rows
    }

    function toggle() {
      var ids = Object.assign({}, panel.store.expandedIds)
      ids[card.finding.id] = !card.expanded
      panel.store.expandedIds = ids
    }

    width: panel.width
    implicitHeight: body.implicitHeight + panel.theme.cardPadding * 2
    radius: panel.theme.radius
    color: panel.theme.cardColor

    Shared.CardEdge { theme: panel.theme }

    Column {
      id: body

      anchors { left: parent.left; right: parent.right; top: parent.top; margins: panel.theme.cardPadding }
      spacing: panel.theme.spaceSmall

      // The card leads with its urgency rather than restating it as a coloured
      // sentence under the title, and the whole head is what opens the fold, so
      // the finding is its own disclosure instead of carrying a button that
      // says what the chevron already says.
      Item {
        id: head

        width: parent.width
        implicitHeight: Math.max(headMark.height, headText.implicitHeight)
        // The head replaced a button, so it has to answer the keyboard the
        // way that button did: a finding whose details cannot be reached
        // without a pointer is a finding half the panel cannot read.
        activeFocusOnTab: true

        Keys.onPressed: event => {
          if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && event.key !== Qt.Key_Space) return
          event.accepted = true
          if (event.isAutoRepeat || (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) return
          card.toggle()
        }

        Rectangle {
          visible: head.activeFocus
          anchors.fill: parent
          radius: panel.theme.radiusSmall
          color: "transparent"
          border.width: 1
          border.color: panel.theme.accent
          antialiasing: true
        }

        Rectangle {
          id: headMark

          anchors.left: parent.left
          anchors.top: parent.top
          width: panel.theme.chipHeight - 2
          height: width
          radius: panel.theme.radiusSmall
          color: panel.theme.alpha(panel.urgencyTint(card.finding), 0.12)
          border.width: 1
          border.color: panel.theme.alpha(panel.urgencyTint(card.finding), 0.24)
          antialiasing: true

          Shared.CenteredGlyph {
            anchors.fill: parent
            text: panel.urgencyGlyph(card.finding)
            color: panel.urgencyTint(card.finding)
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textSubhead
          }
        }

        Column {
          id: headText

          anchors.left: headMark.right
          anchors.leftMargin: panel.theme.spaceMedium
          anchors.right: headChevron.left
          anchors.rightMargin: panel.theme.spaceSmall
          anchors.top: parent.top
          spacing: 1

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

          Text {
            width: parent.width
            text: panel.urgencyLabel(card.finding.urgency) + " · " + card.finding.source
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: panel.theme.subtext
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textCaption
          }
        }

        Text {
          id: headChevron

          anchors.right: parent.right
          anchors.verticalCenter: headMark.verticalCenter
          text: card.expanded ? "󰅃" : "󰅀"
          color: headMouse.containsMouse || head.activeFocus ? panel.theme.text : panel.theme.overlay
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textBody

          Behavior on color { ColorAnimation { duration: panel.theme.durationFast } }
        }

        MouseArea {
          id: headMouse

          anchors.fill: parent
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: card.toggle()
        }
      }

      Text {
        width: parent.width
        visible: text !== ""
        text: card.finding.explanation
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }

      // The fold grows the card it is already in rather than inserting rows
      // around it, so the finding stays under the pointer that opened it.
      Item {
        width: parent.width
        height: card.expanded ? detail.implicitHeight : 0
        visible: height > 0
        clip: true

        Behavior on height { NumberAnimation { duration: panel.theme.durationNormal; easing.type: Easing.OutCubic } }

        Column {
          id: detail

          anchors { left: parent.left; right: parent.right; top: parent.top }
          spacing: panel.theme.spaceSmall

          Well {
            Text {
              width: parent.width
              text: card.finding.details
              textFormat: Text.PlainText
              wrapMode: Text.Wrap
              color: panel.theme.subtext
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }
          }

          Column {
            width: parent.width
            spacing: panel.theme.spaceTight

            Repeater {
              model: card.metadata

              Item {
                id: metadataRow

                required property var modelData

                width: parent.width
                implicitHeight: metadataValue.implicitHeight

                Text {
                  anchors.left: parent.left
                  anchors.right: metadataValue.left
                  anchors.rightMargin: panel.theme.spaceMedium
                  anchors.baseline: metadataValue.baseline
                  text: metadataRow.modelData.label
                  textFormat: Text.PlainText
                  elide: Text.ElideRight
                  color: panel.theme.overlay
                  font.family: panel.theme.fontFamily
                  font.pixelSize: panel.theme.textCaption
                }

                // The values are a column of their own, so their right edge is
                // pinned rather than left where each row's label happens to end.
                Text {
                  id: metadataValue

                  anchors.right: parent.right
                  text: metadataRow.modelData.value
                  textFormat: Text.PlainText
                  color: panel.theme.subtext
                  font.family: panel.theme.fontFamily
                  font.pixelSize: panel.theme.textCaption
                }
              }
            }
          }

          Shared.SectionLabel {
            theme: panel.theme
            visible: !card.finding.resolved
            text: "SNOOZE FOR"
          }

          Flow {
            width: parent.width
            spacing: panel.theme.spaceSmall
            visible: !card.finding.resolved
            enabled: card.actionable

            Repeater {
              model: [
                { label: "1 hour", seconds: 3600 },
                { label: "1 day", seconds: 86400 },
                { label: "1 week", seconds: 604800 }
              ]

              Shared.ActionButton {
                required property var modelData

                theme: panel.theme
                text: modelData.label
                onClicked: panel.store.request(card.finding, "snooze", { seconds: modelData.seconds })
              }
            }

            TextField {
              id: customMinutes

              // The field takes the same control height as the buttons it sits
              // beside, so the row of snooze controls keeps one line.
              width: panel.theme.controlHeight * 3
              height: panel.theme.controlHeight
              leftPadding: panel.theme.spaceMedium
              rightPadding: panel.theme.spaceMedium
              placeholderText: "Minutes"
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
                antialiasing: true
              }
            }

            Shared.ActionButton {
              theme: panel.theme
              text: "Snooze"
              enabled: customMinutes.acceptableInput
              onClicked: panel.store.request(card.finding, "snooze", { seconds: Number(customMinutes.text) * 60 })
            }
          }
        }
      }

      Flow {
        width: parent.width
        spacing: panel.theme.spaceSmall
        visible: !card.finding.resolved
        enabled: card.actionable

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

      // A repair that has to be agreed to says so where it will run, carrying
      // the two ways out of it rather than leaving them under a loose line.
      Shared.StatusBanner {
        theme: panel.theme
        width: parent.width
        visible: !!panel.store.confirmation && panel.store.confirmation.id === card.finding.id
        tint: panel.theme.yellow
        glyph: "󰀪"
        title: panel.store.confirmation && panel.store.confirmation.proposed
          ? "Review this AI-proposed action before it runs"
          : "This repair may interrupt active work"
        detail: panel.store.confirmation ? "Confirm to run " + panel.store.confirmation.label : ""

        Shared.ActionButton {
          theme: panel.theme
          enabled: card.actionable
          text: "Confirm"
          danger: true
          onClicked: panel.store.confirm()
        }

        Shared.ActionButton {
          theme: panel.theme
          text: "Cancel"
          onClicked: panel.store.confirmation = null
        }
      }

      // Work in flight is acknowledged in place, without the card changing
      // shape around it.
      Row {
        width: parent.width
        visible: card.working
        spacing: panel.theme.spaceSmall

        Shared.RefreshGlyph {
          width: workingLabel.height
          height: width
          theme: panel.theme
          spinning: card.working
          font.pixelSize: panel.theme.textBody
        }

        Text {
          id: workingLabel

          text: "Working…"
          verticalAlignment: Text.AlignVCenter
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
      }

      Text {
        width: parent.width
        visible: !card.working && panel.store.actionErrorId === card.finding.id
        text: "󰀪  " + panel.store.actionError
        textFormat: Text.PlainText
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
        color: panel.theme.overlay
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }

      Column {
        width: parent.width
        spacing: panel.theme.spaceSmall
        visible: !!card.finding.analysis

        Well {
          Shared.SectionLabel {
            theme: panel.theme
            text: card.finding.analysisStale ? "AI ANALYSIS · FINDING CHANGED" : "AI ANALYSIS"
            color: card.finding.analysisStale ? panel.theme.yellow : panel.theme.overlay
          }

          Text {
            width: parent.width
            text: card.finding.analysis ? card.finding.analysis.cause : ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            color: panel.theme.text
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textCaption
          }

          // What the model saw is quoted as evidence; what it suggests doing
          // points forward, so the two are not one undifferentiated list.
          Repeater {
            model: card.finding.analysis ? card.finding.analysis.evidence : []

            Text {
              required property string modelData

              width: parent.width
              text: "·  " + modelData
              textFormat: Text.PlainText
              wrapMode: Text.Wrap
              color: panel.theme.subtext
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }
          }

          Repeater {
            model: card.finding.analysis ? card.finding.analysis.nextSteps : []

            Text {
              required property string modelData

              width: parent.width
              text: "→  " + modelData
              textFormat: Text.PlainText
              wrapMode: Text.Wrap
              color: panel.theme.text
              font.family: panel.theme.fontFamily
              font.pixelSize: panel.theme.textCaption
            }
          }
        }

        Flow {
          width: parent.width
          spacing: panel.theme.spaceSmall
          visible: !card.finding.analysisStale && !card.finding.resolved
          enabled: card.actionable

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
