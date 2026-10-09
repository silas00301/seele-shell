pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// The selected day's events under the month. The worker sends the day already
// sorted and labelled; this view only draws it, and says plainly when the day
// is loading, not downloaded, or genuinely empty.
FocusScope {
  id: agenda
  required property var theme
  required property var store
  required property string day
  required property string today
  // The shell's minute clock, in milliseconds, for past and ongoing rows.
  required property double now
  property bool popupHovered: false
  property string expandedKey: ""
  property string pendingReveal: ""
  signal settingsRequested()
  signal copyRequested()

  readonly property var view: store.agenda
  readonly property bool current: view.day === day
  readonly property var items: current ? view.items : []
  readonly property var account: store.account
  readonly property string status: store.status
  readonly property bool syncing: account.syncing
  readonly property string mode: {
    if (status === "setup") return "setup"
    if (status === "signed-out") return "signed-out"
    if (status === "signing-in") return "signing-in"
    if (status === "unavailable" || !current) return "loading"
    if (store.calendars.length > 0 && store.selectedCount === 0) return "no-calendars"
    if (!view.covered) return view.loading || status === "connecting" ? "loading" : "unavailable"
    return items.length ? "events" : "empty"
  }
  readonly property string dayLabel: {
    if (!day) return "Agenda"
    var offset = Math.round((new Date(day + "T12:00:00") - new Date(today + "T12:00:00")) / 86400000)
    if (offset === 0) return "Today"
    if (offset === 1) return "Tomorrow"
    if (offset === -1) return "Yesterday"
    return Qt.formatDate(new Date(day + "T12:00:00"), "ddd d MMM")
  }
  readonly property bool offline: status === "offline" || status === "expired"
  readonly property string detail: {
    if (offline) return account.updated ? "Offline · synced " + account.updated : "Offline"
    if (status === "online" && account.stale && account.updated) return "Synced " + account.updated
    if (mode === "events") return items.length === 1 ? "1 event" : items.length + " events"
    return ""
  }

  // Unfolds an event once its day has arrived from the worker.
  function reveal(key) {
    pendingReveal = key
    expandedKey = key
    Qt.callLater(agenda.settleReveal)
  }
  function settleReveal() {
    if (!pendingReveal) return
    for (var i = 0; i < items.length; i++) {
      if (items[i].key !== pendingReveal) continue
      eventList.positionViewAtIndex(i, ListView.Contain)
      pendingReveal = ""
      return
    }
  }
  onItemsChanged: if (pendingReveal) Qt.callLater(agenda.settleReveal)

  // A cached day arrives from the worker within a frame; only a fetch that
  // actually takes time says it is loading, so moving between days never flashes.
  property bool loadingShown: false
  onModeChanged: {
    loadingShown = false
    if (mode === "loading") loadingDelay.restart()
  }
  Timer { id: loadingDelay; interval: 180; onTriggered: agenda.loadingShown = agenda.mode === "loading" }

  Column {
    id: agendaColumn
    anchors.fill: parent
    spacing: agenda.theme.spaceSmall

    Shared.SectionRule {
      id: agendaRule
      theme: agenda.theme
      width: parent.width
      label: agenda.dayLabel
      detail: agenda.detail
      textBottomInset: agenda.theme.spaceTight
      detailTrailingSpacing: agenda.theme.spaceLarge
      trailingSpacing: agenda.theme.spaceTight
      detailColor: agenda.offline || (agenda.status === "online" && agenda.account.stale) ? agenda.theme.yellow : agenda.theme.overlay

      Shared.GlyphButton {
        theme: agenda.theme
        implicitWidth: agenda.theme.chipHeight - 4
        implicitHeight: agenda.theme.chipHeight - 4
        visible: agenda.day !== ""
        glyph: "󰆏"
        text: "Copy " + agenda.day
        onClicked: agenda.copyRequested()
      }
      Shared.GlyphButton {
        id: refreshButton
        theme: agenda.theme
        implicitWidth: agenda.theme.chipHeight - 4
        implicitHeight: agenda.theme.chipHeight - 4
        visible: agenda.store.signedIn
        enabled: !agenda.syncing
        glyph: agenda.syncing ? "" : "󰑐"
        text: agenda.syncing ? "Refreshing Google Calendar" : "Refresh Google Calendar"
        onClicked: agenda.store.refresh()
        Shared.RefreshGlyph {
          theme: agenda.theme
          anchors.centerIn: parent
          width: agenda.theme.textIcon
          height: width
          visible: agenda.syncing
          spinning: visible
          color: agenda.theme.subtext
          font.pixelSize: agenda.theme.textBody
        }
      }
    }

    Shared.StatusBanner {
      id: expiredBanner
      width: parent.width
      visible: agenda.status === "expired"
      theme: agenda.theme
      glyph: "󰥿"
      tint: agenda.theme.yellow
      title: "Google sign-in expired"
      detail: "Showing saved events. Reminders continue."
      Shared.ActionButton {
        theme: agenda.theme
        text: agenda.account.signing_in ? "Waiting for Google…" : "Sign in"
        selected: true
        enabled: !agenda.account.signing_in
        onClicked: agenda.store.signin()
      }
    }

    Item {
      width: parent.width
      height: agendaColumn.height - agendaRule.height - agendaColumn.spacing
        - (expiredBanner.visible ? expiredBanner.height + agendaColumn.spacing : 0)

      Shared.SeeleListView {
        id: eventList
        objectName: "calendarEvents"
        theme: agenda.theme
        anchors.fill: parent
        visible: agenda.mode === "events"
        clip: true
        spacing: agenda.theme.spaceSmall
        model: agenda.mode === "events" ? agenda.items : []
        boundsBehavior: contentHeight > height ? Flickable.DragAndOvershootBounds : Flickable.StopAtBounds
        ScrollBar.vertical: Shared.SlimScrollBar { theme: agenda.theme; popupHovered: agenda.popupHovered }
        delegate: eventRow
      }

      Shared.EmptyState {
        objectName: "calendarEmpty"
        anchors.centerIn: parent
        width: parent.width
        visible: agenda.mode !== "events" && (agenda.mode !== "loading" || agenda.loadingShown)
        theme: agenda.theme
        glyph: ({ "setup": "󰃭", "signed-out": "󰊭", "signing-in": "󰊭", "no-calendars": "󰃮", "unavailable": "󰅤", "empty": "󰃯" })[agenda.mode] || ""
        tint: agenda.mode === "unavailable" ? agenda.theme.yellow : agenda.theme.overlay
        title: ({
          "setup": "Connect Google Calendar",
          "signed-out": "Sign in to Google Calendar",
          "signing-in": "Waiting for Google…",
          "no-calendars": "No calendars selected",
          "loading": "Loading events",
          "unavailable": "Not available offline",
          "empty": "Nothing scheduled"
        })[agenda.mode] || ""
        detail: ({
          "setup": "Your agenda, day dots and reminders appear here.",
          "signed-out": "Seele only reads the calendars you choose.",
          "signing-in": "Finish signing in in your browser.",
          "unavailable": "Events for this date have not been downloaded yet."
        })[agenda.mode] || ""

        Shared.RefreshGlyph {
          theme: agenda.theme
          visible: agenda.mode === "loading"
          width: agenda.theme.textCard
          height: width
          spinning: visible
          color: agenda.theme.overlay
        }
        Shared.ActionButton {
          theme: agenda.theme
          visible: agenda.mode === "setup" || agenda.mode === "no-calendars"
          selected: true
          text: agenda.mode === "setup" ? "Set up" : "Choose calendars"
          onClicked: agenda.settingsRequested()
        }
        Shared.ActionButton {
          theme: agenda.theme
          visible: agenda.mode === "signed-out"
          selected: true
          text: "Sign in with Google"
          onClicked: agenda.store.signin()
        }
        Shared.ActionButton {
          theme: agenda.theme
          visible: agenda.mode === "signing-in"
          text: "Cancel"
          onClicked: agenda.store.cancelSignin()
        }
        Shared.ActionButton {
          theme: agenda.theme
          visible: agenda.mode === "unavailable"
          text: "Retry"
          onClicked: agenda.store.refresh()
        }
      }
    }
  }

  Component {
    id: eventRow

    Rectangle {
      id: row
      required property var modelData
      required property int index
      readonly property bool expanded: agenda.expandedKey === modelData.key
      readonly property bool timed: !modelData.all_day
      readonly property bool past: timed && modelData.end <= agenda.now
      readonly property bool ongoing: timed && modelData.start <= agenda.now && modelData.end > agenda.now
      // The same fifteen minutes the bar counts down, when Join becomes the obvious next step.
      readonly property bool imminent: timed && modelData.start - agenda.now <= 900000 && modelData.end > agenda.now
      readonly property bool answer: modelData.rsvp === "tentative" || modelData.rsvp === "needsAction"
      readonly property color tint: modelData.color || agenda.theme.accent
      function toggle() { agenda.expandedKey = row.expanded ? "" : row.modelData.key }

      objectName: "calendarEvent_" + modelData.key
      width: ListView.view.width
      height: rowBody.implicitHeight + agenda.theme.spaceMedium * 2
      radius: agenda.theme.radiusRow
      color: row.expanded ? agenda.theme.cardColor : agenda.theme.rowColor
      activeFocusOnTab: true
      Accessible.role: Accessible.Button
      Accessible.name: row.modelData.title + ", " + row.modelData.time
      Keys.onPressed: event => {
        if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && event.key !== Qt.Key_Space) return
        row.toggle()
        event.accepted = true
      }
      Behavior on color { ColorAnimation { duration: agenda.theme.durationFast } }

      Shared.HoverWash { theme: agenda.theme; hovered: rowMouse.containsMouse }
      Shared.FocusRing { theme: agenda.theme; shown: row.activeFocus }

      MouseArea {
        id: rowMouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: { row.forceActiveFocus(); row.toggle() }
      }

      // The event's own colour, as Google has it; the text beside it stays on theme tokens.
      Rectangle {
        x: agenda.theme.spaceSmall
        y: agenda.theme.spaceMedium
        width: 3
        height: parent.height - agenda.theme.spaceMedium * 2
        radius: width / 2
        color: row.tint
        opacity: row.past ? 0.45 : 1
      }

      Column {
        id: rowBody
        x: agenda.theme.spaceLarge + agenda.theme.spaceTight
        y: agenda.theme.spaceMedium
        width: parent.width - x - agenda.theme.spaceMedium
        spacing: 3

        // A fixed line height, so a row with a Join button is as tall as one without.
        Item {
          width: parent.width
          height: agenda.theme.chipHeight - 6

          Text {
            id: rowTitle
            anchors.left: parent.left
            anchors.right: rowTrailing.left
            anchors.rightMargin: rowTrailing.width > 0 ? agenda.theme.spaceSmall : 0
            anchors.verticalCenter: parent.verticalCenter
            text: row.modelData.title
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: agenda.theme.text
            opacity: row.past ? 0.55 : 1
            font.family: agenda.theme.fontFamily
            font.pixelSize: agenda.theme.textBody
            font.weight: agenda.theme.weightStrong
          }

          Row {
            id: rowTrailing
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: agenda.theme.spaceSmall

            Text {
              visible: row.answer && !row.expanded
              anchors.verticalCenter: parent.verticalCenter
              text: row.modelData.rsvp_label
              textFormat: Text.PlainText
              color: agenda.theme.yellow
              font.family: agenda.theme.fontFamily
              font.pixelSize: agenda.theme.textCaption
              font.weight: agenda.theme.weightMedium
            }
            // Joining is the one thing an imminent meeting is opened for.
            Shared.ActionButton {
              objectName: "calendarQuickJoin"
              theme: agenda.theme
              visible: row.modelData.join !== "" && !row.past && !row.expanded
              implicitHeight: agenda.theme.chipHeight - 6
              selected: row.imminent
              text: "Join"
              Accessible.name: "Join " + (row.modelData.join_label || "meeting")
              onClicked: Qt.openUrlExternally(row.modelData.join)
            }
            Text {
              anchors.verticalCenter: parent.verticalCenter
              text: row.expanded ? "󰅃" : "󰅀"
              color: rowMouse.containsMouse || row.activeFocus ? agenda.theme.text : agenda.theme.overlay
              font.family: agenda.theme.fontFamily
              font.pixelSize: agenda.theme.textBody
            }
          }
        }

        Text {
          width: parent.width
          text: (row.ongoing ? "Now · " : "") + row.modelData.time
            + (!row.expanded && row.modelData.location ? " · " + row.modelData.location : "")
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: row.ongoing ? agenda.theme.accent : agenda.theme.subtext
          opacity: row.past ? 0.55 : 1
          font.family: agenda.theme.fontFamily
          font.pixelSize: agenda.theme.textCaption
          font.weight: row.ongoing ? agenda.theme.weightMedium : agenda.theme.weightRegular
        }

        Loader {
          width: parent.width
          active: row.expanded
          visible: active
          sourceComponent: Column {
            width: parent ? parent.width : 0
            spacing: agenda.theme.spaceSmall
            topPadding: agenda.theme.spaceSmall

            Repeater {
              model: [
                { glyph: "󰅐", text: row.modelData.when },
                { glyph: "󰟙", text: row.modelData.location },
                { glyph: "", text: row.modelData.calendar, swatch: true },
                { glyph: "󰀈", text: row.modelData.rsvp_label ? "Your response: " + row.modelData.rsvp_label : "" }
              ].filter(line => line.text)
              Row {
                id: detailLine
                required property var modelData
                width: parent.width
                spacing: agenda.theme.spaceSmall
                Item {
                  width: agenda.theme.textIcon
                  height: detailText.lineCount > 1 ? detailText.font.pixelSize * 1.4 : detailText.height
                  Text {
                    visible: !detailLine.modelData.swatch
                    anchors.centerIn: parent
                    text: detailLine.modelData.glyph
                    color: agenda.theme.overlay
                    font.family: agenda.theme.fontFamily
                    font.pixelSize: agenda.theme.textBody
                  }
                  Rectangle {
                    visible: !!detailLine.modelData.swatch
                    anchors.centerIn: parent
                    width: agenda.theme.spaceMedium
                    height: width
                    radius: width / 2
                    color: row.tint
                  }
                }
                Text {
                  id: detailText
                  width: parent.width - agenda.theme.textIcon - parent.spacing
                  text: detailLine.modelData.text
                  textFormat: Text.PlainText
                  wrapMode: Text.Wrap
                  maximumLineCount: 3
                  elide: Text.ElideRight
                  color: agenda.theme.text
                  font.family: agenda.theme.fontFamily
                  font.pixelSize: agenda.theme.textCaption
                }
              }
            }

            Text {
              width: parent.width
              visible: text !== ""
              text: row.modelData.description
              textFormat: Text.PlainText
              wrapMode: Text.Wrap
              maximumLineCount: 8
              elide: Text.ElideRight
              lineHeight: 1.15
              color: agenda.theme.subtext
              font.family: agenda.theme.fontFamily
              font.pixelSize: agenda.theme.textCaption
            }

            Flow {
              width: parent.width
              spacing: agenda.theme.spaceSmall
              visible: row.modelData.join !== "" || row.modelData.link !== ""
              Shared.ActionButton {
                objectName: "calendarJoin"
                theme: agenda.theme
                visible: row.modelData.join !== ""
                selected: row.imminent
                text: "Join " + (row.modelData.join_label || "meeting")
                onClicked: Qt.openUrlExternally(row.modelData.join)
              }
              Shared.ActionButton {
                objectName: "calendarOpen"
                theme: agenda.theme
                visible: row.modelData.link !== ""
                text: "Open in Google Calendar"
                onClicked: Qt.openUrlExternally(row.modelData.link)
              }
            }
          }
        }
      }
    }
  }
}
