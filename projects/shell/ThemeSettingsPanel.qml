pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "../shared" as Shared

// The Control Center's Themes panel: Light, Dark or Auto, when Auto switches,
// and a gallery of every preset as a tile of the wallpaper it is linked to.
// Clicking a tile wears it now, as the switcher does, and its sun and moon
// pins make it the theme that mode wears, whatever its own.
// Every change is sent to `seele-theme` through the same store the switcher
// uses, so the two never disagree about what is applied.
FocusScope {
  id: panel
  required property var theme
  required property var store
  // Bounds the whole panel, so the window can hand it what its output has.
  // The gallery scrolls inside what is left; where that is less than a row of
  // tiles, the gallery keeps one row and the whole panel scrolls instead, so
  // nothing reaches past the output.
  property real maximumHeight: theme.themeSettingsMaximumHeight
  // The scroll bars show while the pointer is over the panel.
  readonly property bool popupHovered: panelHover.hovered
  readonly property int columns: Math.max(1, Math.floor((width + theme.spaceMedium) / (theme.themeGalleryCardMinimum + theme.spaceMedium)))
  readonly property real cardWidth: Math.floor((width - (columns - 1) * theme.spaceMedium) / columns)
  readonly property real tileHeight: Math.round(cardWidth / theme.themeTileAspect)
  implicitHeight: Math.min(content.implicitHeight, maximumHeight)
  signal browseRequested()
  HoverHandler { id: panelHover }

  // Scrolls `view` so `item` is whole, or as much of it as fits from its top.
  function scrollTo(view, item) {
    var top = item.mapToItem(view.contentItem, 0, 0).y
    if (top < view.contentY) view.contentY = top
    else if (top + item.height > view.contentY + view.height) view.contentY = Math.min(top, top + item.height - view.height)
  }
  // Brings `card` into view, as the keyboard reaches it: within the gallery,
  // then within the panel when the panel itself scrolls.
  function reveal(card) {
    panel.scrollTo(gallery, card)
    panel.scrollTo(view, card)
  }

  // A schedule time, sent once when it is a real `HH:MM` and has changed:
  // on Enter or when the field is left. Escape puts the saved time back and
  // hands the keyboard to the panel, so only a second Escape closes it.
  component TimeField: TextField {
    id: field
    property string committed: ""
    property string sent: ""
    signal chosen(string value)
    width: panel.theme.timeFieldWidth
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
    Keys.onEscapePressed: event => { text = committed; content.forceActiveFocus(); event.accepted = true }
    background: Rectangle {
      radius: panel.theme.radiusSmall
      color: panel.theme.wellColor
      border.width: panel.theme.hairline
      border.color: field.activeFocus ? panel.theme.accent : field.acceptableInput ? panel.theme.cardBorder : panel.theme.red
    }
  }

  // A preset's colours as a small chip: its base, carrying its accent and
  // state colours.
  component Swatch: Rectangle {
    id: swatch
    required property var preset
    width: panel.theme.swatchWidth
    height: panel.theme.swatchHeight
    radius: panel.theme.radiusSmall
    color: swatch.preset ? swatch.preset.base : panel.theme.wellColor
    border.width: panel.theme.hairline
    border.color: panel.theme.swatchBorder
    antialiasing: true
    Row {
      anchors.centerIn: parent
      spacing: panel.theme.spaceTight
      visible: !!swatch.preset
      Repeater {
        model: swatch.preset ? [swatch.preset.accent, swatch.preset.red, swatch.preset.green, swatch.preset.yellow] : []
        Rectangle {
          required property var modelData
          width: panel.theme.swatchDotSize
          height: panel.theme.swatchDotSize
          radius: panel.theme.swatchDotSize / 2
          color: modelData
        }
      }
    }
  }

  // One of a tile's two modes: filled while that mode wears the tile's
  // preset, a quiet disc over the picture otherwise.
  component ModePin: AbstractButton {
    id: pin
    required property var preset
    property bool selected: false
    // The catalog gives `#rrggbb`; a colour property reads it as one, which
    // the theme's alpha() needs.
    readonly property color mantleColor: pin.preset.mantle
    implicitWidth: panel.theme.compactButtonSize
    implicitHeight: panel.theme.compactButtonSize
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    Keys.onPressed: event => {
      if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
      if (!event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) pin.clicked()
      event.accepted = true
    }
    contentItem: Text {
      text: pin.text
      textFormat: Text.PlainText
      horizontalAlignment: Text.AlignHCenter
      verticalAlignment: Text.AlignVCenter
      // The theme's own colour for what sits on its switches' accent.
      color: pin.selected ? panel.theme.switchHandleOn : pin.preset.text
      font.family: panel.theme.fontFamily
      font.pixelSize: panel.theme.textLabel
    }
    background: Rectangle {
      radius: height / 2
      color: pin.selected ? panel.theme.accent : panel.theme.alpha(pin.mantleColor, panel.theme.themeTilePinOpacity)
      Shared.HoverWash { theme: panel.theme; hovered: pin.hovered && !pin.selected; pressed: pin.down; pressX: pin.pressX; pressY: pin.pressY }
      Shared.FocusRing { theme: panel.theme; shown: pin.visualFocus }
    }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
  }

  // A preset as its gallery tile: its wallpaper edge to edge, or its base
  // where it has none, its swatch, its name, and a pin for each mode. The row
  // is the store's, kept by ID; what is drawn is read from the catalog by that
  // ID, so a rebuilt catalog is shown as it now is.
  component GalleryCard: Item {
    id: card
    required property var entry
    readonly property string themeId: card.entry.id
    readonly property var preset: panel.store.find(card.themeId) || card.entry
    readonly property bool onScreen: panel.store.current === card.themeId
    readonly property bool focused: wear.activeFocus || lightPin.activeFocus || darkPin.activeFocus
    objectName: "theme-" + card.themeId
    width: panel.cardWidth
    height: panel.tileHeight
    onFocusedChanged: if (card.focused) panel.reveal(card)
    // The preset on screen is in view when the gallery opens and after a
    // switch made anywhere else: again whenever the grid places the tile or
    // the panel resizes the gallery, which settle after the tile is made.
    function follow() { if (card.onScreen) Qt.callLater(panel.reveal, card) }
    onOnScreenChanged: card.follow()
    onYChanged: card.follow()
    Component.onCompleted: card.follow()
    Connections {
      target: gallery
      function onHeightChanged() { card.follow() }
    }

    // The tile wears the preset now, the way the switcher's click does: it
    // becomes the theme for its own mode and brings that mode with it.
    AbstractButton {
      id: wear
      objectName: "wear-" + card.themeId
      anchors.fill: parent
      hoverEnabled: true
      focusPolicy: Qt.StrongFocus
      Accessible.name: "Wear " + card.preset.name
      onClicked: panel.store.wear(card.themeId)
      Keys.onPressed: event => {
        if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
        if (!event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) wear.clicked()
        event.accepted = true
      }
      background: Tile {
        preset: card.preset
        onScreen: card.onScreen
        button: wear
        Shared.FocusRing { theme: panel.theme; shown: wear.visualFocus }
      }
      HoverHandler { cursorShape: Qt.PointingHandCursor }
    }
    // Which mode each pin gives it to; a filled pin is a mode already wearing
    // it, whatever the tile's own mode.
    Row {
      anchors { top: parent.top; right: parent.right; margins: panel.theme.spaceSmall }
      spacing: panel.theme.spaceTight
      ModePin {
        id: lightPin
        objectName: "light-" + card.themeId
        preset: card.preset
        text: "󰖙"
        selected: panel.store.slots.light === card.themeId
        Accessible.name: "Use " + card.preset.name + " in light mode"
        onClicked: panel.store.assign("light", card.themeId)
      }
      ModePin {
        id: darkPin
        objectName: "dark-" + card.themeId
        preset: card.preset
        text: "󰖔"
        selected: panel.store.slots.dark === card.themeId
        Accessible.name: "Use " + card.preset.name + " in dark mode"
        onClicked: panel.store.assign("dark", card.themeId)
      }
    }
    // The preset on screen is outlined in the shell's accent, as the
    // switcher's centre is; the rest keep a hairline.
    Rectangle {
      anchors.fill: parent
      radius: panel.theme.radius
      color: panel.theme.alpha(panel.theme.accent, 0)
      border.width: card.onScreen ? panel.theme.strokeStrong : panel.theme.hairline
      border.color: card.onScreen ? panel.theme.accent : panel.theme.cardBorder
      antialiasing: true
    }
  }

  // A preset's tile face, in its own colours: its wallpaper, its swatch, and
  // its name over a shade of its mantle so the name reads on any picture.
  component Tile: Rectangle {
    id: tile
    required property var preset
    property bool onScreen: false
    // The tile's button, whose hover and press wash the picture but not the
    // name over it.
    property AbstractButton button: null
    // The catalog gives `#rrggbb`; a colour property reads it as one, which
    // the theme's alpha() needs.
    readonly property color mantleColor: tile.preset.mantle
    radius: panel.theme.radius
    color: tile.preset.base
    antialiasing: true
    clip: true
    Shared.ThemePicture {
      objectName: "wallpaper"
      anchors.fill: parent
      path: tile.preset.wallpaper || ""
      topLeftRadius: tile.radius
      topRightRadius: tile.radius
      bottomLeftRadius: tile.radius
      bottomRightRadius: tile.radius
    }
    Shared.HoverWash {
      theme: panel.theme
      hovered: !!tile.button && tile.button.hovered && !tile.onScreen
      pressed: !!tile.button && tile.button.down
      pressX: tile.button ? tile.button.pressX : width / 2
      pressY: tile.button ? tile.button.pressY : height / 2
    }
    Swatch {
      anchors { top: parent.top; left: parent.left; margins: panel.theme.spaceSmall }
      preset: tile.preset
    }
    Rectangle {
      objectName: "shade"
      anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
      height: names.implicitHeight + panel.theme.spaceLarge + panel.theme.spaceSmall
      bottomLeftRadius: tile.radius
      bottomRightRadius: tile.radius
      gradient: Gradient {
        GradientStop { position: 0; color: panel.theme.alpha(tile.mantleColor, 0) }
        GradientStop { position: 1; color: panel.theme.alpha(tile.mantleColor, panel.theme.themeTileShadeOpacity) }
      }
      Column {
        id: names
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: panel.theme.spaceSmall }
        spacing: panel.theme.spaceLine
        Text {
          objectName: "name"
          width: parent.width
          text: tile.preset.name
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: tile.preset.text
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textLabel
          font.weight: panel.theme.weightStrong
        }
        Text {
          objectName: "caption"
          width: parent.width
          visible: tile.onScreen
          text: "On screen"
          textFormat: Text.PlainText
          elide: Text.ElideRight
          color: tile.preset.accent
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
      }
    }
  }

  // A theme's own motion or sounds, and whether to let them through.
  component EffectRow: Item {
    id: effectRow
    property string title: ""
    property string detail: ""
    property bool checked: false
    signal toggled()
    width: parent ? parent.width : 0
    height: panel.theme.rowHeight
    Column {
      anchors { left: parent.left; right: effectSwitch.left; rightMargin: panel.theme.spaceMedium; verticalCenter: parent.verticalCenter }
      spacing: panel.theme.spaceLine
      Text {
        width: parent.width
        text: effectRow.title
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: panel.theme.text
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textBody
        font.weight: panel.theme.weightMedium
      }
      Text {
        width: parent.width
        text: effectRow.detail
        textFormat: Text.PlainText
        elide: Text.ElideRight
        color: panel.theme.subtext
        font.family: panel.theme.fontFamily
        font.pixelSize: panel.theme.textCaption
      }
    }
    Shared.ControlSwitch {
      id: effectSwitch
      objectName: effectRow.objectName + "Switch"
      theme: panel.theme
      anchors { right: parent.right; verticalCenter: parent.verticalCenter }
      checked: effectRow.checked
      onToggled: effectRow.toggled()
    }
  }

  Shared.SeeleFlickable {
    id: view
    objectName: "view"
    theme: panel.theme
    anchors.fill: parent
    contentWidth: width
    contentHeight: content.implicitHeight
    interactive: contentHeight > height
    clip: true
    ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }

    Column {
      id: content
      width: parent.width
      spacing: panel.theme.spaceMedium

      Column {
        id: settings
        width: parent.width
        spacing: content.spacing

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

        // Light and Dark hold the mode; Auto hands it to the schedule.
        Shared.SectionRule { theme: panel.theme; width: parent.width; label: "Appearance" }
        Shared.SegmentWell {
          theme: panel.theme
          width: parent.width
          height: panel.theme.controlHeight
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "appearanceLight"; text: "󰖙  Light"; selected: panel.store.appearanceChoice === "light"; onClicked: panel.store.setAppearance("light") }
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "appearanceDark"; text: "󰖔  Dark"; selected: panel.store.appearanceChoice === "dark"; onClicked: panel.store.setAppearance("dark") }
          Shared.SegmentChoice { theme: panel.theme; width: parent.width / 3; height: parent.height; objectName: "appearanceAuto"; text: "󰔎  Auto"; selected: panel.store.appearanceChoice === "auto"; onClicked: panel.store.setAppearance("auto") }
        }

        // When Auto switches: at sunrise and sunset, or at two fixed times.
        Item {
          width: parent.width
          height: panel.theme.controlHeight
          visible: panel.store.autoSource !== "off"
          Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "Switch at"
            color: panel.theme.subtext
            font.family: panel.theme.fontFamily
            font.pixelSize: panel.theme.textLabel
            font.weight: panel.theme.weightMedium
          }
          Shared.SegmentWell {
            theme: panel.theme
            anchors.right: parent.right
            width: Math.round(parent.width * panel.theme.themeScheduleShare)
            height: parent.height
            Shared.SegmentChoice {
              theme: panel.theme
              width: parent.width / 2
              height: parent.height
              objectName: "autoSun"
              text: "Sunrise, sunset"
              selected: panel.store.autoSource === "sun"
              // Sunrise and sunset need the timezone's city; without one the
              // choice is shown but cannot be made.
              enabled: !!panel.store.appearance.place
              onClicked: panel.store.setAuto("sun")
            }
            Shared.SegmentChoice { theme: panel.theme; width: parent.width / 2; height: parent.height; objectName: "autoSchedule"; text: "Set times"; selected: panel.store.autoSource === "schedule"; onClicked: panel.store.setAuto("schedule") }
          }
        }
        // The schedule's two times, while it keeps them.
        Item {
          width: parent.width
          height: panel.theme.chipHeight
          visible: panel.store.autoSource === "schedule"
          Row {
            anchors.right: parent.right
            spacing: panel.theme.spaceMedium
            Text {
              anchors.verticalCenter: parent.verticalCenter
              text: "󰖙  from"
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
              text: "󰖔  from"
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
        }
        // What the schedule will do next.
        Text {
          objectName: "schedule"
          width: parent.width
          visible: panel.store.autoSource !== "off" && text !== ""
          text: panel.store.scheduleText
          textFormat: Text.PlainText
          horizontalAlignment: Text.AlignRight
          wrapMode: Text.Wrap
          color: panel.theme.subtext
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }

        // Every preset, to wear now or to give to either mode, and the switcher
        // that steps through them.
        Shared.SectionRule {
          id: galleryRule
          theme: panel.theme
          width: parent.width
          label: "Themes"
          Shared.ActionButton {
            objectName: "browse"
            theme: panel.theme
            implicitHeight: panel.theme.chipHeight
            text: "Open switcher"
            onClicked: panel.browseRequested()
          }
        }
        Text {
          id: galleryNote
          width: parent.width
          text: "Click a theme to wear it now. Its 󰖙 and 󰖔 choose the theme light and dark mode wear."
          textFormat: Text.PlainText
          wrapMode: Text.Wrap
          color: panel.theme.overlay
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textCaption
        }
      }

      // The gallery takes what the rest leaves of the panel's height, and never
      // less than a whole row of tiles; below that the panel scrolls.
      Shared.SeeleFlickable {
        id: gallery
        objectName: "gallery"
        theme: panel.theme
        width: parent.width
        height: Math.min(grid.height, Math.max(panel.tileHeight, panel.maximumHeight - settings.height - content.spacing - (effects.visible ? effects.height + content.spacing : 0)))
        contentWidth: width
        contentHeight: grid.height
        clip: true
        ScrollBar.vertical: Shared.SlimScrollBar { theme: panel.theme; popupHovered: panel.popupHovered }
        Grid {
          id: grid
          columns: panel.columns
          spacing: panel.theme.spaceMedium
          Repeater {
            model: panel.store.model
            delegate: GalleryCard {}
          }
        }
      }

      Column {
        id: effects
        width: parent.width
        visible: panel.theme.panelMotion || panel.theme.themeSounds || panel.store.actionError !== ""
        spacing: content.spacing

        // Only a theme that moves or sounds has anything to offer here, so the
        // presets' panel is as it was.
        Shared.SectionRule {
          theme: panel.theme
          width: parent.width
          visible: panel.theme.panelMotion || panel.theme.themeSounds
          label: "Effects"
        }
        EffectRow {
          objectName: "motion"
          visible: panel.theme.panelMotion
          title: "Motion"
          detail: "Panel trims sweep in and their glitch flickers"
          checked: panel.store.motion
          onToggled: panel.store.setEffect("motion", !checked)
        }
        EffectRow {
          objectName: "sound"
          visible: panel.theme.themeSounds
          title: "Sounds"
          detail: "Cues for toasts, panels, levels and the focus timer"
          checked: panel.store.sound
          onToggled: panel.store.setEffect("sound", !checked)
        }
        Text {
          width: parent.width
          visible: panel.store.actionError !== ""
          text: panel.store.actionError
          textFormat: Text.PlainText
          wrapMode: Text.Wrap
          color: panel.theme.red
          font.family: panel.theme.fontFamily
          font.pixelSize: panel.theme.textBody
        }
      }
    }
  }
}
