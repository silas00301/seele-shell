//@ pragma UseQApplication

import QtQuick
import "../shared/Motion.js" as Motion
import "../shared/Palette.js" as Palette
import "../shared/Shapes.js" as Shapes
import "../shared" as Shared
import Quickshell
import Quickshell.Io
import Quickshell.Services.Greetd
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
  readonly property string systemctl: "@SYSTEMCTL@"
  property string userName: "user"
  property string displayName: "User"
  property var sessionCommand: []

  // The same shape scale, type ramp and Material colour roles the lock uses,
  // so signing in and unlocking are one surface seen twice rather than two
  // designs.
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
    path: "/etc/seele-greeter/theme.json"
    printErrors: true
    onLoaded: {
      try {
        var theme = JSON.parse(text())
        Palette.assign(root, theme, true)
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

            // The lock's profile mark: the initial in the primary container,
            // ringed in the primary colour.
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
                z: 2
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

            // The lock's password field: an outlined pill straight on the
            // wallpaper, in the primary colour once the greeter can take
            // input and in the error colour when a password was refused.
            Rectangle {
              width: parent.width
              height: 48
              radius: height / 2
              color: root.alpha(root.roles.surfaceContainerLowest, 0)
              border.width: root.focusWidth
              border.color: root.authFailed ? root.roles.error : root.inputReady ? root.roles.primary : root.roles.outline

              TextInput {
                id: passwordInput

                anchors.fill: parent
                anchors.leftMargin: 48
                anchors.rightMargin: 24
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
                anchors.rightMargin: 24
                anchors.verticalCenter: parent.verticalCenter
                visible: root.passwordText.length === 0
                text: root.authenticating
                  ? (root.authMessage || "Authenticating")
                  : root.authFailed
                    ? root.authMessage
                    : root.authMessage && !root.responseRequired
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
          width: 332
          // Padding, header, gap, one row of buttons. The literal it replaces
          // was six pixels short, and the card clips, so the row lost its edge.
          height: root.panelMargin * 2 + root.panelHeaderHeight + root.panelSpacing + 72
          radius: root.radiusPanel
          color: root.panelColor
          border.width: root.hairline
          border.color: root.panelBorder
          clip: true

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
          // The lock's power button: Material's large icon button, round at
          // rest, squared towards the large corner while its grid is open.
          width: 48
          height: 48
          radius: powerMouse.pressed ? root.shapeSmall : root.powerMenuOpen ? root.shapeLarge : width / 2
          color: root.powerMenuOpen ? root.roles.secondaryContainer : root.cardColor

          Behavior on color { ColorAnimation { duration: root.durationFast } }
          Behavior on radius { NumberAnimation { duration: root.durationFastSpatial; easing.type: Easing.BezierSpline; easing.bezierCurve: root.springFastSpatial } }

          Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: powerMouse.pressed ? root.pressColor : powerMouse.containsMouse ? root.hoverColor : root.alpha(root.text, 0)
            Behavior on color { ColorAnimation { duration: root.durationFast } }
          }

          Text {
            anchors.centerIn: parent
            z: 2
            text: "󰐥"
            color: root.powerMenuOpen ? root.roles.textOnSecondaryContainer : root.text
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
