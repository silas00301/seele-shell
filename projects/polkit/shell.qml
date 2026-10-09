//@ pragma UseQApplication

import QtQuick
import "../shared/Palette.js" as Palette
import "../shared/Shapes.js" as Shapes
import Quickshell
import Quickshell.Io
import Quickshell.Services.Polkit
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

  // Shell chrome tokens. These mirror Seele Shell's own so the dialog is the
  // same object as every other surface rather than a lookalike: Material's
  // dialog corner, the colour roles from the shared derivation in Palette.js,
  // and the hairline a floating surface is edged with.
  readonly property int radiusPanel: 28
  readonly property int hairline: 1
  readonly property int focusWidth: 2
  readonly property int markSize: 56
  readonly property var roles: Palette.roles({
    base: base, mantle: mantle, crust: crust, surface: surface, overlay: overlay,
    text: text, subtext: subtext, accent: accent, red: red, green: green, yellow: yellow
  })
  // Material puts a dialog on the high surface step.
  readonly property color panelColor: roles.surfaceContainerHigh
  readonly property color panelBorder: alpha(roles.outlineVariant, 0.9)
  // The shell's type ramp and weights, so the dialog is the same object as the
  // lock's prompt and Seele Shell's own YubiKey notice.
  readonly property int textBody: 11
  readonly property int textLead: 13
  readonly property int textCard: 17
  readonly property int textDisplay: 20
  readonly property int weightStrong: Font.DemiBold

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

  FileView {
    path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/seele-shell/theme.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: {
      try {
        var theme = JSON.parse(text())
        Palette.assign(root, theme)
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
      color: root.roles.scrim

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
      radius: root.radiusPanel
      color: root.panelColor
      border.width: root.hairline
      border.color: root.panelBorder
      antialiasing: true

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

        // Material's dialog leads with its icon. The key sits in the shell's
        // shape, in the warning container, because a touch is being waited on.
        Item {
          anchors.horizontalCenter: parent.horizontalCenter
          width: root.markSize
          height: root.markSize

          Canvas {
            anchors.fill: parent
            antialiasing: true
            property color fill: root.roles.warningContainer
            onFillChanged: requestPaint()
            onPaint: {
              var context = getContext("2d")
              context.reset()
              Shapes.fill(context, Shapes.radii("cookie9"), width, height, 0, fill)
            }
          }

          Text {
            anchors.centerIn: parent
            text: ""
            color: root.roles.textOnWarningContainer
            font.family: root.fontFamily
            font.pixelSize: root.textDisplay
          }
        }

        Text {
          anchors.horizontalCenter: parent.horizontalCenter
          text: "Touch your YubiKey"
          color: root.text
          font.family: root.fontFamily
          font.pixelSize: root.textLead
          font.weight: root.weightStrong
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
          font.pixelSize: root.textBody
          wrapMode: Text.WordWrap
        }

        // The lock's password field: an outlined pill, in the primary colour
        // while it has the keyboard and in the error colour when PAM says so.
        Rectangle {
          width: parent.width
          height: 48
          radius: height / 2
          color: root.alpha(root.panelColor, 0)
          border.width: root.focusWidth
          border.color: root.prompting && root.flow.supplementaryIsError
            ? root.roles.error
            : passwordInput.activeFocus ? root.roles.primary : root.roles.outline

          TextInput {
            id: passwordInput

            anchors.fill: parent
            anchors.leftMargin: 24
            anchors.rightMargin: 24
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
            selectionColor: root.roles.secondaryContainer
            selectedTextColor: root.text
            font.family: root.fontFamily
            font.pixelSize: root.textCard
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
            anchors.leftMargin: 24
            anchors.right: parent.right
            anchors.rightMargin: 24
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
            color: root.prompting && root.flow.supplementaryIsError ? root.roles.error : root.subtext
            font.family: root.fontFamily
            font.pixelSize: root.textBody
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
