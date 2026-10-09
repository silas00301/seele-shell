import QtQuick
import "Motion.js" as Motion
import "Palette.js" as Palette
import Quickshell
import Quickshell.Io

// Shared by the desktop shell and standalone Seele applications.
ShellRoot {
  id: root

  // Seele's native desktop shell.
  property color base: Palette.fallback.base
  property color mantle: Palette.fallback.mantle
  // The darkest step in the palette. On a dark scheme it is the lowest surface
  // container, and on any scheme it is the ink drawn on a pale fill.
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
  // iOS-style privacy indicator colours, deliberately outside the theme palette.
  property color iosOrange: "#ff9f0a"
  property color iosGreen: "#30d158"
  property color iosRed: "#ff453a"
  // A light's colour temperature from warm (2700 K) through neutral to cool
  // (6500 K), drawn along the track that sets it. It is the light's own
  // colour, so like the privacy colours it stays outside the theme palette.
  readonly property var temperatureSpectrum: ["#ff9a45", "#fff1e2", "#9fc2ff"]
  property string wallpaper: Quickshell.env("SEELE_SHELL_WALLPAPER") || Palette.fallback.wallpaper

  // Shape. Seele follows Material 3 Expressive's corner scale, so a surface
  // picks the step for the role it plays rather than a pixel count: the
  // larger a container, the rounder its corners, and a control the pointer
  // aims at is a full pill, `height / 2`.
  readonly property int shapeExtraSmall: 4
  readonly property int shapeSmall: 8
  readonly property int shapeMedium: 12
  readonly property int shapeLarge: 16
  readonly property int shapeLargeIncreased: 20
  readonly property int shapeExtraLarge: 28
  // The roles those steps play. A floating panel takes the extra-large
  // corner a Material sheet does; a card inside it the large one; a row, a
  // field or a dropdown the medium one; a part nested inside an already
  // rounded part the small one.
  readonly property int radiusPanel: shapeExtraLarge
  readonly property int radius: shapeLarge
  readonly property int radiusRow: shapeMedium
  readonly property int radiusSmall: shapeSmall
  readonly property int barHeight: 30
  readonly property int barItemHeight: 22
  readonly property int barSpacing: 2
  readonly property int barPadding: 4
  readonly property int panelGap: 5
  // One spring for every scrollable. Qt's default overshoot drifts long enough
  // to read as lag rather than as feedback, so the flick decelerates hard and
  // the rebound is short.
  readonly property int scrollRebound: 130
  readonly property int scrollDeceleration: 9000
  readonly property int scrollFlickVelocity: 2200
  readonly property int osdGap: 16
  readonly property int panelMargin: 16
  readonly property int panelSpacing: 10
  readonly property int scrollGutter: 8
  readonly property int scrollInset: 4
  // A panel's header: its mark in an Expressive shape beside the title, and
  // a little taller when a line of context sits under that title.
  readonly property int panelHeaderHeight: 32
  readonly property int panelHeaderDetailHeight: 42
  readonly property int panelMarkSize: 32
  // The shape an empty state's mark sits in.
  readonly property int emptyMarkSize: 56
  // One type ramp for the whole shell. Steps are named for the role they play
  // rather than for their value, so a surface picks a level instead of
  // inventing a number, and the ramp is the only place a size is decided. A
  // glyph normally takes the step above the text it sits beside, because an
  // icon drawn at the same pixel size reads smaller than a letter does.
  readonly property int textMicro: 8       // a numeral riding beside a label
  readonly property int textCaption: 9     // row detail and secondary state
  readonly property int textLabel: 10      // button, chip and section labels
  readonly property int textBody: 11       // row titles and primary body text
  readonly property int textStrong: 12     // emphasised body, menu bar clock
  readonly property int textLead: 13       // a card's own subject
  readonly property int textIcon: 14       // menu bar and list-row glyphs
  readonly property int textSubhead: 15    // a card's lead subject
  readonly property int textCard: 17       // the glyph a card leads with
  readonly property int textTitle: 18      // panel titles
  readonly property int textDisplay: 20    // panel glyphs and hero numerals
  readonly property int textCode: 26       // a pairing code, read at arm's length
  readonly property int textHero: 34       // the single glyph a prompt leads with
  // Weight carries hierarchy instead of bolding everything: DemiBold for a
  // title or an active label, Medium for a quiet one, Light only for the large
  // numerals that would otherwise read as a wall.
  readonly property int weightRegular: Font.Normal
  readonly property int weightMedium: Font.Medium
  readonly property int weightStrong: Font.DemiBold
  readonly property int weightLight: Font.Light
  // Spacing ramp inside a card. Panels keep `panelMargin` and `panelSpacing`.
  readonly property int spaceTight: 4
  readonly property int hairline: 1
  readonly property int spaceSmall: 6
  readonly property int spaceMedium: 8
  readonly property int spaceLarge: 12
  readonly property int cardPadding: 10
  // The three heights a chip, a button and a list row take, so a panel keeps
  // its rhythm no matter which surface assembled it. A tile or a card still
  // sizes to what it holds.
  readonly property int chipHeight: 28
  readonly property int controlHeight: 34
  readonly property int rowHeight: 40
  // A labelled button is a pill padded this far either side of its label,
  // and a plain tooltip is this tall.
  readonly property int buttonPadding: 16
  readonly property int tooltipHeight: 24
  // Material's switch at the shell's density: its 52 by 32 track scaled to
  // sit in a row, with the outline and the handle's inset scaled with it.
  readonly property int switchWidth: 44
  readonly property int switchHeight: 26
  readonly property int switchOutline: 2
  readonly property int switchInset: 3
  // A meter is drawn this tall, and its filled part is held this far from
  // the rest of its track.
  readonly property int meterHeight: 6
  readonly property int meterGap: 3
  // Expressive's wavy progress: how far a crest reaches from the track, how
  // long one wave is, and how long one wave takes to flow past.
  readonly property int meterWaveAmplitude: 3
  readonly property int meterWavelength: 24
  readonly property int meterWavePeriod: 1600
  // The space between two choices in a connected button group.
  readonly property int segmentGap: 2
  // A row that leads with a mark and sets a caption line under its title, with
  // its own controls at the far end.
  readonly property int detailRowHeight: 52
  // A Control Center tile carries a label over its own detail line beside a
  // glyph, so it stands above the row ramp. Every tile in the grid takes it,
  // because a grid whose tiles disagree on height reads as a mistake.
  readonly property int controlTileHeight: 55
  // The round control a connectivity row, a tile and a level lead with. It is
  // one size wherever it appears, so the knob that switches Wi-Fi and the one
  // that mutes the output read as the same kind of thing, and a tile without a
  // knob keeps the column for its glyph so its title lines up beside one.
  readonly property int knobSize: 30
  // The square an application's own themed icon is drawn in where it leads a
  // row's title. It is sized against that title rather than against the row,
  // because it is standing in for the glyph that would otherwise be there.
  readonly property int rowIconSize: 16
  // The column a level's percentage is pinned to. Every track in a stack of
  // them takes it, because a right edge that moves with the number stops two
  // bars from being two readings of the same thing.
  readonly property int levelValueWidth: 46
  // A notification card is as tall as what it holds, but never shorter than
  // this: one line of summary over one line of body, beside the app icon. An
  // empty list is measured against it too, so "nothing here" costs one card.
  readonly property int notificationRowHeight: 54
  // Text needs more contrast than decorative borders and inactive glyphs.
  readonly property color mutedText: subtext

  // Colour roles. Material 3 names colours by what they do -- the container a
  // control sits on and the colour of what is drawn on it. Palette.js derives
  // every role from the palette above, so a preset, the theme switcher and
  // the auth clients repaint them all at once. QML reserves the `on` prefix
  // for signal handlers, so Material's `onPrimary` is `textOnPrimary` here,
  // and its `onSurface` and `onSurfaceVariant` are the palette's own `text`
  // and `subtext`.
  readonly property var roles: Palette.roles({
    base: base, mantle: mantle, crust: crust, surface: surface, overlay: overlay,
    text: text, subtext: subtext, accent: accent, red: red, green: green, yellow: yellow
  })
  readonly property bool darkScheme: roles.darkScheme
  readonly property color primary: roles.primary
  readonly property color textOnPrimary: roles.textOnPrimary
  readonly property color primaryContainer: roles.primaryContainer
  readonly property color textOnPrimaryContainer: roles.textOnPrimaryContainer
  readonly property color secondary: roles.secondary
  readonly property color secondaryContainer: roles.secondaryContainer
  readonly property color textOnSecondaryContainer: roles.textOnSecondaryContainer
  readonly property color error: roles.error
  readonly property color textOnError: roles.textOnError
  readonly property color errorContainer: roles.errorContainer
  readonly property color textOnErrorContainer: roles.textOnErrorContainer
  readonly property color successContainer: roles.successContainer
  readonly property color textOnSuccessContainer: roles.textOnSuccessContainer
  readonly property color warningContainer: roles.warningContainer
  readonly property color textOnWarningContainer: roles.textOnWarningContainer
  readonly property color outline: roles.outline
  readonly property color outlineVariant: roles.outlineVariant
  readonly property color inverseSurface: roles.inverseSurface
  readonly property color inverseOnSurface: roles.inverseOnSurface
  readonly property color scrim: roles.scrim
  readonly property color surfaceContainerLowest: roles.surfaceContainerLowest
  readonly property color surfaceContainerLow: roles.surfaceContainerLow
  readonly property color surfaceContainer: roles.surfaceContainer
  readonly property color surfaceContainerHigh: roles.surfaceContainerHigh
  readonly property color surfaceContainerHighest: roles.surfaceContainerHighest

  // Elevation is tonal. Material draws depth by stepping up the surface ramp
  // rather than by translucency, a texture or a lit edge, so a panel is a
  // solid container, a card on it is the next step up, a row inside that
  // card the step above again, and a track, a field or an unset switch the
  // highest step. `floatColor` is a control that overlaps a row whose own
  // fill moves under the pointer.
  readonly property color panelColor: surfaceContainer
  readonly property color cardColor: surfaceContainerHigh
  readonly property color rowColor: surfaceContainerHighest
  readonly property color wellColor: surfaceContainerHighest
  readonly property color floatColor: surfaceContainerHighest
  // Material containers carry no stroke. A floating panel is the exception
  // on a desktop that cannot cast a shadow under a layer surface: a hairline
  // of the outline variant is what keeps a dark panel from dissolving into a
  // dark window behind it. Cards and controls inside it take none.
  readonly property color panelBorder: alpha(outlineVariant, 0.9)
  readonly property color edgeLight: alpha(text, 0.1)
  readonly property color separatorColor: outlineVariant

  // State layers. Material reports hover, focus and press as a layer of the
  // content's own colour over whatever the control already says: 8% for the
  // pointer, 10% for a press, 16% for a drag. `hoverColor` is the layer;
  // filled controls composite it over their resting container instead of
  // replacing it.
  readonly property real stateHover: 0.08
  readonly property real statePressed: 0.1
  readonly property real stateDragged: 0.16
  readonly property color hoverColor: alpha(text, stateHover)
  // Focus is an outline in the secondary colour, held this far clear of the
  // control and drawn this thick.
  readonly property int focusGap: 2
  readonly property int focusWidth: 2
  // Where a tint rests on nothing at all it fades to its own colour at zero
  // alpha rather than to `transparent`. Qt interpolates a colour channel by
  // channel and `transparent` is black, so a tint animated against it is
  // dragged down through grey on the way in and back up through it on the way
  // out. The pill then reads as a smudge lifting off the strip instead of as
  // light arriving on it.
  readonly property color clearColor: alpha(text, 0)
  readonly property color clearDanger: alpha(red, 0)
  // A press replaces a resting fill where a surface has no layer of its own
  // to lay over it, so it is the card already carrying the pressed layer.
  readonly property color pressColor: Qt.tint(cardColor, alpha(text, statePressed + stateHover))
  readonly property color selectedColor: secondaryContainer
  readonly property color activeTint: primaryContainer
  readonly property color fillColor: primary
  readonly property color successColor: successContainer
  readonly property color dangerTint: errorContainer
  readonly property color dangerColor: Qt.tint(errorContainer, alpha(red, 0.18))
  readonly property color dangerPress: Qt.tint(errorContainer, alpha(red, 0.34))

  // Motion. Material 3 Expressive moves on springs: a spatial spring for
  // anything that travels, grows or changes shape, which may overshoot a
  // little and settle, and an effects spring for colour and opacity, which
  // never does. Qt animates on durations, so each spring is sampled into a
  // Bézier spline over its own settling time and handed to the animation as
  // its easing curve; Motion.js holds the arithmetic. The fast pair is the
  // shell's default, because a desktop answers a pointer sooner than a phone
  // answers a thumb; only the default spatial spring is slow enough for a
  // surface that unfolds. Each spring is Material's Expressive stiffness and
  // damping ratio.
  readonly property real fastSpatialStiffness: Motion.fastSpatialStiffness
  readonly property real fastSpatialDamping: Motion.fastSpatialDamping
  readonly property real defaultSpatialStiffness: Motion.defaultSpatialStiffness
  readonly property real defaultSpatialDamping: Motion.defaultSpatialDamping
  readonly property real fastEffectsStiffness: Motion.fastEffectsStiffness
  readonly property real fastEffectsDamping: Motion.fastEffectsDamping
  readonly property var springFastSpatial: springCurve(fastSpatialStiffness, fastSpatialDamping)
  readonly property var springDefaultSpatial: springCurve(defaultSpatialStiffness, defaultSpatialDamping)
  readonly property var springFastEffects: springCurve(fastEffectsStiffness, fastEffectsDamping)
  readonly property int durationFastSpatial: springDuration(fastSpatialStiffness, fastSpatialDamping)
  readonly property int durationDefaultSpatial: springDuration(defaultSpatialStiffness, defaultSpatialDamping)
  // `durationFast` is the fast effects spring: a tint or a fade. A finish cue
  // is the exception that has to be read without looking at the bar: it
  // comes in on `durationFast`, holds for `durationGlance`, and leaves on
  // `durationSettle`.
  readonly property int durationFast: springDuration(fastEffectsStiffness, fastEffectsDamping)
  readonly property int durationNormal: durationFastSpatial
  readonly property int durationGlance: 560
  readonly property int durationSettle: 480
  // How far a screen-edge finish cue reaches in from the bezel, as a fraction
  // of the shorter side, and how strong the Done colour is at that bezel. The
  // light falls off inside the rim, so the work in the middle stays put.
  readonly property real edgeCueReach: 0.1
  readonly property real edgeCueAlpha: 0.82
  // Disabled actions stay legible while clearly withdrawing interaction.
  readonly property real disabledOpacity: 0.45
  // A track the pointer has to hit is targeted taller than it is drawn, and
  // the handle that rides on it is Material's bar rather than a dot: a thin
  // upright held clear of the track on either side by `trackHandleGap`.
  readonly property int trackTarget: 20
  readonly property int trackHead: 4
  readonly property int trackHandleGap: 4
  // A level that is itself a card's subject -- the volume a Control Center
  // module sets -- is Material's extra-large slider: a track tall enough to
  // carry its own mark.
  readonly property int levelHeight: 40
  // The media block is one object at one size, so its height is decided here
  // rather than by whichever surface happens to be holding it.
  readonly property int mediaBodyHeight: 148
  // Wide enough for the block to set a title and an artist beside its art
  // without eliding either, and to carry the transport under them unsqueezed.
  readonly property int mediaPanelWidth: 400
  // Colour workbench: paired editors and a fixed contrast readout column.
  readonly property int colorLabWidth: 480
  readonly property int colorLabScoreWidth: 172
  readonly property int notesWindowWidth: 960
  readonly property int notesWindowHeight: 680
  // Small enough to be tiled into a column beside something else, which is
  // where a capture window spends most of its life. The recent-notes list
  // folds itself away before the writing area is squeezed to nothing.
  readonly property int notesMinimumWidth: 420
  readonly property int notesMinimumHeight: 420
  readonly property int notesSidebarWidth: 232
  // The recent-notes list can be dragged between these, and folds itself away
  // entirely below `notesNarrowWidth`, so a window tiled into a column keeps
  // its measure instead of splitting what is left of it in two.
  readonly property int notesSidebarMinimum: 180
  readonly property int notesSidebarMaximum: 420
  readonly property int notesNarrowWidth: 640
  readonly property int notesMemoListHeight: 112
  readonly property int notesPickerHeight: 220
  readonly property int waveformWidth: 280
  // The home panel stays compact, with a viewport bounded by its output.
  // Compact Hermes connection and explicit local rebuild approval.
  readonly property int hermesWidth: 380
  readonly property int homeAssistantWidth: 420
  readonly property int homeAssistantMaximumHeight: 640
  // The GitHub inbox scrolls inside this bound, tall enough that an opened
  // thread shows its analysis and the start of its conversation together.
  readonly property int githubInboxMaximumHeight: 520
  // The port inspector scrolls inside this bound, tall enough that an open
  // listener shows its address, its owners and a confirmation together.
  readonly property int portsMaximumHeight: 520
  // Resource charts and the process list share one output-bounded viewport.
  readonly property int resourcesWidth: 480
  readonly property int resourcesMaximumHeight: 720
  // Quick Look keeps media large enough to inspect while staying within the
  // focused output. Its transport shares one readable maximum measure.
  readonly property int quickLookWidth: 1100
  readonly property int quickLookHeight: 760
  readonly property int quickLookTimelineWidth: 420
  // The colour picker's lens magnifies this many captured pixels across: few
  // enough that each one is still a square the eye can aim at, many enough to
  // show which side of a one-pixel border the point is on. Its size is built
  // from the control ramp so the lens keeps the shell's rhythm instead of
  // arriving at a counted constant of its own.
  readonly property int colorLensCells: 9
  readonly property int colorLensSize: controlHeight * 4
  // Two readable editing columns for transient local text transforms.
  readonly property int textWorkbenchWidth: 760
  readonly property int textWorkbenchEditorHeight: 240
  // A duplex chart and two rate columns retain their measure on this panel.
  readonly property int networkActivityWidth: 480
  // A reading's label, its peak and limits, a state chip and its value share
  // one line in the Sensors panel without eliding a typical driver label.
  readonly property int sensorsWidth: 440
  // The calculator keeps its tape within one compact workbench.
  readonly property int calculatorWidth: 480
  readonly property int calculatorMaximumHeight: 620
  // The floating theme switcher stays inside this bound; on a shorter output
  // its centre card shrinks, keeping its shape, before anything is cut off.
  readonly property int themesMaximumHeight: 640
  // The floating carousel is wide enough for a 16:10 preview and three
  // slices on either side of it.
  readonly property int themesWidth: 1040
  // The Control Center's Themes panel: a three-way segment and two theme
  // rows with their names beside their swatches.
  readonly property int themeSettingsWidth: 440
  readonly property int clockWidth: 480
  readonly property int clockRows: 7
  readonly property int calendarMaximumHeight: 640
  readonly property int calendarIndicatorWidth: 190
  // Tall enough for the meeting readout, its suggestions, your calendar and
  // four zones before the zone list scrolls, without taking an output.
  readonly property int meetingMaximumHeight: 660
  // An hour ribbon: a two-digit hour set in `textMicro` with room to breathe.
  readonly property int meetingRibbonHeight: 18

  function alpha(color, opacity) {
    return Qt.rgba(color.r, color.g, color.b, opacity)
  }

  function layeredColor(base, tint) {
    var opacity = tint.a + base.a * (1 - tint.a)
    if (opacity <= 0) return Qt.rgba(0, 0, 0, 0)
    return Qt.rgba(
      (tint.r * tint.a + base.r * base.a * (1 - tint.a)) / opacity,
      (tint.g * tint.a + base.g * base.a * (1 - tint.a)) / opacity,
      (tint.b * tint.a + base.b * base.a * (1 - tint.a)) / opacity,
      opacity
    )
  }

  function hoveredColor(base) {
    return layeredColor(base, hoverColor)
  }

  // The palette's own ink that reads on `color`.
  function inkOn(color) {
    return Palette.inkOn(color, { base: base, crust: crust, text: text })
  }

  function springPosition(stiffness, dampingRatio, seconds) {
    return Motion.springPosition(stiffness, dampingRatio, seconds)
  }

  function springSettle(stiffness, dampingRatio) {
    return Motion.springSettle(stiffness, dampingRatio)
  }

  function springDuration(stiffness, dampingRatio) {
    return Motion.springDuration(stiffness, dampingRatio)
  }

  function springCurve(stiffness, dampingRatio) {
    return Motion.springCurve(stiffness, dampingRatio)
  }

  FileView {
    id: themeFile
    path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/seele-shell/theme.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: {
      try {
        var theme = JSON.parse(text())
        Palette.assign(root, theme)
      } catch (error) {
        console.warn("seele-shell/theme", error)
      }
    }
  }

  // The helper atomically replaces selection.json behind the Home Manager
  // symlink. A watcher can remain attached to the old inode after that rename.
  Timer {
    interval: 1000
    running: true
    repeat: true
    onTriggered: themeFile.reload()
  }

}
