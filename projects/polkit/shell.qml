//@ pragma UseQApplication

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Polkit
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

  // Shell chrome tokens. These mirror the block at the top of seele-shell's
  // shell.qml so the dialog is the same material as every other surface rather
  // than a lookalike: one radius, the same translucent fill the compositor
  // blurs, the same wash and grain film over it.
  readonly property int radius: 8
  readonly property string grain: "grain.png"
  readonly property real grainOpacity: 0.05
  readonly property color panelColor: alpha(base, 0.86)
  readonly property color panelBorder: alpha(accent, 0.65)

  // `flow` is null whenever polkit has nothing outstanding, so every binding
  // below has to tolerate that rather than assume a live request.
  readonly property var flow: agent.flow
  readonly property bool prompting: flow !== null && !flow.isCompleted
  readonly property bool canType: prompting && flow.isResponseRequired

  // pam_u2f runs first and blocks for the touch, so PAM has not asked for a
  // password yet while the key is waiting. Hold anything typed during that
  // window and submit it the moment the conversation actually asks, so both
  // routes are open at once even though the stack itself is sequential.
  property string pendingPassword: ""

  function alpha(color, a) {
    return Qt.rgba(color.r, color.g, color.b, a)
  }

  function submitPassword(value) {
    if (!prompting) return
    if (flow.isResponseRequired) {
      flow.submit(value)
      pendingPassword = ""
    } else {
      pendingPassword = value
    }
  }

  function flushPendingPassword() {
    if (!canType || pendingPassword.length === 0) return
    var held = pendingPassword
    pendingPassword = ""
    flow.submit(held)
  }

  function cancel() {
    if (!prompting) return
    flow.cancelAuthenticationRequest()
  }

  function escapeMarkup(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
  }

  // polkit hands over one sentence and no separate field naming the requester,
  // so the name has to be lifted out of the message itself. An action written
  // for an application opens with it -- "1Password SSH Agent is trying to ..."
  // -- while polkit's own generic phrasing opens "Authentication is required
  // to ...", which names nobody and is left plain.
  function requesterMarkup(message) {
    var text = String(message || "")
    var split = text.indexOf(" is ")
    if (split <= 0) return escapeMarkup(text)

    var requester = text.substring(0, split)
    if (requester === "Authentication" || requester === "Authorization") return escapeMarkup(text)

    return "<font color=\"" + String(root.accent) + "\">" + escapeMarkup(requester) + "</font>"
      + escapeMarkup(text.substring(split))
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
      } catch (error) {
        console.warn("seele-polkit/theme", error)
      }
    }
  }

  // The desktop shell shows its own YubiKey touch OSD from the touch detector's
  // socket. While this dialog is up it already says the same thing, and the OSD
  // would sit behind an overlay that covers the screen, so publish the dialog's
  // state and let the shell stand down.
  FileView {
    id: stateFile
    path: Quickshell.env("XDG_RUNTIME_DIR") + "/seele-polkit.state"
    printErrors: false
  }

  onPromptingChanged: stateFile.setText(prompting ? "1" : "0")
  Component.onCompleted: stateFile.setText("0")

  PolkitAgent {
    id: agent
  }

  PanelWindow {
    id: window

    visible: root.prompting
    color: "transparent"
    exclusiveZone: 0

    anchors {
      top: true
      bottom: true
      left: true
      right: true
    }

    WlrLayershell.layer: WlrLayer.Overlay
    // Named into the shell's namespace so the compositor's blur and no-anim
    // layer rules cover it like every other Seele surface.
    WlrLayershell.namespace: "seele-shell-polkit"
    // The dialog is the only thing that should receive keys while it is up: a
    // password typed into whatever sits behind it would be both lost and leaked.
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

    Rectangle {
      anchors.fill: parent
      color: root.alpha(root.mantle, 0.4)

      MouseArea {
        anchors.fill: parent
        onClicked: root.cancel()
      }
    }

    Rectangle {
      id: card

      anchors.centerIn: parent
      width: 360
      height: column.implicitHeight + 44
      radius: root.radius
      color: root.panelColor
      border.width: 1
      border.color: root.panelBorder
      antialiasing: true

      SurfaceWash { radius: root.radius - 1 }
      SurfaceGrain { inset: 3 }

      // Swallow clicks so the click-away behind cannot cancel through the card.
      MouseArea {
        anchors.fill: parent
      }

      Column {
        id: column

        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: 22
        anchors.rightMargin: 22
        spacing: 13
        z: 2

        Text {
          anchors.horizontalCenter: parent.horizontalCenter
          text: ""
          color: root.yellow
          font.family: root.fontFamily
          font.pixelSize: 34
        }

        Text {
          anchors.horizontalCenter: parent.horizontalCenter
          text: "Touch your YubiKey"
          color: root.text
          font.family: root.fontFamily
          font.pixelSize: 13
          font.bold: true
        }

        // What is actually being authorised. Without it the dialog would ask for
        // a touch while saying nothing about what the touch approves.
        Text {
          width: parent.width
          horizontalAlignment: Text.AlignHCenter
          visible: root.prompting && root.flow.message.length > 0
          textFormat: Text.StyledText
          text: root.prompting ? root.requesterMarkup(root.flow.message) : ""
          color: root.subtext
          font.family: root.fontFamily
          font.pixelSize: 11
          wrapMode: Text.WordWrap
        }

        Rectangle {
          width: parent.width
          height: 48
          radius: root.radius
          color: root.alpha(root.surface, 0.45)
          border.width: 2
          border.color: root.prompting && root.flow.supplementaryIsError
            ? root.red
            : passwordInput.activeFocus ? root.accent : root.alpha(root.overlay, 0.8)

          TextInput {
            id: passwordInput

            anchors.fill: parent
            anchors.leftMargin: 16
            anchors.rightMargin: 16
            enabled: root.prompting
            focus: true
            activeFocusOnPress: true
            // No caret, matching the lock screen: the masked dots are the only
            // feedback either surface gives.
            cursorVisible: false
            cursorDelegate: Item {
              width: 0
              height: 0
              visible: false
            }
            verticalAlignment: TextInput.AlignVCenter
            echoMode: root.prompting && root.flow.responseVisible ? TextInput.Normal : TextInput.Password
            passwordCharacter: "●"
            passwordMaskDelay: 0
            color: root.text
            selectionColor: root.alpha(root.accent, 0.45)
            selectedTextColor: root.text
            font.family: root.fontFamily
            font.pixelSize: 15
            font.letterSpacing: 3
            clip: true

            onAccepted: {
              var submitted = text
              text = ""
              root.submitPassword(submitted)
            }

            Keys.onPressed: function(event) {
              if (event.key === Qt.Key_Escape) {
                root.cancel()
                event.accepted = true
              } else if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_U) {
                passwordInput.clear()
                event.accepted = true
              }
            }
          }

          Text {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            visible: passwordInput.text.length === 0
            // While the key is still being waited on the field is live but PAM
            // has not asked yet, so say the password is an option rather than
            // implying the dialog is busy.
            text: !root.prompting
              ? ""
              : root.pendingPassword.length > 0
                ? "Password held until asked"
                : root.canType
                  ? "Enter password"
                  : root.flow.supplementaryMessage.length > 0
                    ? root.flow.supplementaryMessage
                    : "…or type your password"
            color: root.prompting && root.flow.supplementaryIsError ? root.red : root.subtext
            font.family: root.fontFamily
            font.pixelSize: 11
            elide: Text.ElideRight
          }
        }
      }
    }

    // A new request reuses this window, so the field has to be cleared and
    // refocused per flow rather than once at construction.
    Connections {
      target: root
      function onFlowChanged() {
        passwordInput.text = ""
        root.pendingPassword = ""
        if (root.prompting) Qt.callLater(passwordInput.forceActiveFocus)
      }
      function onCanTypeChanged() { root.flushPendingPassword() }
    }
  }
}
