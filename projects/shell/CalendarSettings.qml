pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// Account and calendar choice. Setup reads as its steps: a client ID, then
// signing in, then the connected account with the calendars it can show.
FocusScope {
  id: settings
  required property var theme
  required property var store
  property bool popupHovered: false
  property bool confirmDisconnect: false

  readonly property var account: store.account
  readonly property string status: store.status
  readonly property bool signingIn: status === "signing-in"
  readonly property bool signedIn: store.signedIn
  readonly property bool syncing: account.syncing
  readonly property string statusText: ({
    "setup": "Not set up",
    "signed-out": "Signed out",
    "signing-in": "Signing in…",
    "connecting": "Connecting…",
    "online": syncing ? "Syncing…" : account.updated ? "Synced " + account.updated : "Connected",
    "offline": account.updated ? "Offline · synced " + account.updated : "Offline",
    "expired": "Sign-in expired",
    "unavailable": "Starting…"
  })[status] || ""
  readonly property color statusColor: status === "online" ? theme.green
    : status === "offline" || status === "expired" ? theme.yellow : theme.overlay

  onVisibleChanged: if (!visible) {
    confirmDisconnect = false
    clientSecret.text = ""
  }
  onConfirmDisconnectChanged: if (confirmDisconnect) disarm.restart()
  Timer { id: disarm; interval: 6000; onTriggered: settings.confirmDisconnect = false }

  component Explanation: Text {
    width: parent ? parent.width : 0
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    lineHeight: 1.15
    color: settings.theme.subtext
    font.family: settings.theme.fontFamily
    font.pixelSize: settings.theme.textCaption
  }

  Column {
    id: settingsColumn
    anchors.fill: parent
    spacing: settings.theme.panelSpacing

    Shared.SectionRule {
      id: accountRule
      theme: settings.theme
      width: parent.width
      label: "GOOGLE ACCOUNT"
      detail: settings.statusText
      detailColor: settings.statusColor
    }

    // Step one: the Desktop OAuth client Seele signs in through.
    Column {
      id: setupStep
      objectName: "calendarSetup"
      width: parent.width
      visible: settings.status === "setup"
      spacing: settings.theme.spaceMedium

      Explanation {
        text: "Seele reads Google Calendar through a Desktop OAuth client of your own. In Google Cloud, enable the Calendar API, create an OAuth client of type Desktop app, and paste its client ID here."
      }
      Explanation {
        text: "Redirect URI: http://127.0.0.1:<port>. Seele chooses the port at sign-in. Desktop app clients do not need a redirect URI entered in Google Cloud. If Cloud asks for one, check the client type."
      }
      TextField {
        id: clientId
        objectName: "calendarClientId"
        width: parent.width
        implicitHeight: settings.theme.controlHeight
        placeholderText: "…apps.googleusercontent.com"
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
        onAccepted: if (text.trim() !== "") settings.store.setup(text.trim())
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
          objectName: "calendarSaveClient"
          theme: settings.theme
          text: "Save client ID"
          selected: true
          enabled: settings.store.ready && clientId.text.trim() !== ""
          onClicked: settings.store.setup(clientId.text.trim())
        }
        Shared.ActionButton {
          theme: settings.theme
          text: "Open Google Cloud"
          onClicked: Qt.openUrlExternally("https://console.cloud.google.com/apis/credentials")
        }
      }
    }

    // Step two: sign in with the saved client and an optional Cloud client secret.
    Column {
      id: signinStep
      objectName: "calendarSignin"
      width: parent.width
      visible: settings.status === "signed-out" || settings.signingIn
      spacing: settings.theme.spaceMedium

      Explanation {
        text: settings.signingIn
          ? "Finish signing in in your browser. This closes by itself after five minutes."
          : "If Google gave your Desktop client a secret, enter it here. An empty field reuses a saved secret. Seele keeps credentials in the system wallet."
      }
      TextField {
        id: clientSecret
        objectName: "calendarClientSecret"
        width: parent.width
        implicitHeight: settings.theme.controlHeight
        placeholderText: "Google OAuth client secret (optional)"
        echoMode: TextInput.Password
        enabled: !settings.signingIn
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
        Accessible.name: "Google OAuth client secret"
        onAccepted: if (settings.store.ready && !settings.signingIn) {
          settings.store.signin(text.trim())
          text = ""
        }
        background: Rectangle {
          color: settings.theme.wellColor
          radius: settings.theme.radius
          border.width: settings.theme.hairline
          border.color: clientSecret.activeFocus ? settings.theme.accent : settings.theme.cardBorder
          Behavior on border.color { ColorAnimation { duration: settings.theme.durationFast } }
        }
      }
      Row {
        spacing: settings.theme.spaceSmall
        Shared.ActionButton {
          objectName: "calendarSignIn"
          theme: settings.theme
          text: settings.signingIn ? "Waiting for Google…" : "Sign in with Google"
          selected: true
          enabled: settings.store.ready && !settings.signingIn
          onClicked: {
            settings.store.signin(clientSecret.text.trim())
            clientSecret.text = ""
          }
        }
        Shared.ActionButton {
          theme: settings.theme
          visible: settings.signingIn
          text: "Cancel"
          onClicked: settings.store.cancelSignin()
        }
        Shared.ActionButton {
          theme: settings.theme
          visible: !settings.signingIn
          text: "Use another client ID"
          onClicked: {
            clientSecret.text = ""
            settings.store.forgetClient()
          }
        }
      }
    }

    // Signed in: who, how fresh, and a way out.
    Rectangle {
      id: accountCard
      objectName: "calendarAccount"
      width: parent.width
      visible: settings.signedIn
      height: accountRow.implicitHeight + settings.theme.cardPadding * 2
      radius: settings.theme.radius
      color: settings.theme.cardColor
      Shared.CardEdge { theme: settings.theme }

      Row {
        id: accountRow
        x: settings.theme.cardPadding
        y: settings.theme.cardPadding
        width: parent.width - settings.theme.cardPadding * 2
        spacing: settings.theme.spaceMedium

        Rectangle {
          anchors.verticalCenter: parent.verticalCenter
          width: settings.theme.chipHeight
          height: width
          radius: settings.theme.radiusSmall
          color: settings.theme.alpha(settings.theme.accent, 0.1)
          border.width: 1
          border.color: settings.theme.alpha(settings.theme.accent, 0.22)
          Shared.CenteredGlyph {
            anchors.fill: parent
            text: "󰊭"
            color: settings.theme.accent
            font.family: settings.theme.fontFamily
            font.pixelSize: settings.theme.textSubhead
          }
        }
        Column {
          anchors.verticalCenter: parent.verticalCenter
          width: parent.width - settings.theme.chipHeight - accountRefresh.width - parent.spacing * 2
          spacing: 1
          Text {
            width: parent.width
            text: settings.account.account || "Google account"
            textFormat: Text.PlainText
            elide: Text.ElideMiddle
            color: settings.theme.text
            font.family: settings.theme.fontFamily
            font.pixelSize: settings.theme.textBody
            font.weight: settings.theme.weightStrong
          }
          Text {
            width: parent.width
            text: "Read-only · refreshes every 5 minutes"
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: settings.theme.subtext
            font.family: settings.theme.fontFamily
            font.pixelSize: settings.theme.textCaption
          }
        }
        Shared.GlyphButton {
          id: accountRefresh
          theme: settings.theme
          anchors.verticalCenter: parent.verticalCenter
          enabled: !settings.syncing
          glyph: settings.syncing ? "" : "󰑐"
          text: settings.syncing ? "Refreshing Google Calendar" : "Refresh Google Calendar"
          onClicked: settings.store.refresh()
          Shared.RefreshGlyph {
            theme: settings.theme
            anchors.centerIn: parent
            width: settings.theme.textIcon
            height: width
            visible: settings.syncing
            spinning: visible
            color: settings.theme.subtext
            font.pixelSize: settings.theme.textBody
          }
        }
      }
    }

    Shared.StatusBanner {
      id: expiredBanner
      width: parent.width
      visible: settings.status === "expired"
      theme: settings.theme
      glyph: "󰥿"
      tint: settings.theme.yellow
      title: "Google sign-in expired"
      detail: "Saved events and reminders continue until you sign in again."
      Shared.ActionButton {
        theme: settings.theme
        text: settings.account.signing_in ? "Waiting for Google…" : "Sign in"
        selected: true
        enabled: !settings.account.signing_in
        onClicked: settings.store.signin()
      }
    }

    Shared.StatusBanner {
      id: errorBanner
      width: parent.width
      visible: settings.account.error !== "" && settings.status !== "expired"
      theme: settings.theme
      tint: settings.theme.yellow
      title: settings.account.error
    }

    Shared.SectionRule {
      id: calendarsRule
      visible: settings.signedIn
      theme: settings.theme
      width: parent.width
      label: "CALENDARS"
      detail: settings.store.calendars.length
        ? settings.store.selectedCount + " of " + settings.store.calendars.length + " shown"
        : ""
    }

    Item {
      id: calendarArea
      width: parent.width
      visible: settings.signedIn
      // The picker takes what the account section and the footer leave.
      height: Math.max(settings.theme.rowHeight * 2,
        settingsColumn.height - y - footer.height - settingsColumn.spacing)

      Shared.SeeleListView {
        id: calendarList
        theme: settings.theme
        anchors.fill: parent
        visible: settings.store.calendars.length > 0
        clip: true
        spacing: settings.theme.spaceTight
        model: settings.store.calendars
        boundsBehavior: contentHeight > height ? Flickable.DragAndOvershootBounds : Flickable.StopAtBounds
        ScrollBar.vertical: Shared.SlimScrollBar { theme: settings.theme; popupHovered: settings.popupHovered }
        delegate: calendarRow
      }

      Shared.EmptyState {
        anchors.centerIn: parent
        width: parent.width
        visible: settings.store.calendars.length === 0
        theme: settings.theme
        title: settings.status === "connecting" || settings.syncing ? "Loading calendars" : "No calendars yet"
        detail: settings.status === "offline" ? "They appear once Google Calendar is reachable." : ""
        Shared.RefreshGlyph {
          theme: settings.theme
          visible: settings.status === "connecting" || settings.syncing
          width: settings.theme.textCard
          height: width
          spinning: visible
          color: settings.theme.overlay
        }
      }
    }

    Item {
      id: footer
      width: parent.width
      visible: settings.signedIn
      height: visible ? settings.theme.controlHeight : 0

      Shared.ActionButton {
        objectName: "calendarDisconnect"
        visible: !settings.confirmDisconnect
        anchors.left: parent.left
        theme: settings.theme
        danger: true
        text: "Disconnect"
        onClicked: settings.confirmDisconnect = true
      }
      Row {
        visible: settings.confirmDisconnect
        anchors.fill: parent
        spacing: settings.theme.spaceSmall
        Shared.ActionButton {
          objectName: "calendarConfirmDisconnect"
          theme: settings.theme
          danger: true
          text: "Disconnect"
          onClicked: { settings.confirmDisconnect = false; settings.store.disconnect() }
        }
        Shared.ActionButton {
          theme: settings.theme
          text: "Keep"
          onClicked: settings.confirmDisconnect = false
        }
        Text {
          anchors.verticalCenter: parent.verticalCenter
          width: parent.width - x
          text: "Removes the token and saved events."
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: settings.theme.subtext
          font.family: settings.theme.fontFamily
          font.pixelSize: settings.theme.textCaption
        }
      }
    }
  }

  Component {
    id: calendarRow

    Rectangle {
      id: row
      required property var modelData
      readonly property bool busy: modelData.loading && settings.syncing
      function toggle() { settings.store.choose(row.modelData.id, !row.modelData.selected) }

      objectName: "calendarChoice_" + modelData.id
      width: ListView.view.width
      height: settings.theme.rowHeight
      radius: settings.theme.radius
      color: settings.theme.rowColor
      activeFocusOnTab: true
      Accessible.role: Accessible.CheckBox
      Accessible.checkable: true
      Accessible.checked: row.modelData.selected
      Accessible.name: row.modelData.name
      Keys.onPressed: event => {
        if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter && event.key !== Qt.Key_Space) return
        row.toggle()
        event.accepted = true
      }

      Shared.HoverWash { theme: settings.theme; hovered: rowMouse.containsMouse }
      Shared.FocusRing { theme: settings.theme; shown: row.activeFocus }
      MouseArea {
        id: rowMouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: { row.forceActiveFocus(); row.toggle() }
      }

      Rectangle {
        id: swatch
        anchors.left: parent.left
        anchors.leftMargin: settings.theme.spaceLarge
        anchors.verticalCenter: parent.verticalCenter
        width: settings.theme.spaceLarge
        height: width
        radius: width / 2
        color: row.modelData.color || settings.theme.accent
        opacity: row.modelData.selected ? 1 : 0.5
      }
      Column {
        anchors.left: swatch.right
        anchors.leftMargin: settings.theme.spaceMedium
        anchors.right: rowSwitch.left
        anchors.rightMargin: settings.theme.spaceMedium
        anchors.verticalCenter: parent.verticalCenter
        spacing: 1
        Text {
          width: parent.width
          text: row.modelData.name
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: row.modelData.selected ? settings.theme.text : settings.theme.subtext
          font.family: settings.theme.fontFamily
          font.pixelSize: settings.theme.textBody
          font.weight: settings.theme.weightMedium
        }
        Text {
          width: parent.width
          visible: text !== ""
          text: row.modelData.loading
            ? (settings.syncing ? "Loading events…" : "Loads when Google Calendar is reachable")
            : row.modelData.role
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: settings.theme.overlay
          font.family: settings.theme.fontFamily
          font.pixelSize: settings.theme.textCaption
        }
      }
      Shared.ControlSwitch {
        id: rowSwitch
        anchors.right: parent.right
        anchors.rightMargin: settings.theme.spaceMedium
        anchors.verticalCenter: parent.verticalCenter
        theme: settings.theme
        checked: row.modelData.selected
        busy: row.busy
        onToggled: row.toggle()
      }
    }
  }
}
