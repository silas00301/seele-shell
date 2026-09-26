import QtQuick
import QtQuick.Controls
import "../shared" as Shared

Column {
  id: settings
  required property var theme
  required property var store
  property bool confirmDisconnect: false
  spacing: theme.panelSpacing
  Shared.SectionRule { theme: settings.theme; width: parent.width; label: "GOOGLE ACCOUNT" }
  Text {
    width: parent.width
    text: settings.store.configured ? "Read-only access to the calendars you select." : "Create a Desktop OAuth client in Google Cloud, then paste its client ID here."
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    color: settings.theme.subtext
    font.family: settings.theme.fontFamily
    font.pixelSize: settings.theme.textCaption
  }
  TextField {
    id: clientId
    width: parent.width
    visible: !settings.store.configured
    implicitHeight: settings.theme.controlHeight
    placeholderText: "Desktop OAuth client ID"
    color: settings.theme.text
    placeholderTextColor: settings.theme.overlay
    selectionColor: settings.theme.selectedColor
    selectedTextColor: settings.theme.text
    font.family: settings.theme.fontFamily
    font.pixelSize: settings.theme.textBody
    leftPadding: settings.theme.cardPadding
    rightPadding: settings.theme.cardPadding
    selectByMouse: true
    verticalAlignment: TextInput.AlignVCenter
    Accessible.name: "Google Desktop OAuth client ID"
    onAccepted: settings.store.setup(text.trim())
    background: Rectangle {
      color: settings.theme.wellColor
      radius: settings.theme.radius
      border.width: settings.theme.hairline
      border.color: clientId.activeFocus ? settings.theme.accent : settings.theme.cardBorder
      Behavior on border.color { ColorAnimation { duration: settings.theme.durationFast } }
    }
  }
  Row {
    spacing: settings.theme.spaceSmall
    Shared.ActionButton {
      theme: settings.theme
      text: settings.store.error.indexOf("Waiting") === 0 ? "Waiting for Google…" : settings.store.configured ? "Sign in with Google" : "Save client ID"
      selected: true
      enabled: settings.store.ready && settings.store.error.indexOf("Waiting") !== 0 && (settings.store.configured || clientId.text.trim() !== "")
      onClicked: if (settings.store.configured) settings.store.signin(); else settings.store.setup(clientId.text.trim())
    }
    Shared.ActionButton { theme: settings.theme; text: "Refresh"; visible: settings.store.configured; enabled: settings.store.connected; onClicked: settings.store.send("refresh") }
  }
  Shared.StatusBanner {
    visible: settings.store.error !== ""
    width: parent.width
    theme: settings.theme
    title: settings.store.error
    tint: settings.store.error.indexOf("Waiting") === 0 ? settings.theme.accent : settings.theme.yellow
  }
  Shared.SectionRule {
    visible: settings.store.calendars.length > 0
    theme: settings.theme
    width: parent.width
    label: "CALENDARS"
    detail: settings.store.selected.length + " selected"
  }
  Flickable {
    width: parent.width
    height: visible ? Math.min(settings.theme.rowHeight * 4, calendarColumn.implicitHeight) : 0
    visible: settings.store.calendars.length > 0
    clip: true
    contentHeight: calendarColumn.implicitHeight
    Column {
      id: calendarColumn
      width: parent.width
      spacing: settings.theme.spaceTight
      Repeater {
        model: settings.store.calendars
        Rectangle {
          id: calendarRow
          required property var modelData
          width: calendarColumn.width
          height: settings.theme.rowHeight
          radius: settings.theme.radius
          color: settings.theme.rowColor
          Shared.CardEdge { theme: settings.theme }
          Row {
            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; margins: settings.theme.spaceMedium }
            spacing: settings.theme.spaceMedium
            Rectangle { width: settings.theme.spaceLarge; height: width; radius: width / 2; color: calendarRow.modelData.backgroundColor || settings.theme.accent }
            Text {
              width: calendarRow.width - settings.theme.controlHeight - settings.theme.spaceLarge * 4
              text: (calendarRow.modelData.summary || calendarRow.modelData.id) + (calendarRow.modelData.primary ? " · Primary" : "")
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: settings.theme.text
              font.family: settings.theme.fontFamily
              font.pixelSize: settings.theme.textBody
            }
            Shared.ControlSwitch {
              theme: settings.theme
              checked: settings.store.selected.indexOf(calendarRow.modelData.id) >= 0
              enabled: settings.store.connected && !settings.store.selecting
              onToggled: settings.store.choose(calendarRow.modelData.id)
            }
          }
        }
      }
    }
  }
  Shared.ActionButton {
    theme: settings.theme
    visible: settings.store.configured
    text: settings.confirmDisconnect ? "Confirm disconnect" : "Disconnect and clear cache"
    danger: true
    onClicked: {
      if (settings.confirmDisconnect) { settings.store.send("disconnect"); settings.confirmDisconnect = false }
      else settings.confirmDisconnect = true
    }
  }
}
