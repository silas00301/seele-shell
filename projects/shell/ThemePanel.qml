pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// A carousel of palettes, after Omarchy's: the chosen preset large in the
// middle as a small desktop drawn in its own colours, its neighbours as narrow
// slices fading off to either side, and its name beneath. Every preset is on
// it, and moving switches the desktop at once, so the shell around the picker
// repaints with each step.
//
// A preset chosen here becomes the theme for its own mode and brings that mode
// with it, so the switcher needs nothing else. Light, Dark or Auto, the
// schedule, and a preset worn by the other mode belong to the Control Center's
// Themes panel. The catalog, both themes, publication and the schedule belong
// to `seele-theme`; ordering and movement belong to `qml-core`.
FocusScope {
  id: panel
  required property var theme
  required property var store
  // Bounds the whole picker, so the window can hand it what its output has.
  property real maximumHeight: theme.themesMaximumHeight
  readonly property string hint: "←→ switch · Enter keeps · Esc puts back"
  readonly property int centre: store.carousel.order.indexOf(store.focusedId)
  readonly property var preview: centre >= 0 && centre < store.model.count ? store.model.get(centre).entry : null
  // Card and slice sizes shrink together on a short output, keeping 16:10.
  readonly property real cardHeight: Math.max(160, Math.min(300, maximumHeight - chrome.height - labels.height - content.spacing * 2))
  readonly property real cardWidth: Math.round(cardHeight * 1.6)
  readonly property real sliceWidth: Math.round(cardHeight * 0.19)
  readonly property real sliceHeight: Math.round(cardHeight * 0.86)
  readonly property real sliceGap: theme.spaceMedium
  // As many slices on each side as the width holds.
  readonly property int sideCount: Math.max(0, Math.floor((width - cardWidth - sliceGap * 2) / 2 / (sliceWidth + sliceGap)))
  implicitHeight: content.implicitHeight
  signal closeRequested()

  function handleKey(event) {
    if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
    if (event.key === Qt.Key_Left || event.key === Qt.Key_Backtab) { store.step("left"); event.accepted = true }
    else if (event.key === Qt.Key_Right || event.key === Qt.Key_Tab) { store.step("right"); event.accepted = true }
    else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { keep(); event.accepted = true }
    else if (event.key === Qt.Key_Escape) { store.cancel(); closeRequested(); event.accepted = true }
  }
  // Keeps what is on screen and closes.
  function keep() {
    store.keep()
    closeRequested()
  }

  // A line of the scene's terminal.
  component SceneLine: Text {
    textFormat: Text.StyledText
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textBody
    elide: Text.ElideRight
  }

  // A preset shown as the thing it themes: the bar, a focused terminal carrying
  // the accent on its border the way the compositor draws one and tmux's status
  // line along its foot, a notification with an accent action, a switch that is
  // on, a slider in the accent and the preset's state colours. Every colour is one of the preset's own roles;
  // every measurement is material geometry for this one scene, scaled with the
  // card.
  component Scene: Rectangle {
    id: scene
    required property var preset
    readonly property color baseColor: scene.preset.base
    readonly property color mantleColor: scene.preset.mantle
    readonly property color surfaceColor: scene.preset.surface
    readonly property color overlayColor: scene.preset.overlay
    readonly property color textColor: scene.preset.text
    readonly property color subtextColor: scene.preset.subtext
    readonly property color accentColor: scene.preset.accent
    readonly property real unit: height / 300
    readonly property int small: Math.max(panel.theme.textCaption, Math.round(11 * scene.unit))
    // One run of terminal text in one role. The roles are native-validated
    // `#rrggbb`, and the words are fixed here, so nothing outside the picker
    // reaches the rich-text parser.
    function tint(role, words) {
      return "<font color=\"" + scene.preset[role] + "\">" + words + "</font>"
    }

    radius: panel.theme.radius
    color: scene.baseColor
    antialiasing: true
    clip: true

    Rectangle {
      id: bar
      width: parent.width
      height: Math.round(26 * scene.unit)
      topLeftRadius: scene.radius
      topRightRadius: scene.radius
      color: scene.mantleColor
      Row {
        x: Math.round(12 * scene.unit)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Math.round(6 * scene.unit)
        Rectangle { width: Math.round(18 * scene.unit); height: Math.round(7 * scene.unit); radius: height / 2; color: scene.accentColor }
        Repeater {
          model: 3
          Rectangle { width: Math.round(7 * scene.unit); height: width; radius: width / 2; color: scene.overlayColor }
        }
      }
      Text {
        anchors.centerIn: parent
        text: "Sat 26  12:00"
        color: scene.textColor
        font.family: panel.theme.fontFamily
        font.pixelSize: scene.small
        font.weight: panel.theme.weightMedium
      }
      Row {
        anchors { right: parent.right; rightMargin: Math.round(12 * scene.unit); verticalCenter: parent.verticalCenter }
        spacing: Math.round(6 * scene.unit)
        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: Math.round(7 * scene.unit); height: width; radius: width / 2; color: scene.preset.green }
        Text { text: "84%"; color: scene.subtextColor; font.family: panel.theme.fontFamily; font.pixelSize: scene.small }
      }
    }

    Rectangle {
      id: terminal
      x: Math.round(16 * scene.unit)
      y: bar.height + Math.round(16 * scene.unit)
      width: Math.round(scene.width * 0.58)
      height: scene.height - y - Math.round(16 * scene.unit)
      radius: Math.round(7 * scene.unit)
      color: scene.baseColor
      border.width: Math.max(2, Math.round(2 * scene.unit))
      border.color: scene.accentColor
      Column {
        x: Math.round(14 * scene.unit)
        y: Math.round(12 * scene.unit)
        width: parent.width - x * 2
        spacing: Math.round(4 * scene.unit)
        SceneLine { width: parent.width; text: scene.tint("accent", "~/seele") + " " + scene.tint("overlay", "on") + " " + scene.tint("green", "main") }
        SceneLine { width: parent.width; text: scene.tint("accent", "❯") + " " + scene.tint("text", "jj log --limit 2") }
        SceneLine { width: parent.width; text: scene.tint("yellow", "◉") + " " + scene.tint("text", "rebuild the picker") + " " + scene.tint("overlay", "2m") }
        SceneLine { width: parent.width; text: scene.tint("subtext", "○") + " " + scene.tint("subtext", "legible quiet text") + " " + scene.tint("overlay", "1h") }
        SceneLine { width: parent.width; text: scene.tint("accent", "❯") + " " + scene.tint("text", "nix flake check") }
        SceneLine { width: parent.width; text: scene.tint("green", "✓") + " " + scene.tint("subtext", "13 presets passed") }
        SceneLine { width: parent.width; text: scene.tint("red", "✗") + " " + scene.tint("text", "one check failed") + " " + scene.tint("overlay", "# see log") }
        Row {
          spacing: Math.round(6 * scene.unit)
          SceneLine { text: scene.tint("accent", "❯") }
          Rectangle { anchors.verticalCenter: parent.verticalCenter; width: Math.round(7 * scene.unit); height: Math.round(14 * scene.unit); color: scene.textColor }
        }
      }
      // tmux's status line, which the switch recolours with the terminal.
      Rectangle {
        x: terminal.border.width
        y: terminal.height - height - terminal.border.width
        width: terminal.width - terminal.border.width * 2
        height: Math.round(20 * scene.unit)
        bottomLeftRadius: terminal.radius - terminal.border.width
        bottomRightRadius: terminal.radius - terminal.border.width
        color: scene.mantleColor
        Row {
          anchors.verticalCenter: parent.verticalCenter
          spacing: Math.round(8 * scene.unit)
          Rectangle {
            width: activeWindow.implicitWidth + Math.round(12 * scene.unit)
            height: Math.round(20 * scene.unit)
            color: scene.accentColor
            Text { id: activeWindow; anchors.centerIn: parent; text: "0:fish"; color: scene.baseColor; font.family: panel.theme.fontFamily; font.pixelSize: scene.small; font.weight: panel.theme.weightStrong }
          }
          Text { anchors.verticalCenter: parent.verticalCenter; text: "1:nvim"; color: scene.subtextColor; font.family: panel.theme.fontFamily; font.pixelSize: scene.small }
        }
        Text {
          anchors { right: parent.right; rightMargin: Math.round(10 * scene.unit); verticalCenter: parent.verticalCenter }
          text: "seele"
          color: scene.overlayColor
          font.family: panel.theme.fontFamily
          font.pixelSize: scene.small
        }
      }
    }

    Column {
      x: terminal.x + terminal.width + Math.round(14 * scene.unit)
      y: terminal.y
      width: scene.width - x - Math.round(16 * scene.unit)
      spacing: Math.round(14 * scene.unit)
      Rectangle {
        width: parent.width
        height: Math.round(86 * scene.unit)
        radius: Math.round(7 * scene.unit)
        color: scene.surfaceColor
        Column {
          x: Math.round(11 * scene.unit)
          y: Math.round(10 * scene.unit)
          width: parent.width - x * 2
          spacing: Math.round(3 * scene.unit)
          Text {
            width: parent.width
            text: "Build finished"
            elide: Text.ElideRight
            color: scene.textColor
            font.family: panel.theme.fontFamily
            font.pixelSize: Math.max(panel.theme.textLabel, Math.round(12 * scene.unit))
            font.weight: panel.theme.weightStrong
          }
          Text {
            width: parent.width
            text: "nerv · just now"
            elide: Text.ElideRight
            color: scene.subtextColor
            font.family: panel.theme.fontFamily
            font.pixelSize: scene.small
          }
        }
        Rectangle {
          anchors { right: parent.right; bottom: parent.bottom; margins: Math.round(10 * scene.unit) }
          width: openLabel.implicitWidth + Math.round(16 * scene.unit)
          height: Math.round(20 * scene.unit)
          radius: Math.round(5 * scene.unit)
          color: scene.accentColor
          Text {
            id: openLabel
            anchors.centerIn: parent
            text: "Open"
            color: scene.baseColor
            font.family: panel.theme.fontFamily
            font.pixelSize: scene.small
            font.weight: panel.theme.weightStrong
          }
        }
      }
      Rectangle {
        width: parent.width
        height: Math.round(40 * scene.unit)
        radius: Math.round(7 * scene.unit)
        color: scene.surfaceColor
        Text {
          anchors { left: parent.left; leftMargin: Math.round(11 * scene.unit); verticalCenter: parent.verticalCenter }
          text: "Do not disturb"
          color: scene.textColor
          font.family: panel.theme.fontFamily
          font.pixelSize: scene.small
        }
        Rectangle {
          id: toggle
          anchors { right: parent.right; rightMargin: Math.round(11 * scene.unit); verticalCenter: parent.verticalCenter }
          width: Math.round(34 * scene.unit)
          height: Math.round(18 * scene.unit)
          radius: height / 2
          color: scene.accentColor
          Rectangle {
            anchors { right: parent.right; rightMargin: Math.round(3 * scene.unit); verticalCenter: parent.verticalCenter }
            width: toggle.height - Math.round(6 * scene.unit)
            height: width
            radius: width / 2
            color: scene.baseColor
          }
        }
      }
      Item {
        width: parent.width
        height: Math.round(14 * scene.unit)
        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width; height: Math.round(4 * scene.unit); radius: height / 2; color: scene.surfaceColor }
        Rectangle { id: fill; anchors.verticalCenter: parent.verticalCenter; width: Math.round(parent.width * 0.62); height: Math.round(4 * scene.unit); radius: height / 2; color: scene.accentColor }
        Rectangle { x: fill.width - width / 2; anchors.verticalCenter: parent.verticalCenter; width: Math.round(14 * scene.unit); height: width; radius: width / 2; color: scene.textColor }
      }
      Row {
        spacing: Math.round(6 * scene.unit)
        Repeater {
          model: [scene.preset.accent, scene.preset.red, scene.preset.green, scene.preset.yellow]
          Rectangle {
            required property var modelData
            width: Math.round(22 * scene.unit)
            height: width
            radius: Math.round(5 * scene.unit)
            color: modelData
          }
        }
      }
    }
  }

  // A neighbour as a narrow strip of itself: its bar, a window wearing its
  // accent border, and its state colours stacked beneath.
  component Slice: Rectangle {
    id: slice
    required property var preset
    radius: panel.theme.radius
    color: slice.preset.base
    antialiasing: true
    clip: true
    Rectangle {
      width: parent.width
      height: Math.round(parent.height * 0.08)
      topLeftRadius: slice.radius
      topRightRadius: slice.radius
      color: slice.preset.mantle
    }
    Rectangle {
      x: Math.round(parent.width * 0.16)
      y: Math.round(parent.height * 0.16)
      width: parent.width - x * 2
      height: Math.round(parent.height * 0.46)
      radius: 4
      color: slice.preset.surface
      border.width: 2
      border.color: slice.preset.accent
    }
    Column {
      anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: Math.round(parent.height * 0.06) }
      spacing: 5
      Repeater {
        model: [slice.preset.accent, slice.preset.red, slice.preset.green, slice.preset.yellow]
        Rectangle {
          required property var modelData
          width: 8
          height: 8
          radius: 4
          color: modelData
        }
      }
    }
  }

  Column {
    id: content
    width: parent.width
    spacing: panel.theme.panelSpacing

    Column {
      id: chrome
      width: parent.width
      spacing: panel.theme.panelSpacing
      visible: panel.store.error !== "" || panel.store.reloadPending !== ""

      Shared.StatusBanner {
        theme: panel.theme
        width: parent.width
        visible: panel.store.error !== ""
        glyph: "󰀦"
        tint: panel.theme.yellow
        title: "The theme catalog is unavailable"
        detail: panel.store.error
        Shared.ActionButton {
          theme: panel.theme
          objectName: "retry"
          text: "Retry"
          enabled: !panel.store.busy
          onClicked: panel.store.refresh()
        }
      }
      Shared.StatusBanner {
        theme: panel.theme
        width: parent.width
        visible: panel.store.reloadPending !== ""
        glyph: "󰑐"
        tint: panel.theme.yellow
        title: "Some applications keep the old colors"
        detail: panel.store.reloadPending
      }
    }

    // The carousel. The centre is the preset on screen; every other entry
    // sits a fixed step away on its side and slides there.
    Item {
      id: strip
      objectName: "carousel"
      width: parent.width
      height: visible ? panel.cardHeight : 0
      visible: panel.store.model.count > 0
      Repeater {
        model: panel.store.model
        delegate: Item {
          id: card
          required property var entry
          required property int index
          readonly property int offset: card.index - panel.centre
          readonly property bool centred: card.offset === 0
          readonly property real origin: (strip.width - panel.cardWidth) / 2
          objectName: "card-" + card.entry.id
          visible: Math.abs(card.offset) <= panel.sideCount
          z: card.centred ? 100 : 50 - Math.abs(card.offset)
          width: card.centred ? panel.cardWidth : panel.sliceWidth
          height: card.centred ? panel.cardHeight : panel.sliceHeight
          x: card.centred ? card.origin
            : card.offset < 0 ? card.origin + card.offset * (panel.sliceWidth + panel.sliceGap)
            : card.origin + panel.cardWidth + panel.sliceGap + (card.offset - 1) * (panel.sliceWidth + panel.sliceGap)
          y: (strip.height - height) / 2
          Behavior on x { NumberAnimation { duration: panel.theme.durationNormal; easing.type: Easing.OutCubic } }
          Behavior on width { NumberAnimation { duration: panel.theme.durationNormal; easing.type: Easing.OutCubic } }
          Behavior on height { NumberAnimation { duration: panel.theme.durationNormal; easing.type: Easing.OutCubic } }

          Loader {
            anchors.fill: parent
            active: card.centred
            sourceComponent: Scene { objectName: "scene"; preset: card.entry }
          }
          Loader {
            anchors.fill: parent
            active: !card.centred
            sourceComponent: Slice { preset: card.entry }
          }
          // Neighbours sit back under a light veil, kept light so that
          // under a pale shell they fade rather than turn grey and lose the
          // colours they are there to show; the centre is lit by the
          // shell's own accent, the way the keyboard is shown everywhere
          // else.
          Rectangle {
            anchors.fill: parent
            radius: panel.theme.radius
            color: panel.theme.alpha(panel.theme.crust, card.centred ? 0 : sliceMouse.containsMouse ? 0.06 : 0.24)
            border.width: card.centred ? 2 : 1
            border.color: card.centred ? panel.theme.accent : panel.theme.alpha(panel.theme.text, 0.12)
            antialiasing: true
            Behavior on color { ColorAnimation { duration: panel.theme.durationFast } }
          }
          // A neighbour is chosen by clicking it; the centre, clicked, is
          // kept.
          MouseArea {
            id: sliceMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
              if (card.centred) panel.keep()
              else panel.store.choose(card.entry.id, true)
            }
          }
        }
      }
    }

    Column {
      id: labels
      width: parent.width
      spacing: panel.theme.spaceSmall

      Shared.EmptyState {
        theme: panel.theme
        width: parent.width
        visible: panel.store.model.count === 0
        glyph: "󰔎"
        title: "No themes available"
        detail: "The catalog is generated during a rebuild. Refresh once the desktop has one."
        Shared.ActionButton {
          theme: panel.theme
          objectName: "emptyAction"
          text: "Refresh"
          enabled: !panel.store.busy
          onClicked: panel.store.refresh()
        }
      }

      Text {
        objectName: "name"
        width: parent.width
        visible: !!panel.preview
        text: panel.preview ? panel.preview.name : ""
        textFormat: Text.PlainText
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        color: panel.theme.text
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textDisplay
        font.weight: panel.theme.weightStrong
      }
      // Which mode the preset on screen belongs to, and so which it now is
      // the theme for.
      Text {
        objectName: "kind"
        width: parent.width
        visible: !!panel.preview
        text: panel.preview ? (panel.preview.mode === "light" ? "󰖙  Light" : "󰖔  Dark") + " · " + (panel.centre + 1) + " of " + panel.store.model.count : ""
        textFormat: Text.PlainText
        horizontalAlignment: Text.AlignHCenter
        color: panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }
      Text {
        width: parent.width
        visible: panel.store.actionError !== ""
        text: panel.store.actionError
        textFormat: Text.PlainText
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        color: panel.theme.red
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textBody
      }
      Text {
        objectName: "hint"
        width: parent.width
        text: panel.hint
        textFormat: Text.PlainText
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        color: panel.theme.overlay
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }
    }
  }
  Keys.onPressed: event => panel.handleKey(event)
}
