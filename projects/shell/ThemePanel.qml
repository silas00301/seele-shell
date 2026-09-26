pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// A carousel of palettes, after Omarchy's: the chosen preset large in the
// middle as a small desktop drawn in its own colours, its neighbours as narrow
// slices fading off to either side, and its name beneath. Moving switches the
// desktop at once, so the shell around the picker repaints with each step.
//
// The picker edits one of two slots, the light theme or the dark theme; the
// mode toggle chooses which, and is also the desktop's mode. By default the
// carousel offers only presets of the mode it is editing, and one click opens
// it to every preset. The schedule that flips the mode by itself sits beside
// the toggle. The catalog, the slots, publication and the schedule belong to
// `seele-theme`; ordering, filtering and movement belong to `qml-core`.
FocusScope {
  id: panel
  required property var theme
  required property var store
  // Bounds the whole picker, so the window can hand it what its output has.
  property real maximumHeight: theme.themesMaximumHeight
  readonly property string hint: "←→ choose · ↑ light · ↓ dark · type to filter · Enter keeps · Esc puts back"
  readonly property bool light: store.mode === "light"
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
    if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) {
      if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_A) { store.toggleAll(); event.accepted = true }
      return
    }
    if (event.key === Qt.Key_Left || event.key === Qt.Key_Backtab) { store.step("left"); event.accepted = true }
    else if (event.key === Qt.Key_Right || event.key === Qt.Key_Tab) { store.step("right"); event.accepted = true }
    else if (event.key === Qt.Key_Up) { store.setMode("light"); event.accepted = true }
    else if (event.key === Qt.Key_Down) { store.setMode("dark"); event.accepted = true }
    else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { keep(); event.accepted = true }
    else if (event.key === Qt.Key_Escape) {
      // Escape first takes back a filter, and only then the picker's changes.
      if (store.query !== "") store.query = ""
      else { store.cancel(); closeRequested() }
      event.accepted = true
    }
    else if (event.key === Qt.Key_Backspace) {
      if (store.query !== "") store.query = store.query.slice(0, -1)
      event.accepted = true
    }
    else if (event.key === Qt.Key_Space) {
      if (store.query !== "") store.query = store.query + " "
      event.accepted = true
    }
    else if (event.text && event.text.length === 1 && event.text.charCodeAt(0) > 32 && event.text.charCodeAt(0) !== 127) {
      store.query = store.query + event.text
      event.accepted = true
    }
  }
  // Keeps what is on screen — including a centre a filter moved without
  // switching anything — and closes.
  function keep() {
    store.choose(store.focusedId, true)
    closeRequested()
  }
  function showEverything() {
    store.resetFilters()
    store.showAll = true
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

  // A schedule time, sent once when it is a real `HH:MM` and has changed.
  // Enter and Escape end the edit here and hand the keyboard back to the
  // carousel; neither reaches the picker, whose Enter keeps and closes.
  component TimeField: TextField {
    id: field
    property string committed: ""
    property string sent: ""
    signal chosen(string value)
    width: 64
    implicitHeight: panel.theme.chipHeight
    text: field.committed
    horizontalAlignment: Text.AlignHCenter
    maximumLength: 5
    validator: RegularExpressionValidator { regularExpression: /^([01][0-9]|2[0-3]):[0-5][0-9]$/ }
    color: panel.theme.text
    selectionColor: panel.theme.selectedColor
    font.family: panel.theme.fontFamily
    font.pixelSize: panel.theme.textBody
    function commit() {
      if (!acceptableInput || text === committed || text === sent) return
      sent = text
      chosen(text)
    }
    // Typing leaves the binding to the saved time in place, so a saved time
    // that changes replaces the edit by itself; what was sent is forgotten, so
    // the same time can be chosen again later.
    onCommittedChanged: sent = ""
    onEditingFinished: commit()
    Keys.onReturnPressed: event => { commit(); panel.forceActiveFocus(); event.accepted = true }
    Keys.onEnterPressed: event => { commit(); panel.forceActiveFocus(); event.accepted = true }
    Keys.onEscapePressed: event => { text = committed; panel.forceActiveFocus(); event.accepted = true }
    background: Rectangle {
      radius: panel.theme.radiusSmall
      color: panel.theme.wellColor
      border.width: 1
      border.color: field.activeFocus ? panel.theme.accent : field.acceptableInput ? panel.theme.cardBorder : panel.theme.red
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

      // Which slot the carousel edits, which is also the desktop's mode, and
      // the schedule that changes the mode by itself.
      Item {
        width: parent.width
        height: panel.theme.controlHeight
        Shared.SegmentWell {
          id: modes
          theme: panel.theme
          width: 196
          height: parent.height
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 2; height: parent.height; objectName: "modeLight"; text: "󰖙  Light"; selected: panel.light; onClicked: panel.store.setMode("light") }
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 2; height: parent.height; objectName: "modeDark"; text: "󰖔  Dark"; selected: !panel.light; onClicked: panel.store.setMode("dark") }
        }
        Row {
          anchors { right: parent.right; verticalCenter: parent.verticalCenter }
          spacing: panel.theme.spaceMedium
          Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "Auto"
            color: panel.theme.subtext
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textLabel
            font.weight: panel.theme.weightMedium
          }
          Shared.SegmentWell {
            theme: panel.theme
            width: 246
            height: modes.height
            Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "autoOff"; text: "Off"; selected: panel.store.autoSource === "off"; onClicked: panel.store.setAuto("off") }
            Shared.SegmentChoice {
              theme: panel.theme
              width: parent.width / 3
              height: parent.height
              objectName: "autoSun"
              text: "Sun"
              selected: panel.store.autoSource === "sun"
              // Sunrise and sunset need the timezone's city; without one the
              // choice is shown but cannot be made.
              enabled: !!panel.store.appearance.place
              onClicked: panel.store.setAuto("sun")
            }
            Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "autoSchedule"; text: "Times"; selected: panel.store.autoSource === "schedule"; onClicked: panel.store.setAuto("schedule") }
          }
        }
      }
    }

    // The carousel. The centre is the preset on screen for the edited mode;
    // every other entry sits a fixed step away on its side and slides there.
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
          // Neighbours sit back under a light veil, kept light so that under a
          // pale shell they fade rather than turn grey and lose the colours
          // they are there to show; the centre is lit by the shell's own
          // accent, the way the keyboard is shown everywhere else.
          Rectangle {
            anchors.fill: parent
            radius: panel.theme.radius
            color: panel.theme.alpha(panel.theme.crust, card.centred ? 0 : sliceMouse.containsMouse ? 0.06 : 0.24)
            border.width: card.centred ? 2 : 1
            border.color: card.centred ? panel.theme.accent : panel.theme.alpha(panel.theme.text, 0.12)
            antialiasing: true
            Behavior on color { ColorAnimation { duration: panel.theme.durationFast } }
          }
          // A neighbour is chosen by clicking it; the centre, clicked, is kept.
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
        glyph: panel.store.query !== "" ? "󰍉" : "󰔎"
        title: panel.store.total === 0 ? "No themes available"
          : panel.store.query !== "" ? "Nothing matches “" + panel.store.query + "”"
          : "No " + panel.store.mode + " presets"
        detail: panel.store.total === 0
          ? "The catalog is generated during a rebuild. Refresh once the desktop has one."
          : "Any preset may be the " + panel.store.mode + " theme."
        Shared.ActionButton {
          theme: panel.theme
          objectName: "emptyAction"
          text: panel.store.total === 0 ? "Refresh" : "Show all presets"
          enabled: panel.store.total > 0 || !panel.store.busy
          onClicked: {
            if (panel.store.total === 0) panel.store.refresh()
            else panel.showEverything()
          }
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

      // What is being edited, how much of the catalog is shown, and the way
      // to the rest.
      Row {
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: panel.theme.spaceMedium
        visible: panel.store.total > 0
        Text {
          objectName: "scope"
          anchors.verticalCenter: parent.verticalCenter
          text: (panel.light ? "Light theme" : "Dark theme")
            + (panel.centre >= 0 ? " · " + (panel.centre + 1) + " of " + panel.store.model.count : "")
          textFormat: Text.PlainText
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
        Shared.ActionButton {
          objectName: "scopeToggle"
          theme: panel.theme
          implicitHeight: panel.theme.chipHeight
          text: panel.store.showAll ? "Only " + panel.store.mode + " presets" : "Show all " + panel.store.total
          onClicked: panel.store.toggleAll()
        }
      }

      Text {
        objectName: "filter"
        width: parent.width
        visible: panel.store.query !== ""
        text: "󰍉  " + panel.store.query
        textFormat: Text.PlainText
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        color: panel.theme.accent
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textBody
      }

      // The schedule's two times, while it keeps them.
      Row {
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: panel.theme.spaceMedium
        visible: panel.store.autoSource === "schedule"
        Text {
          anchors.verticalCenter: parent.verticalCenter
          text: "󰖙  Light from"
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
        TimeField {
          objectName: "lightAt"
          committed: (panel.store.appearance.auto || {}).lightAt || "07:00"
          onChosen: value => panel.store.setAuto("schedule", value, (panel.store.appearance.auto || {}).darkAt)
        }
        Text {
          anchors.verticalCenter: parent.verticalCenter
          text: "󰖔  Dark from"
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
        TimeField {
          objectName: "darkAt"
          committed: (panel.store.appearance.auto || {}).darkAt || "19:00"
          onChosen: value => panel.store.setAuto("schedule", (panel.store.appearance.auto || {}).lightAt, value)
        }
      }
      // What the schedule will do next.
      Text {
        objectName: "schedule"
        width: parent.width
        visible: text !== ""
        text: panel.store.scheduleText
        textFormat: Text.PlainText
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
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
