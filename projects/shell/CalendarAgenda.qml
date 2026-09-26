import QtQuick
import "../shared" as Shared

Column {
  id: agenda
  required property var theme
  required property var store
  property string day: ""
  property string expandedId: ""
  spacing: theme.spaceSmall

  Shared.SectionRule {
    theme: agenda.theme
    width: parent.width
    label: "AGENDA"
    detail: agenda.day + (agenda.store.stale ? " · Offline / stale" : "")
    detailColor: agenda.store.stale ? agenda.theme.yellow : agenda.theme.subtext
  }
  Shared.StatusBanner {
    visible: !agenda.store.hasDay(agenda.day)
    width: parent.width
    theme: agenda.theme
    title: !agenda.store.configured ? "Connect Google Calendar" : agenda.store.connected ? "Loading this date" : "This date is not cached"
    detail: !agenda.store.configured ? "Open Calendar settings to sign in." : agenda.store.connected ? "Fetching events from Google Calendar." : "Reconnect to load events for this date."
    tint: agenda.theme.yellow
    Shared.ActionButton { theme: agenda.theme; text: "Retry"; visible: agenda.store.configured && !agenda.store.connected; onClicked: agenda.store.send("day", {date: agenda.day}) }
  }
  Shared.EmptyState {
    visible: agenda.store.hasDay(agenda.day) && agenda.store.agenda(agenda.day).length === 0
    width: parent.width
    height: visible ? implicitHeight : 0
    theme: agenda.theme
    title: "No events"
  }
  Flickable {
    width: parent.width
    height: visible ? agenda.theme.rowHeight * 4 + agenda.theme.spaceMedium : 0
    visible: agenda.store.hasDay(agenda.day)
    contentHeight: eventColumn.implicitHeight
    clip: true
    Column {
      id: eventColumn
      width: parent.width
      spacing: agenda.theme.spaceSmall
      Repeater {
        model: agenda.store.agenda(agenda.day)
        Rectangle {
          id: row
          required property var modelData
          readonly property bool expanded: agenda.expandedId === String(modelData.calendar_id) + ":" + String(modelData.id)
          width: eventColumn.width
          height: content.implicitHeight + agenda.theme.cardPadding * 2
          radius: agenda.theme.radius
          color: agenda.theme.cardColor
          Shared.CardEdge { theme: agenda.theme }
          HoverHandler { id: rowHover }
          Shared.HoverWash { theme: agenda.theme; hovered: rowHover.hovered }
          Rectangle { width: agenda.theme.spaceTight; height: parent.height - agenda.theme.spaceMedium; x: agenda.theme.spaceTight; y: agenda.theme.spaceTight; radius: width / 2; color: agenda.store.eventColor(row.modelData) || agenda.theme.accent }
          Column {
            id: content
            x: agenda.theme.cardPadding + agenda.theme.spaceTight; y: agenda.theme.cardPadding
            width: parent.width - agenda.theme.cardPadding * 2 - agenda.theme.spaceTight
            spacing: agenda.theme.spaceTight
            Text {
              width: parent.width
              text: row.modelData.summary || "(Untitled event)"
              elide: Text.ElideRight
              color: agenda.theme.text
              font.family: agenda.theme.fontFamily
              font.pixelSize: agenda.theme.textLabel
              font.weight: agenda.theme.weightStrong
            }
            Text {
              width: parent.width
              text: row.modelData.start && row.modelData.start.date ? agenda.store.allDayLabel(row.modelData) : Qt.formatTime(new Date(row.modelData.start.dateTime), "HH:mm") + "–" + Qt.formatTime(new Date(row.modelData.end.dateTime), "HH:mm")
              color: agenda.theme.subtext
              font.family: agenda.theme.fontFamily
              font.pixelSize: agenda.theme.textCaption
            }
            Column {
              width: parent.width
              spacing: 4
              visible: row.expanded
              Text {
                width: parent.width
                text: agenda.store.rsvp(row.modelData) ? "RSVP: " + agenda.store.rsvp(row.modelData) : ""
                visible: text !== ""
                color: agenda.theme.subtext
                font.family: agenda.theme.fontFamily
                font.pixelSize: agenda.theme.textCaption
              }
              Text { width: parent.width; text: row.modelData.location || ""; visible: text !== ""; wrapMode: Text.Wrap; color: agenda.theme.text; font.family: agenda.theme.fontFamily; font.pixelSize: agenda.theme.textCaption }
              Text { width: parent.width; text: agenda.store.description(row.modelData); textFormat: Text.PlainText; visible: text !== ""; wrapMode: Text.Wrap; color: agenda.theme.subtext; font.family: agenda.theme.fontFamily; font.pixelSize: agenda.theme.textCaption }
              Row {
                spacing: agenda.theme.spaceSmall
                Shared.ActionButton { theme: agenda.theme; text: "Join"; visible: agenda.store.meeting(row.modelData) !== ""; onClicked: Qt.openUrlExternally(agenda.store.meeting(row.modelData)) }
                Shared.ActionButton { theme: agenda.theme; text: "Open in Google Calendar"; visible: agenda.store.safeLink(row.modelData.htmlLink) !== ""; onClicked: Qt.openUrlExternally(agenda.store.safeLink(row.modelData.htmlLink)) }
              }
            }
          }
          MouseArea {
            anchors { left: parent.left; right: parent.right; top: parent.top }
            height: agenda.theme.rowHeight
            cursorShape: Qt.PointingHandCursor
            onClicked: agenda.expandedId = row.expanded ? "" : String(row.modelData.calendar_id) + ":" + String(row.modelData.id)
          }
        }
      }
    }
  }
}
