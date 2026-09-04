//@ pragma UseQApplication

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Greetd
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
  property color yellow: "#f9e2af"
  property string fontFamily: "Maple Mono NF CN"
  property string wallpaper: "/etc/wallpaper/wallpaper.jpg"
  readonly property string systemctl: "@SYSTEMCTL@"
  property string userName: "user"
  property string displayName: "User"
  property var sessionCommand: []

  readonly property int radius: 8
  readonly property int panelMargin: 16
  readonly property int panelSpacing: 10
  readonly property int panelHeaderHeight: 28
  readonly property int spaceTight: 4
  // The same type ramp, weights and elevation the lock uses, so signing in and
  // unlocking are one surface seen twice rather than two designs.
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
  readonly property color cardColor: alpha(surface, 0.55)
  readonly property color cardBorder: alpha(text, 0.07)
  readonly property bool inputReady: Greetd.available
    && (Greetd.state === GreetdState.Inactive || responseRequired)
  readonly property bool authenticating: Greetd.state === GreetdState.Authenticating
    && !responseRequired

  property date now: new Date()
  property string passwordText: ""
  property string pendingPassword: ""
  property string authMessage: ""
  property bool authFailed: false
  property bool responseRequired: false
  property bool yubikeyTouchRequired: false
  property int failedAttempts: 0
  property bool powerMenuOpen: false
  property string pendingPowerAction: ""
  property var powerCommand: []

  signal focusPassword()

  onInputReadyChanged: if (inputReady) focusDelay.restart()

  function alpha(color, opacity) {
    return Qt.rgba(color.r, color.g, color.b, opacity)
  }

  function submitPassword(password) {
    if (!inputReady || password.length === 0) return

    pendingPassword = password
    passwordText = ""
    authMessage = "Checking password"
    authFailed = false
    yubikeyTouchRequired = false

    if (Greetd.state === GreetdState.Inactive) {
      responseRequired = false
      Greetd.createSession(userName)
    } else {
      respondToPrompt()
    }
  }

  function respondToPrompt() {
    if (!responseRequired || pendingPassword.length === 0) return
    var response = pendingPassword
    pendingPassword = ""
    responseRequired = false
    Greetd.respond(response)
  }

  function authenticationFailed(message) {
    pendingPassword = ""
    passwordText = ""
    responseRequired = false
    yubikeyTouchRequired = false
    failedAttempts += 1
    authFailed = true
    authMessage = message && message.length > 0
      ? message
      : failedAttempts === 1
        ? "Authentication failed"
        : "Authentication failed · " + failedAttempts + " attempts"
    focusDelay.restart()
  }

  function requestPower(action) {
    if (powerProcess.running) return

    if (action === "suspend") {
      powerMenuOpen = false
      powerCommand = [systemctl, "suspend"]
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
    powerCommand = [systemctl, action === "reboot" ? "reboot" : "poweroff"]
    powerProcess.running = true
  }

  component LoadingSpinner: Canvas {
    id: spinner

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
      context.strokeStyle = spinner.color
      context.arc(width / 2, height / 2, Math.max(1, width / 2 - context.lineWidth), -Math.PI / 2, Math.PI)
      context.stroke()
    }

    NumberAnimation on rotation {
      from: 0
      to: 360
      duration: 720
      loops: Animation.Infinite
      running: spinner.visible
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
    z: 3

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
        color: powerButton.variant === "destructive" ? root.red : root.accent
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

  FileView {
    path: "/etc/seele-greeter/theme.json"
    printErrors: true
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
        root.yellow = theme.yellow || root.yellow
        root.fontFamily = theme.fontFamily || root.fontFamily
        root.wallpaper = theme.wallpaper || root.wallpaper
        root.userName = theme.userName || root.userName
        root.displayName = theme.displayName || root.displayName
        root.sessionCommand = theme.sessionCommand || root.sessionCommand
      } catch (error) {
        console.warn("seele-greeter/theme", error)
      }
    }
  }

  Connections {
    target: Greetd

    function onAuthMessage(message, error, required, echoResponse) {
      root.responseRequired = required
      root.authFailed = error
      root.authMessage = message
      root.yubikeyTouchRequired = !error && !required && /yubikey|finger|security key/i.test(message)
      if (required) root.respondToPrompt()
    }

    function onAuthFailure(message) {
      root.authenticationFailed(message)
    }

    function onReadyToLaunch() {
      root.authMessage = "Starting session"
      root.authFailed = false
      root.yubikeyTouchRequired = false
      Greetd.launch(root.sessionCommand)
    }

    function onError(message) {
      root.authenticationFailed(message)
    }
  }

  Timer {
    interval: 1000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: root.now = new Date()
  }

  Timer {
    id: focusDelay
    interval: 120
    running: true
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
      onStreamFinished: if (String(text).trim() !== "") {
        root.authFailed = true
        root.authMessage = "Power action failed"
      }
    }
  }

  Variants {
    model: Quickshell.screens

    PanelWindow {
      id: greeterWindow

      required property var modelData

      screen: modelData
      anchors { top: true; bottom: true; left: true; right: true }
      exclusionMode: ExclusionMode.Ignore
      color: root.base
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
      WlrLayershell.namespace: "seele-greeter"

      Rectangle {
        anchors.fill: parent
        color: root.base

        Image {
          anchors.fill: parent
          source: "file://" + root.wallpaper
          fillMode: Image.PreserveAspectCrop
          asynchronous: true
          cache: true
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
          width: Math.min(360, parent.width - 48)
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
                z: 2
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
              border.color: root.authFailed ? root.red : root.inputReady ? root.accent : root.overlay

              TextInput {
                id: passwordInput

                anchors.fill: parent
                anchors.leftMargin: 48
                anchors.rightMargin: 16
                enabled: root.inputReady
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

                onAccepted: root.submitPassword(root.passwordText)

                Keys.onPressed: function(event) {
                  if (event.key === Qt.Key_Escape || (event.modifiers & Qt.ControlModifier && event.key === Qt.Key_U)) {
                    root.passwordText = ""
                    event.accepted = true
                  }
                }

                Connections {
                  target: root
                  function onFocusPassword() {
                    if (root.inputReady) Qt.callLater(passwordInput.forceActiveFocus)
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
                text: root.authenticating
                  ? (root.authMessage || "Authenticating")
                  : root.authFailed
                    ? root.authMessage
                    : root.authMessage && !root.responseRequired
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
          width: 332
          // Padding, header, gap, one row of buttons. The literal it replaces
          // was six pixels short, and the card clips, so the row lost its edge.
          height: root.panelMargin * 2 + root.panelHeaderHeight + root.panelSpacing + 72
          radius: root.radius
          color: root.panelColor
          border.width: 1
          border.color: root.panelBorder
          clip: true

          SurfaceWash { radius: root.radius - 1 }
          SurfaceGrain { inset: 3 }
          MouseArea { anchors.fill: parent }

          Column {
            anchors.fill: parent
            anchors.margins: root.panelMargin
            spacing: root.panelSpacing
            z: 2

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

            Row {
              width: parent.width
              spacing: 8

              PowerButton { action: "suspend"; icon: "󰒲"; label: "Suspend"; variant: "default" }
              PowerButton { action: "reboot"; icon: "󰜉"; label: "Reboot"; variant: "default" }
              PowerButton { action: "poweroff"; icon: "󰐥"; label: "Shut down"; variant: "destructive" }
            }
          }
        }

        Rectangle {
          anchors.left: parent.left
          anchors.leftMargin: 22
          anchors.bottom: parent.bottom
          anchors.bottomMargin: 22
          width: 48
          height: 48
          radius: width / 2
          color: powerMouse.pressed
            ? root.pressColor
            : powerMouse.containsMouse || root.powerMenuOpen ? root.hoverColor : root.cardColor
          border.width: root.powerMenuOpen ? 1 : 0
          border.color: root.accent

          Behavior on color { ColorAnimation { duration: root.durationFast } }

          SurfaceWash { radius: parent.radius - 1 }

          Text {
            anchors.centerIn: parent
            z: 2
            text: "󰐥"
            color: root.text
            font.family: root.fontFamily
            font.pixelSize: root.textDisplay
          }

          MouseArea {
            id: powerMouse
            anchors.fill: parent
            z: 3
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.powerMenuOpen = !root.powerMenuOpen
          }
        }
      }
    }
  }
}
