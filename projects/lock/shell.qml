//@ pragma UseQApplication

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Pam
import Quickshell.Wayland

ShellRoot {
  id: root

  property color base: "#1e1e2e"
  property color mantle: "#181825"
  property color surface: "#313244"
  property color overlay: "#6c7086"
  property color text: "#cdd6f4"
  property color subtext: "#a6adc8"
  property color accent: "#b4befe"
  property color red: "#f38ba8"
  property color green: "#a6e3a1"
  property color yellow: "#f9e2af"
  property string fontFamily: "Maple Mono NF CN"
  property string wallpaper: "/etc/wallpaper/wallpaper.jpg"

  // Keep authentication chrome on the same tokens as Seele Shell. The lock
  // stays a separate client, but it should not grow a second design system.
  readonly property int radius: 8
  readonly property int panelMargin: 16
  readonly property int panelSpacing: 10
  readonly property int panelHeaderHeight: 28
  readonly property int spaceTight: 4
  // The same type ramp and weights Seele Shell uses, so the lock is the desktop
  // at rest rather than a surface that merely resembles it.
  readonly property int textLabel: 10
  readonly property int textBody: 11
  readonly property int textLead: 13
  readonly property int textCard: 17
  readonly property int textTitle: 18
  readonly property int textDisplay: 20
  readonly property int textMark: 30
  readonly property int textClock: 72
  readonly property int weightStrong: Font.DemiBold
  readonly property int weightLight: Font.Light
  readonly property int durationFast: 110
  readonly property string grain: "grain.png"
  readonly property real grainOpacity: 0.05
  readonly property color panelColor: alpha(base, 0.86)
  readonly property color panelBorder: alpha(accent, 0.65)
  readonly property color hoverColor: alpha(accent, 0.18)
  readonly property color pressColor: alpha(accent, 0.42)
  readonly property color dangerTint: alpha(red, 0.14)
  readonly property color dangerColor: alpha(red, 0.28)
  readonly property color dangerPress: alpha(red, 0.48)

  readonly property string userName: Quickshell.env("USER") || Quickshell.env("LOGNAME") || "user"
  readonly property string displayName: Quickshell.env("SEELE_LOCK_NAME") || titleCase(userName)
  readonly property bool secure: sessionLock.secure
  readonly property bool authenticating: pam.active
  readonly property color cardColor: alpha(surface, 0.55)
  readonly property color cardBorder: alpha(text, 0.07)

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

  component PowerButton: Rectangle {
    id: powerButton

    required property string action
    required property string icon
    required property string label
    required property string variant

    width: (parent.width - 16) / 3
    height: 72
    radius: root.radius
    color: variant === "destructive"
      ? powerMouse.pressed
        ? root.dangerPress
        : powerMouse.containsMouse ? root.dangerColor : root.dangerTint
      : powerMouse.pressed ? root.pressColor : powerMouse.containsMouse ? root.hoverColor : root.cardColor
    border.width: root.pendingPowerAction === action ? 1 : 0
    border.color: variant === "destructive" ? root.red : root.accent

    Behavior on color { ColorAnimation { duration: root.durationFast } }

    Rectangle {
      visible: powerButton.border.width === 0
      anchors.fill: parent
      radius: parent.radius
      color: "transparent"
      border.width: 1
      border.color: powerButton.variant === "destructive" ? root.alpha(root.red, 0.22) : root.cardBorder
      antialiasing: true
    }

    Column {
      anchors.centerIn: parent
      spacing: root.spaceTight

      Text {
        anchors.horizontalCenter: parent.horizontalCenter
        text: powerButton.icon
        color: powerButton.variant === "destructive"
          ? root.red
          : powerButton.action === "reboot-windows" && root.pendingPowerAction === powerButton.action
            ? root.yellow
            : root.accent
        font.family: root.fontFamily
        font.pixelSize: root.textTitle
      }

      Text {
        anchors.horizontalCenter: parent.horizontalCenter
        text: root.pendingPowerAction === powerButton.action ? "Confirm" : powerButton.label
        color: root.text
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

  component LoadingSpinner: Canvas {
    id: loadingSpinner

    property color color: root.accent

    antialiasing: true
    transformOrigin: Item.Center
    onVisibleChanged: if (visible) requestPaint()
    onWidthChanged: requestPaint()
    onColorChanged: requestPaint()
    onPaint: {
      var context = getContext("2d")
      context.clearRect(0, 0, width, height)
      context.beginPath()
      context.lineWidth = Math.max(1.5, width * 0.14)
      context.lineCap = "round"
      context.strokeStyle = loadingSpinner.color
      context.arc(width / 2, height / 2, Math.max(1, width / 2 - context.lineWidth), -Math.PI / 2, Math.PI)
      context.stroke()
    }

    NumberAnimation on rotation {
      from: 0
      to: 360
      duration: 720
      loops: Animation.Infinite
      running: loadingSpinner.visible
    }
  }

  component SurfaceWash: Rectangle {
    anchors.fill: parent
    anchors.margins: 1
    color: "transparent"

    gradient: Gradient {
      GradientStop { position: 0.0; color: root.alpha(root.text, 0.06) }
      GradientStop { position: 0.55; color: "transparent" }
      GradientStop { position: 1.0; color: root.alpha(root.mantle, 0.45) }
    }
  }

  component SurfaceGrain: Item {
    id: grainLayer

    property real inset: 0

    anchors.fill: parent
    z: 1

    Image {
      anchors.fill: parent
      anchors.margins: grainLayer.inset
      source: root.grain
      fillMode: Image.Tile
      opacity: root.grainOpacity
      smooth: false
    }

    Rectangle {
      anchors.top: parent.top
      anchors.topMargin: 1
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.leftMargin: grainLayer.inset * 3
      anchors.rightMargin: grainLayer.inset * 3
      height: 1
      color: root.alpha(root.text, 0.1)
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
        root.base = theme.base || root.base
        root.mantle = theme.mantle || root.mantle
        root.surface = theme.surface || root.surface
        root.overlay = theme.overlay || root.overlay
        root.text = theme.text || root.text
        root.subtext = theme.subtext || root.subtext
        root.accent = theme.accent || root.accent
        root.red = theme.red || root.red
        root.green = theme.green || root.green
        root.yellow = theme.yellow || root.yellow
        root.fontFamily = theme.fontFamily || root.fontFamily
        root.wallpaper = theme.wallpaper || root.wallpaper
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

    // A lock the compositor refuses -- because another client already holds
    // ext-session-lock-v1 -- never changes `locked`, so the handler above never
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

            Rectangle {
              anchors.horizontalCenter: parent.horizontalCenter
              width: 72
              height: 72
              radius: width / 2
              color: root.panelColor
              border.width: 1
              border.color: root.panelBorder

              SurfaceWash { radius: parent.radius - 1 }

              Text {
                anchors.centerIn: parent
                text: root.displayName.charAt(0)
                color: root.text
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

            Rectangle {
              width: parent.width
              height: 48
              radius: root.radius
              color: "transparent"
              border.width: 2
              border.color: root.authFailed ? root.red : root.secure ? root.accent : root.overlay

              TextInput {
                id: passwordInput
                anchors.fill: parent
                anchors.leftMargin: 48
                anchors.rightMargin: 16
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
                selectionColor: root.alpha(root.accent, 0.45)
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
                anchors.rightMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                visible: root.passwordText.length === 0
                text: !root.secure
                  ? "Securing session"
                  : root.authenticating
                    ? (root.authMessage || "Authenticating")
                    : root.authFailed
                      ? root.authMessage
                      : "Enter password"
                color: root.authFailed ? root.red : root.subtext
                font.family: root.fontFamily
                font.pixelSize: root.textBody
                elide: Text.ElideRight
              }

              Item {
                anchors.left: parent.left
                anchors.leftMargin: 15
                anchors.verticalCenter: parent.verticalCenter
                width: 18
                height: 18

                LoadingSpinner {
                  anchors.fill: parent
                  visible: root.authenticating && !root.yubikeyTouchRequired
                  color: root.accent
                }

                Text {
                  anchors.centerIn: parent
                  visible: !root.authenticating || root.yubikeyTouchRequired
                  text: root.yubikeyTouchRequired ? "" : "󰌾"
                  color: root.authFailed ? root.red : root.yubikeyTouchRequired ? root.yellow : root.accent
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
          radius: root.radius
          color: root.panelColor
          border.width: 1
          border.color: root.panelBorder

          SurfaceWash { radius: root.radius - 1 }
          SurfaceGrain { inset: 3 }

          MouseArea { anchors.fill: parent }

          Column {
            anchors.fill: parent
            anchors.margins: root.panelMargin
            spacing: root.panelSpacing

            Row {
              width: parent.width
              height: root.panelHeaderHeight
              spacing: root.panelSpacing

              Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "󰐥"
                color: root.accent
                font.family: root.fontFamily
                font.pixelSize: root.textDisplay
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
          width: 48
          height: 48
          radius: width / 2
          color: powerMenuMouse.pressed
            ? root.pressColor
            : powerMenuMouse.containsMouse || root.powerMenuOpen ? root.hoverColor : root.cardColor
          border.width: root.powerMenuOpen ? 1 : 0
          border.color: root.accent

          Behavior on color { ColorAnimation { duration: root.durationFast } }

          SurfaceWash { radius: parent.radius - 1 }

          Text {
            anchors.centerIn: parent
            text: "󰐥"
            color: root.text
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
