//@ pragma UseQApplication

import QtQuick
import "../shared/Palette.js" as Palette
import "../shared/Motion.js" as Motion
import "../shared/Shapes.js" as Shapes
import "../shared" as Shared
import Quickshell
import Quickshell.Io
import Quickshell.Services.Pam
import Quickshell.Wayland

ShellRoot {
  id: root

  property color base: Palette.fallback.base
  property color mantle: Palette.fallback.mantle
  // The palette's darkest step, which the shared role derivation reads for
  // the lowest surface container and for ink on a pale fill.
  property color crust: Palette.fallback.crust
  property color surface: Palette.fallback.surface
  property color overlay: Palette.fallback.overlay
  property color text: Palette.fallback.text
  property color subtext: Palette.fallback.subtext
  property color accent: Palette.fallback.accent
  property color red: Palette.fallback.red
  property color green: Palette.fallback.green
  property color yellow: Palette.fallback.yellow
  property string fontFamily: Palette.fallback.fontFamily
  property string wallpaper: Palette.fallback.wallpaper

  // Keep authentication chrome on the same tokens as Seele Shell. The lock
  // stays a separate client, but it should not grow a second design system:
  // the shape scale, the type ramp and the Material colour roles are the
  // shell's own, and the roles come from the same derivation in Palette.js.
  readonly property int radiusPanel: 28
  readonly property int shapeSmall: 8
  readonly property int shapeLarge: 16
  readonly property int panelMargin: 16
  readonly property int panelSpacing: 10
  readonly property int panelHeaderHeight: 32
  readonly property int panelMarkSize: 32
  readonly property int spaceTight: 4
  readonly property int hairline: 1
  readonly property int focusWidth: 2
  // The same type ramp and weights Seele Shell uses, so the lock is the desktop
  // at rest rather than a surface that merely resembles it.
  readonly property int textLabel: 10
  readonly property int textBody: 11
  readonly property int textLead: 13
  readonly property int textSubhead: 15
  readonly property int textCard: 17
  readonly property int textTitle: 18
  readonly property int textDisplay: 20
  readonly property int textMark: 30
  readonly property int textClock: 72
  readonly property int weightStrong: Font.DemiBold
  readonly property int weightLight: Font.Light
  readonly property int durationFast: Motion.springDuration(Motion.fastEffectsStiffness, Motion.fastEffectsDamping)
  readonly property int durationFastSpatial: Motion.springDuration(Motion.fastSpatialStiffness, Motion.fastSpatialDamping)
  readonly property var springFastSpatial: Motion.springCurve(Motion.fastSpatialStiffness, Motion.fastSpatialDamping)
  readonly property var roles: Palette.roles({
    base: base, mantle: mantle, crust: crust, surface: surface, overlay: overlay,
    text: text, subtext: subtext, accent: accent, red: red, green: green, yellow: yellow
  })
  readonly property color panelColor: roles.surfaceContainer
  readonly property color cardColor: roles.surfaceContainerHigh
  readonly property color panelBorder: alpha(roles.outlineVariant, 0.9)
  readonly property color hoverColor: alpha(text, 0.08)
  readonly property color pressColor: alpha(text, 0.1)

  readonly property string userName: Quickshell.env("USER") || Quickshell.env("LOGNAME") || "user"
  readonly property string displayName: Quickshell.env("SEELE_LOCK_NAME") || titleCase(userName)
  readonly property bool secure: sessionLock.secure
  readonly property bool authenticating: pam.active

  property date now: new Date()
  property string passwordText: ""
  property string pendingPassword: ""
  property string authMessage: ""
  property bool authFailed: false
  property bool yubikeyTouchRequired: false
  property int failedAttempts: 0
  property bool powerMenuOpen: false
  property string pendingPowerAction: ""
  property var powerCommand: []

  signal focusPassword()

  function alpha(color, opacity) {
    return Qt.rgba(color.r, color.g, color.b, opacity)
  }

  function titleCase(value) {
    var text = String(value || "")
    return text.length > 0 ? text.charAt(0).toUpperCase() + text.slice(1) : "User"
  }

  function submitPassword(password) {
    if (!secure || authenticating || password.length === 0) return

    pendingPassword = password
    authMessage = "Checking password"
    authFailed = false
    yubikeyTouchRequired = false
    passwordText = ""

    if (!pam.start()) {
      authenticationFailed()
      return
    }

    Qt.callLater(respondToPrompt)
  }

  function respondToPrompt() {
    if (!pam.active || !pam.responseRequired || pendingPassword.length === 0) return
    pam.respond(pendingPassword)
    pendingPassword = ""
  }

  function authenticationFailed() {
    if (!sessionLock.locked) return
    pendingPassword = ""
    failedAttempts += 1
    authFailed = true
    yubikeyTouchRequired = false
    authMessage = failedAttempts === 1
      ? "Authentication failed"
      : "Authentication failed · " + failedAttempts + " attempts"
    passwordText = ""
    focusDelay.restart()
  }

  function requestPower(action) {
    if (powerProcess.running) return

    if (action === "lock") {
      powerMenuOpen = false
      pendingPowerAction = ""
      focusPassword()
      return
    }

    if (action === "suspend" || action === "logout") {
      powerMenuOpen = false
      powerCommand = action === "suspend"
        ? ["systemctl", "suspend"]
        : ["hyprctl", "dispatch", "exit"]
      powerProcess.running = true
      return
    }

    if (pendingPowerAction !== action) {
      pendingPowerAction = action
      powerConfirm.restart()
      return
    }

    powerConfirm.stop()
    powerMenuOpen = false
    pendingPowerAction = ""
    powerCommand = action === "reboot-windows"
      ? ["systemctl", "--no-block", "start", "reboot-windows.service"]
      : ["systemctl", action === "reboot" ? "reboot" : "poweroff"]
    powerProcess.running = true
  }

  // One action in the Power grid, drawn as the shell draws its own: a tile
  // on the card step that pinches to the small corner while it is held. A
  // destructive action sits in the error container, and an action waiting
  // for its second press fills with the colour that will act.
  component PowerButton: Rectangle {
    id: powerButton

    required property string action
    required property string icon
    required property string label
    required property string variant
    readonly property bool destructive: variant === "destructive"
    readonly property bool confirming: root.pendingPowerAction === action
    readonly property color content: powerButton.confirming
      ? (powerButton.destructive ? root.roles.textOnError : root.roles.textOnPrimary)
      : powerButton.destructive ? root.roles.textOnErrorContainer : root.text

    width: (parent.width - 16) / 3
    height: 72
    radius: powerMouse.pressed ? root.shapeSmall : root.shapeLarge
    color: powerButton.confirming
      ? (powerButton.destructive ? root.roles.error : root.roles.primary)
      : powerButton.destructive ? root.roles.errorContainer : root.cardColor
    antialiasing: true

    Behavior on color { ColorAnimation { duration: root.durationFast } }
    Behavior on radius { NumberAnimation { duration: root.durationFastSpatial; easing.type: Easing.BezierSpline; easing.bezierCurve: root.springFastSpatial } }

    Rectangle {
      anchors.fill: parent
      radius: parent.radius
      color: powerMouse.pressed ? root.alpha(powerButton.content, 0.1)
        : powerMouse.containsMouse ? root.alpha(powerButton.content, 0.08)
        : root.alpha(powerButton.content, 0)
      antialiasing: true
      Behavior on color { ColorAnimation { duration: root.durationFast } }
    }

    Column {
      anchors.centerIn: parent
      spacing: root.spaceTight

      Text {
        anchors.horizontalCenter: parent.horizontalCenter
        text: powerButton.icon
        color: powerButton.confirming || powerButton.destructive
          ? powerButton.content
          : powerButton.action === "reboot-windows" && powerButton.confirming
            ? root.yellow
            : root.roles.primary
        font.family: root.fontFamily
        font.pixelSize: root.textTitle
      }

      Text {
        anchors.horizontalCenter: parent.horizontalCenter
        text: powerButton.confirming ? "Confirm" : powerButton.label
        color: powerButton.content
        font.family: root.fontFamily
        font.pixelSize: root.textLabel
        font.weight: root.weightStrong
      }
    }

    MouseArea {
      id: powerMouse
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: root.requestPower(powerButton.action)
    }
  }

  // A mark in one of Material 3 Expressive's shapes, as the shell's panel
  // headers draw theirs.
  component ShapeMark: Canvas {
    id: shapeMark

    property string shape: "cookie9"
    property color color: root.roles.primaryContainer

    antialiasing: true
    onColorChanged: requestPaint()
    onWidthChanged: requestPaint()
    onPaint: {
      var context = getContext("2d")
      context.reset()
      Shapes.fill(context, Shapes.radii(shapeMark.shape), width, height, 0, shapeMark.color)
    }
  }

  FileView {
    path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/seele-shell/theme.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: {
      try {
        var theme = JSON.parse(text())
        Palette.assign(root, theme, true)
      } catch (error) {
        console.warn("seele-lock/theme", error)
      }
    }
  }

  // Seele Shell listens to the same YubiKey detector. Tell it that the lock
  // owns the PAM conversation so its touch OSD never waits behind this surface
  // and flashes during unlock.
  FileView {
    id: sessionStateFile
    path: Quickshell.env("XDG_RUNTIME_DIR") + "/seele-lock.state"
    printErrors: false
  }

  Component.onCompleted: sessionStateFile.setText("1")

  Timer {
    interval: 1000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: root.now = new Date()
  }

  Timer {
    id: focusDelay
    interval: 180
    onTriggered: root.focusPassword()
  }

  Timer {
    id: powerConfirm
    interval: 5000
    onTriggered: root.pendingPowerAction = ""
  }

  Process {
    id: powerProcess
    command: root.powerCommand
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        if (String(text).trim() !== "") {
          root.authFailed = true
          root.authMessage = "Power action failed"
        }
      }
    }
  }

  PamContext {
    id: pam
    config: "seele-lock"
    user: root.userName

    onResponseRequiredChanged: root.respondToPrompt()
    onPamMessage: {
      if (responseRequired) {
        root.respondToPrompt()
      } else if (message.length > 0) {
        root.authMessage = message
        root.authFailed = messageIsError
        root.yubikeyTouchRequired = !messageIsError && /yubikey|finger|security key/i.test(message)
      }
    }

    onCompleted: function(result) {
      root.pendingPassword = ""
      root.yubikeyTouchRequired = false
      if (result === PamResult.Success) {
        root.authMessage = "Unlocked"
        root.authFailed = false
        sessionLock.locked = false
      } else {
        root.authenticationFailed()
      }
    }
  }

  IpcHandler {
    target: "seele-lock"
    function status(): string {
      if (sessionLock.secure) return "secure"
      return sessionLock.locked ? "securing" : "unlocked"
    }
  }

  // A lock the compositor refuses -- because another client already holds
  // ext-session-lock-v1 -- never changes `locked`, so its unlock handler never
  // runs and this instance would sit here forever without a surface. That
  // leftover is what actually breaks locking: `quickshell -n` exits
  // immediately when an instance for the same config path is already running,
  // so every later attempt becomes a silent no-op against the idle leftover,
  // and locking stays dead until a rebuild changes the path. Give up instead.
  Timer {
    running: !sessionLock.secure
    interval: 5000
    onTriggered: if (!sessionLock.secure) {
      sessionStateFile.setText("0")
      Qt.quit()
    }
  }

  WlSessionLock {
    id: sessionLock
    locked: true

    onSecureStateChanged: {
      if (secure) {
        root.authMessage = ""
        root.focusPassword()
      }
    }

    onLockStateChanged: {
      if (!locked) {
        sessionStateFile.setText("0")
        Qt.callLater(Qt.quit)
      }
    }

    WlSessionLockSurface {
      id: lockSurface
      color: root.base

      Rectangle {
        anchors.fill: parent
        color: root.base

        Image {
          id: wallpaperImage
          anchors.fill: parent
          source: "file://" + root.wallpaper
          fillMode: Image.PreserveAspectCrop
          asynchronous: true
          cache: true
          sourceSize.width: width
          sourceSize.height: height
        }

        Rectangle {
          anchors.fill: parent
          color: root.alpha(root.mantle, 0.2)
        }

        MouseArea {
          anchors.fill: parent
          onClicked: {
            if (root.powerMenuOpen) root.powerMenuOpen = false
            else root.focusPassword()
          }
        }

        Column {
          anchors.horizontalCenter: parent.horizontalCenter
          anchors.top: parent.top
          anchors.topMargin: Math.max(42, parent.height * 0.12)
          spacing: -4

          Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Qt.formatDateTime(root.now, "HH:mm")
            color: root.text
            font.family: root.fontFamily
            font.pixelSize: root.textClock
            font.weight: root.weightLight
          }

          Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Qt.formatDateTime(root.now, "dddd · yyyy-MM-dd")
            color: root.subtext
            font.family: root.fontFamily
            font.pixelSize: root.textLead
          }
        }

        Item {
          id: authCard
          anchors.centerIn: parent
          anchors.verticalCenterOffset: 34
          width: 360
          height: 220

          Column {
            anchors.fill: parent
            anchors.margins: 22
            spacing: 13

            // The one place on the lock where the accent names whose session
            // this is: the initial in the primary container, ringed in the
            // primary colour.
            Rectangle {
              anchors.horizontalCenter: parent.horizontalCenter
              width: 72
              height: 72
              radius: width / 2
              color: root.roles.primaryContainer
              border.width: root.focusWidth
              border.color: root.roles.primary

              Text {
                anchors.centerIn: parent
                text: root.displayName.charAt(0)
                color: root.roles.textOnPrimaryContainer
                font.family: root.fontFamily
                font.pixelSize: root.textMark
                font.weight: root.weightStrong
              }
            }

            Text {
              anchors.horizontalCenter: parent.horizontalCenter
              text: root.displayName
              color: root.text
              font.family: root.fontFamily
              font.pixelSize: root.textLead
              font.weight: root.weightStrong
            }

            // The password field is Material's outlined field drawn as a
            // pill, straight on the wallpaper: no container behind it, an
            // outline in the primary colour once the session is secure and
            // in the error colour when a password was refused.
            Rectangle {
              width: parent.width
              height: 48
              radius: height / 2
              color: root.alpha(root.roles.surfaceContainerLowest, 0)
              border.width: root.focusWidth
              border.color: root.authFailed ? root.roles.error : root.secure ? root.roles.primary : root.roles.outline

              TextInput {
                id: passwordInput
                anchors.fill: parent
                anchors.leftMargin: 48
                anchors.rightMargin: 24
                enabled: root.secure && !root.authenticating
                focus: true
                activeFocusOnPress: true
                text: root.passwordText
                cursorVisible: false
                cursorDelegate: Item {
                  width: 0
                  height: 0
                  visible: false
                }
                verticalAlignment: TextInput.AlignVCenter
                echoMode: TextInput.Password
                passwordCharacter: "●"
                passwordMaskDelay: 0
                color: root.text
                selectionColor: root.roles.secondaryContainer
                selectedTextColor: root.text
                font.family: root.fontFamily
                font.pixelSize: root.textCard
                font.letterSpacing: 3
                clip: true

                onTextEdited: {
                  root.passwordText = text
                  if (root.authFailed) {
                    root.authFailed = false
                    root.authMessage = ""
                  }
                }

                onAccepted: {
                  var submitted = root.passwordText
                  root.passwordText = ""
                  root.submitPassword(submitted)
                }

                Keys.onPressed: function(event) {
                  if (event.key === Qt.Key_Escape || (event.modifiers & Qt.ControlModifier && event.key === Qt.Key_U)) {
                    root.passwordText = ""
                    event.accepted = true
                  }
                }

                Connections {
                  target: root
                  function onFocusPassword() {
                    if (root.secure && !root.authenticating) Qt.callLater(passwordInput.forceActiveFocus)
                  }
                }
              }

              Text {
                anchors.left: parent.left
                anchors.leftMargin: 48
                anchors.right: parent.right
                anchors.rightMargin: 24
                anchors.verticalCenter: parent.verticalCenter
                visible: root.passwordText.length === 0
                text: !root.secure
                  ? "Securing session"
                  : root.authenticating
                    ? (root.authMessage || "Authenticating")
                    : root.authFailed
                      ? root.authMessage
                      : "Enter password"
                color: root.authFailed ? root.roles.error : root.subtext
                font.family: root.fontFamily
                font.pixelSize: root.textBody
                elide: Text.ElideRight
              }

              Item {
                anchors.left: parent.left
                anchors.leftMargin: 18
                anchors.verticalCenter: parent.verticalCenter
                width: 18
                height: 18

                Shared.LoadingIndicator {
                  anchors.fill: parent
                  visible: root.authenticating && !root.yubikeyTouchRequired
                  color: root.roles.primary
                }

                Text {
                  anchors.centerIn: parent
                  visible: !root.authenticating || root.yubikeyTouchRequired
                  text: root.yubikeyTouchRequired ? "" : "󰌾"
                  color: root.authFailed ? root.roles.error : root.yubikeyTouchRequired ? root.yellow : root.roles.primary
                  font.family: root.fontFamily
                  font.pixelSize: root.textCard
                }
              }
            }
          }
        }

        Rectangle {
          id: powerMenu

          visible: root.powerMenuOpen
          anchors.left: parent.left
          anchors.leftMargin: 22
          anchors.bottom: parent.bottom
          anchors.bottomMargin: 82
          width: 420
          // Padding, header, gap, two rows of buttons and the gap between them.
          height: root.panelMargin * 2 + root.panelHeaderHeight + root.panelSpacing + 72 * 2 + 8
          radius: root.radiusPanel
          color: root.panelColor
          border.width: root.hairline
          border.color: root.panelBorder

          MouseArea { anchors.fill: parent }

          Column {
            anchors.fill: parent
            anchors.margins: root.panelMargin
            spacing: root.panelSpacing

            Row {
              width: parent.width
              height: root.panelHeaderHeight
              spacing: root.panelSpacing

              Item {
                anchors.verticalCenter: parent.verticalCenter
                width: root.panelMarkSize
                height: root.panelMarkSize

                ShapeMark { anchors.fill: parent }

                Text {
                  anchors.centerIn: parent
                  text: "󰐥"
                  color: root.roles.textOnPrimaryContainer
                  font.family: root.fontFamily
                  font.pixelSize: root.textSubhead
                }
              }

              Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "Power"
                color: root.text
                font.family: root.fontFamily
                font.pixelSize: root.textTitle
                font.weight: root.weightStrong
              }
            }

            Grid {
              width: parent.width
              columns: 3
              columnSpacing: 8
              rowSpacing: 8

              Repeater {
                model: [
                  { label: "Windows", icon: "󰍲", action: "reboot-windows", variant: "default" },
                  { label: "Lock", icon: "󰌾", action: "lock", variant: "default" },
                  { label: "Log out", icon: "󰍃", action: "logout", variant: "default" },
                  { label: "Suspend", icon: "󰒲", action: "suspend", variant: "default" },
                  { label: "Reboot", icon: "󰜉", action: "reboot", variant: "default" },
                  { label: "Shut down", icon: "󰐥", action: "poweroff", variant: "destructive" }
                ]

                PowerButton {
                  required property var modelData
                  action: modelData.action
                  icon: modelData.icon
                  label: modelData.label
                  variant: modelData.variant
                }
              }
            }
          }
        }

        Rectangle {
          id: powerMenuButton

          anchors.left: parent.left
          anchors.leftMargin: 22
          anchors.bottom: parent.bottom
          anchors.bottomMargin: 22
          // Material's icon button at its large size: round at rest, squared
          // towards the large corner while its grid is open, pinched while
          // it is held.
          width: 48
          height: 48
          radius: powerMenuMouse.pressed ? root.shapeSmall : root.powerMenuOpen ? root.shapeLarge : width / 2
          color: root.powerMenuOpen ? root.roles.secondaryContainer : root.cardColor

          Behavior on color { ColorAnimation { duration: root.durationFast } }
          Behavior on radius { NumberAnimation { duration: root.durationFastSpatial; easing.type: Easing.BezierSpline; easing.bezierCurve: root.springFastSpatial } }

          Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: powerMenuMouse.pressed ? root.pressColor : powerMenuMouse.containsMouse ? root.hoverColor : root.alpha(root.text, 0)
            Behavior on color { ColorAnimation { duration: root.durationFast } }
          }

          Text {
            anchors.centerIn: parent
            text: "󰐥"
            color: root.powerMenuOpen ? root.roles.textOnSecondaryContainer : root.text
            font.family: root.fontFamily
            font.pixelSize: root.textDisplay
          }

          MouseArea {
            id: powerMenuMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.powerMenuOpen = !root.powerMenuOpen
          }
        }
      }
    }
  }
}
