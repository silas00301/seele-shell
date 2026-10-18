import QtQuick
import "Contract.js" as Contract
import "Palette.js" as Palette
import Quickshell
import Quickshell.Io

// Shared by the desktop shell and standalone Seele applications.
//
// This is the theme contract's one implementation; see THEME-CONTRACT.md.
// Every value the shell and Notes draw with is a property here, and
// theme-contract.json lists each one with its type, default and group, in the
// same order. A surface reads its colours, type, shape, spacing, sizes,
// materials, motion, sound and wallpaper from here and never writes a styling
// number or colour of its own; tests/theme-contract.js fails on one that does.
// A writable property is a slot a theme may supply, except the fixed signal
// colours; a readonly one is derived from the slots it names. A derived slot
// that reads `supplied` first is suppliable: a theme may give it outright.
ShellRoot {
  id: root

  // Palette ------------------------------------------------------------------
  // The eleven roles every theme supplies. `seele-theme` projects them from a
  // preset's Base16 palette; before any theme is published the shell draws
  // with the Palette.js fallback.
  property color base: Palette.fallback.base        // the ground the wallpaper sits on
  property color mantle: Palette.fallback.mantle    // panel material, translucent over the wallpaper
  // The darkest step in the palette. Chrome is cut out of the wallpaper with
  // it, wells are cut back to it, and every surface is grounded on it, so the
  // shell's depth comes from ink rather than from grey.
  property color crust: Palette.fallback.crust
  property color surface: Palette.fallback.surface  // cards and rows, as tints of it
  property color overlay: Palette.fallback.overlay  // quiet glyphs, rules and inactive marks
  property color text: Palette.fallback.text        // primary text and glyphs
  property color subtext: Palette.fallback.subtext  // secondary text, held to a legibility floor
  property color accent: Palette.fallback.accent    // state, selection and the one lit control
  property color red: Palette.fallback.red          // danger and failure
  property color green: Palette.fallback.green      // success and done
  property color yellow: Palette.fallback.yellow    // warnings and pending work

  // Signal colours -----------------------------------------------------------
  // iOS-style privacy indicator colours, deliberately outside the theme palette.
  property color iosOrange: "#ff9f0a"
  property color iosGreen: "#30d158"
  property color iosRed: "#ff453a"
  // A light's colour temperature from warm (2700 K) through neutral to cool
  // (6500 K), drawn along the track that sets it. It is the light's own
  // colour, so like the privacy colours it stays outside the theme palette.
  readonly property var temperatureSpectrum: ["#ff9a45", "#fff1e2", "#9fc2ff"]

  // Typography ---------------------------------------------------------------
  property string fontFamily: Palette.fallback.fontFamily  // the one typeface every surface sets text and glyphs in
  // One type ramp for the whole shell. Steps are named for the role they play
  // rather than for their value, so a surface picks a level instead of
  // inventing a number, and the ramp is the only place a size is decided. A
  // glyph normally takes the step above the text it sits beside, because an
  // icon drawn at the same pixel size reads smaller than a letter does.
  property int textMicro: 8       // a numeral riding beside a label
  property int textCaption: 9     // row detail and secondary state
  property int textLabel: 10      // button, chip and section labels
  property int textBody: 11       // row titles and primary body text
  property int textStrong: 12     // emphasised body, menu bar clock
  property int textLead: 13       // a card's own subject
  property int textIcon: 14       // menu bar and list-row glyphs
  property int textSubhead: 15    // a card's lead subject
  property int textCard: 17       // the glyph a card leads with
  property int textTitle: 18      // panel titles
  property int textDisplay: 20    // panel glyphs and hero numerals
  property int textCode: 26       // a pairing code, read at arm's length
  property int textHero: 34       // the single glyph a prompt leads with
  // Weight carries hierarchy instead of bolding everything: DemiBold for a
  // title or an active label, Medium for a quiet one, Light only for the large
  // numerals that would otherwise read as a wall.
  property int weightRegular: Font.Normal
  property int weightMedium: Font.Medium
  property int weightStrong: Font.DemiBold
  property int weightLight: Font.Light
  // Uppercase section labels are the one place tracking earns its width, and
  // they earn more of it than a run of capitals at ordinary spacing would: a
  // rule reads as a rule, rather than as a shouted word, once the letters are
  // far enough apart to be seen individually.
  property real trackingLabel: 1.4
  // A pairing code is spaced so each digit can be read off on its own.
  property int trackingCode: 4
  // A heading may be set in capitals, as signage is: a panel's title and a
  // section's label each take a capitalization, by default as written.
  property int titleCapitalization: Font.MixedCase
  property int labelCapitalization: Font.MixedCase
  // Prose that wraps over several lines is set a little open, and a glyph
  // beside wrapped text aligns to the box one line of it takes.
  property real lineHeightProse: 1.15
  property real lineBoxRatio: 1.4

  // Shape --------------------------------------------------------------------
  // Hyprland rounds windows at 8px, so every panel, button, and bar entry
  // rounds the same way.
  property int radius: 8
  property int radiusSmall: 6
  // The one stroke every border, rule and inner edge is drawn with.
  property int hairline: 1
  // An emphasised outline: the card a choice lands on, the meeting being placed.
  property int strokeStrong: 2
  // The meeting planner's ribbon cells and busy blocks round a little, and the
  // meeting being placed a little more.
  property int meetingCellRadius: 2
  property int meetingSelectionRadius: 3
  // A pressed button closes its corners to this, unless they are already
  // tighter, and opens them again when it is let go, on the spatial curve.
  // It is the resting corner unless a theme gives its own, so by default
  // nothing changes shape.
  readonly property int radiusPressed: supplied.radiusPressed ?? radius
  // Where the keyboard is: a ring `focusRingWidth` wide, `focusRingGap`
  // outside the control's edge, that flares to `focusRingFlare` as it appears
  // (`durationFocusRing`). A negative gap lays the ring inside the edge: by
  // default it is the hairline on the control's own edge, and it does not
  // flare.
  readonly property int focusRingWidth: supplied.focusRingWidth ?? hairline
  readonly property int focusRingGap: supplied.focusRingGap ?? -focusRingWidth
  readonly property int focusRingFlare: supplied.focusRingFlare ?? focusRingWidth
  // A panel may cut two corners instead of rounding them, as a machined
  // plate is cut: its top left and bottom right are taken off at 45 degrees,
  // this far along each edge. Zero keeps the rounded corner `radius` draws.
  property int panelCut: 0

  // Spacing ------------------------------------------------------------------
  property int barSpacing: 2     // between two menu bar entries
  property int barPadding: 4     // a bar entry's pill inside the bar's height
  property int panelGap: 5       // between the bar, a panel and an output's edge
  property int osdGap: 16        // between the bar and an OSD
  property int panelMargin: 16   // a panel's padding
  property int panelSpacing: 10  // between a panel's sections
  property int scrollGutter: 8   // the gutter a scrollable keeps for its handle
  property int scrollInset: 4    // content's distance from that gutter
  // Spacing ramp inside a card. Panels keep `panelMargin` and `panelSpacing`.
  property int spaceTight: 4
  property int spaceSmall: 6
  property int spaceMedium: 8
  property int spaceLarge: 12
  property int cardPadding: 10
  // Two lines of one item stacked under each other: a title over its detail.
  property int spaceLine: 1
  property int spaceMicro: 2     // a mark and its caption, a row of tiny marks
  property int spaceSnug: 3      // a row's title over its detail where they need air
  // Inside a bar entry: its row's gaps, and the padding on either side of a
  // label, so an entry's width is its content plus two of these.
  property int barContentSpacing: 5
  property int barItemPadding: 7
  // A card's glyph column is set this far from its text.
  property int cardGlyphSpacing: 9
  // A dialog's card: its padding on every side and the gap between its parts.
  property int dialogPadding: 22
  property int dialogSpacing: 13
  // The level OSD's padding around its glyph and meter.
  property int osdPadding: 14

  // Metrics ------------------------------------------------------------------
  property int barHeight: 30          // the menu bar
  property int barItemHeight: 22      // a bar entry's pill
  property int panelHeaderHeight: 28  // a panel header with a title and no detail
  // A cut panel's trim: the cut lit in `panelTrimColor`, and a bar
  // `strokeStrong` thick running `panelTrim` along the top edge from it.
  // Past its end, a glitch: a slice `panelGlitch` long knocked down off the
  // edge, in `panelGlitchColor`. Zero draws neither.
  property int panelTrim: 0
  property int panelGlitch: 0
  // How far the glitch jumps to either side while it flickers
  // (`durationPanelGlitch`).
  property int panelGlitchShift: 0
  // The three heights a chip, a button and a list row take, so a panel keeps
  // its rhythm no matter which surface assembled it. A tile or a card still
  // sizes to what it holds.
  property int chipHeight: 28
  property int controlHeight: 34
  property int rowHeight: 40
  // A row that leads with a mark and sets a caption line under its title, with
  // its own controls at the far end.
  property int detailRowHeight: 52
  // A Control Center tile carries a label over its own detail line beside a
  // glyph, so it stands above the row ramp. Every tile in the grid takes it,
  // because a grid whose tiles disagree on height reads as a mistake.
  property int controlTileHeight: 55
  // The round control a connectivity row, a tile and a level lead with. It is
  // one size wherever it appears, so the knob that switches Wi-Fi and the one
  // that mutes the output read as the same kind of thing, and a tile without a
  // knob keeps the column for its glyph so its title lines up beside one.
  property int knobSize: 30
  // A switch is a pill its knob rides inside, inset on every side.
  property int switchWidth: 40
  property int switchHeight: 22
  property int switchKnobInset: 3
  // The handle off, on and held: a switch may grow its handle as it is set
  // and again while it is pressed. By default the handle is one size, the
  // pill's height inside its inset.
  readonly property int switchHandleSize: supplied.switchHandleSize ?? switchHeight - switchKnobInset * 2
  readonly property int switchHandleOnSize: supplied.switchHandleOnSize ?? switchHandleSize
  readonly property int switchHandlePressedSize: supplied.switchHandlePressedSize ?? switchHandleOnSize
  // A round state layer centred on the handle reports the pointer and the
  // press, and the track then keeps its colour. Zero draws none: the track's
  // ring and its darkening report them instead.
  property int switchStateLayerSize: 0
  // The square an application's own themed icon is drawn in where it leads a
  // row's title. It is sized against that title rather than against the row,
  // because it is standing in for the glyph that would otherwise be there.
  property int rowIconSize: 16
  // The glyph column a card leads with.
  property int cardGlyphWidth: 24
  // The column a level's percentage is pinned to. Every track in a stack of
  // them takes it, because a right edge that moves with the number stops two
  // bars from being two readings of the same thing.
  property int levelValueWidth: 46
  // A notification card is as tall as what it holds, but never shorter than
  // this: one line of summary over one line of body, beside the app icon. An
  // empty list is measured against it too, so "nothing here" costs one card.
  property int notificationRowHeight: 54
  // A track the pointer has to hit is drawn thin and targeted tall: the strip
  // gives the meter inside it room to be grabbed without the meter itself
  // growing into a slab, and the head that rides on it rounds on its own size.
  property int trackTarget: 16
  property int trackHead: 10
  // The colour picker's lens magnifies this many captured pixels across: few
  // enough that each one is still a square the eye can aim at, many enough to
  // show which side of a one-pixel border the point is on. Its size is built
  // from the control ramp so the lens keeps the shell's rhythm instead of
  // arriving at a counted constant of its own.
  property int colorLensCells: 9
  readonly property int colorLensSize: controlHeight * 4
  // A panel header whose title carries a detail line under it stands taller,
  // and its mark sits in a well a step inside the chip height.
  property int panelHeaderDetailHeight: 42
  readonly property int panelHeaderMarkSize: chipHeight - 2
  // A section rule is at least this tall, and a collapsible one ends in a
  // chevron of this width, set a little below the label's baseline.
  property int sectionRuleHeight: 26
  property int sectionChevronWidth: 14
  property int sectionChevronDrop: 2
  // A meter is a thin pill; a segment well holds its segments this far in.
  property int meterHeight: 7
  property int segmentWellInset: 2
  // The scroll handle rides in the gutter, inset from both its ends, and is
  // never shorter than `scrollThumbMinimum` or than half a short track.
  property int scrollPadding: 2
  readonly property int scrollThumbWidth: scrollGutter - 4
  property int scrollThumbMinimum: 36
  property real scrollThumbShareLimit: 0.5
  // The activity spinner's square boxes, from a chip's to a header's.
  property int spinnerSmall: 14
  property int spinnerMedium: 16
  property int spinnerLarge: 18
  property int spinnerHeader: 20
  // Its stroke is a share of its size, but never thinner than the minimum.
  property real spinnerStrokeMinimum: 1.5
  property real spinnerStrokeRatio: 0.14
  // An empty state never sets its message wider than this.
  property int emptyStateWidth: 320
  // A glyph button that fits inside a chip row, and a control one text line tall.
  readonly property int compactButtonSize: chipHeight - 4
  readonly property int lineControlHeight: chipHeight - 6
  // The bar of an event's own colour that leads its agenda row.
  property int eventStripeWidth: 3
  // A theme preset's swatch: its base, carrying four dots of its own colours.
  property int swatchWidth: 58
  property int swatchHeight: 24
  property int swatchDotSize: 8
  // A schedule time field holds HH:MM.
  property int timeFieldWidth: 64
  // A hover tip: one line of label with this much room either side.
  property int tipPadding: 10
  property int tipHeight: 26
  // A harness or provider mark is rasterised this large before it is drawn
  // small, so a fine vector keeps its loops.
  property int markRasterSize: 64
  // Menu bar entries: an icon entry, a privacy dot's, the tray chevron's and
  // an agent badge's widths, and what each of them draws.
  property int barIconItemWidth: 30
  property int barDotItemWidth: 20
  property int barChevronItemWidth: 22
  property int barAgentItemWidth: 28
  property int barLogoSize: 20
  property int barIconSize: 16
  property int barWindowIconSize: 15
  property int barDotSize: 6
  property int privacyDotSize: 9
  property int barBadgeWidth: 26
  property int barBadgeHeight: 18
  property int barLabelMaximumWidth: 175
  property int barWindowTitleWidth: 230
  property int barWindowLabelWidth: 190
  property int barMediaWidth: 210
  // A workspace slot in the bar, wider while it is the active one.
  property int workspaceWidth: 22
  property int workspaceActiveWidth: 44
  // An agent badge: its mark, the state bar under it, and the dot an agent
  // row leads with.
  property int agentMarkSize: 13
  property int agentStateWidth: 14
  property int agentStateHeight: 3
  property int agentDotSize: 7
  // An agent row's title and detail lines.
  property int agentLineHeight: 16
  property int agentDetailLineHeight: 14
  // The usage table: its summary, its rows and its columns.
  property int usageSummaryHeight: 44
  property int usageRowHeight: 22
  property int usageDateWidth: 42
  property int usageCostWidth: 34
  property int usageTokensWidth: 52
  property int usageNameWidth: 132
  // A level row in the Audio panel and the mute button beside it; the Control
  // Center's audio sliders.
  property int levelRowHeight: 44
  property int audioSliderHeight: 46
  // A device list shows up to this many rows of this height before it
  // scrolls; Bluetooth and battery lists show more.
  property int deviceRowHeight: 28
  property int deviceListRows: 4
  property int bluetoothListRows: 6
  property int batteryListRows: 5
  property int networkAddressRows: 4
  // The width a device row reserves for the glyph that leads it.
  property int deviceRowGlyphReserve: 30
  // A wide action button under a card.
  property int actionButtonHeight: 38
  // Network panel: the status line, and the speed test card with its run
  // button and gauges.
  property int networkStatusLineHeight: 20
  property int speedtestCardHeight: 204
  property int speedtestHeaderHeight: 30
  property int speedtestButtonWidth: 64
  property int speedtestButtonHeight: 24
  property int speedtestGaugeHeight: 140
  // VPN panel cards, what their text columns leave for trailing controls,
  // and the app button.
  property int vpnCardHeight: 66
  property int sshCardHeight: 94
  property int tailscaleMenuReserve: 58
  property int tailscaleTextReserve: 82
  property int protonTextReserve: 123
  property int vpnAppButtonSize: 32
  // Bluetooth panel: the receiver row's glyph and the room its text leaves,
  // a device row's text and controls.
  property int bluetoothGlyphWidth: 18
  property int bluetoothReceiveReserve: 74
  property int bluetoothDeviceReserve: 62
  property int bluetoothSpinnerInset: 14
  property int bluetoothTrustWidth: 44
  property int bluetoothRowButtonHeight: 24
  property int bluetoothForgetSize: 24
  // A pairing code's box and a dialog's buttons.
  property int pairingCodeHeight: 52
  property int dialogButtonHeight: 30
  // Battery rows: their height, the name's room and the value column.
  property int batteryRowHeight: 38
  property int batteryNameReserve: 90
  property int batteryValueWidth: 60
  // The notification panel's Clear button.
  property int notificationClearWidth: 96
  // The media player picker and its popup.
  property int playerPickerWidth: 148
  property int playerPickerPopupWidth: 220
  // The calendar's Today button, the room its month list leaves the agenda,
  // and a day's disc and the marks under it.
  property int calendarTodayWidth: 84
  property int calendarMonthReserve: 32
  property int calendarDiscTop: 2
  readonly property int calendarDiscSize: chipHeight - 4
  // The tray menu: rows, separators, the icon in its header, the room its
  // title leaves the header's buttons, and those buttons.
  property int trayMenuRowHeight: 34
  property int trayMenuSeparatorHeight: 9
  property int trayMenuIconSize: 18
  property int trayMenuTitleReserve: 132
  property int trayHideWidth: 72
  property int trayHideHeight: 26
  property int trayCloseSize: 30
  // The NixOS session card's glyph column.
  property int sessionCardGlyphReserve: 46
  // A session action button.
  property int sessionButtonHeight: 72
  // The camera preview and its action buttons.
  property int cameraPreviewHeight: 176
  property int cameraActionHeight: 42
  // The level OSD's meter, and the glyph an OSD leads with.
  property int osdMeterWidth: 205
  property int osdMeterHeight: 8
  property int osdIconSize: 22
  property int osdHeight: 58
  // The speed gauge's dial track.
  property int gaugeTrackWidth: 6
  // A process's memory reads wider than a percentage.
  readonly property int memoryValueWidth: levelValueWidth * 1.5

  // Layout -------------------------------------------------------------------
  // The media block is one object at one size, so its height is decided here
  // rather than by whichever surface happens to be holding it.
  property int mediaBodyHeight: 148
  // Wide enough for the block to set a title and an artist beside its art
  // without eliding either, and to carry the transport under them unsqueezed.
  property int mediaPanelWidth: 400
  // Colour workbench: paired editors and a fixed contrast readout column.
  property int colorLabWidth: 480
  property int colorLabScoreWidth: 172
  // The Notes window opens at this size.
  property int notesWindowWidth: 960
  property int notesWindowHeight: 680
  // Small enough to be tiled into a column beside something else, which is
  // where a capture window spends most of its life. The recent-notes list
  // folds itself away before the writing area is squeezed to nothing.
  property int notesMinimumWidth: 420
  property int notesMinimumHeight: 420
  property int notesSidebarWidth: 232
  // The recent-notes list can be dragged between these, and folds itself away
  // entirely below `notesNarrowWidth`, so a window tiled into a column keeps
  // its measure instead of splitting what is left of it in two.
  property int notesSidebarMinimum: 180
  property int notesSidebarMaximum: 420
  property int notesNarrowWidth: 640
  property int notesMemoListHeight: 112
  property int notesPickerHeight: 220
  // The dictation waveform, across the foot of the output it records on.
  property int waveformWidth: 280
  // The dictation waveform draws this many bars across its width.
  property int waveformBars: 40
  // The home panel stays compact, with a viewport bounded by its output.
  // Compact Hermes connection and explicit local rebuild approval.
  property int hermesWidth: 380
  property int homeAssistantWidth: 420
  property int homeAssistantMaximumHeight: 640
  // The GitHub inbox scrolls inside this bound, tall enough that an opened
  // thread shows its analysis and the start of its conversation together.
  property int githubInboxMaximumHeight: 520
  // The port inspector scrolls inside this bound, tall enough that an open
  // listener shows its address, its owners and a confirmation together.
  property int portsMaximumHeight: 520
  // Resource charts and the process list share one output-bounded viewport.
  property int resourcesWidth: 480
  property int resourcesMaximumHeight: 720
  // Quick Look keeps media large enough to inspect while staying within the
  // focused output. Its transport shares one readable maximum measure.
  property int quickLookWidth: 1100
  property int quickLookHeight: 760
  property int quickLookTimelineWidth: 420
  // Two readable editing columns for transient local text transforms.
  property int textWorkbenchWidth: 760
  property int textWorkbenchEditorHeight: 240
  // A duplex chart and two rate columns retain their measure on this panel.
  property int networkActivityWidth: 480
  // A reading's label, its peak and limits, a state chip and its value share
  // one line in the Sensors panel without eliding a typical driver label.
  property int sensorsWidth: 440
  // The calculator keeps its tape within one compact workbench.
  property int calculatorWidth: 480
  property int calculatorMaximumHeight: 620
  // The floating theme switcher stays inside this bound; on a shorter output
  // its centre card shrinks, keeping its shape, before anything is cut off.
  property int themesMaximumHeight: 640
  // The floating carousel is wide enough for a 16:10 preview and three
  // slices on either side of it.
  property int themesWidth: 1040
  // The Control Center's Themes panel: a three-way segment over a gallery of
  // theme tiles three to a row.
  property int themeSettingsWidth: 520
  // The Themes panel stays inside this bound; its gallery scrolls within it.
  property int themeSettingsMaximumHeight: 760
  // The clock popup with its world clocks, the calendar popup, and the bar's
  // next-event entry.
  property int clockWidth: 480
  property int clockRows: 7
  property int calendarMaximumHeight: 640
  property int calendarIndicatorWidth: 190
  // Tall enough for the meeting readout, its suggestions, your calendar and
  // four zones before the zone list scrolls, without taking an output.
  property int meetingMaximumHeight: 660
  // An hour ribbon: a two-digit hour set in `textMicro` with room to breathe.
  property int meetingRibbonHeight: 18
  // Panels opened from the bar, by width.
  property int applicationWidth: 320
  property int caffeinateWidth: 320
  property int focusWidth: 350
  property int audioWidth: 350
  property int calendarWidth: 390
  property int networkWidth: 390
  property int vpnWidth: 390
  property int bluetoothWidth: 360
  property int cameraWidth: 360
  property int headphonesWidth: 340
  property int batteryWidth: 330
  property int notificationCenterWidth: 400
  property int sessionWidth: 420
  property int githubWidth: 440
  property int agentsWidth: 500
  property int trayMenuWidth: 310
  property int trayMenuMaximumHeight: 420
  // Toasts stack in a column of this width, up to this height.
  property int toastWidth: 400
  property int toastMaximumHeight: 430
  // A dialog such as the pairing prompt or the YubiKey touch OSD, and the
  // level OSD strip.
  property int dialogWidth: 360
  property int osdWidth: 300
  // A panel as tall as what it holds stops this far short of its output.
  property int panelScreenReserve: 60
  // A busy block's title is set just inside it. The meeting being placed
  // overhangs the ribbon, and the zone list leaves it room above and below.
  property int meetingBlockMinimumWidth: 2
  property int meetingBlockTextInset: 3
  property int meetingBlockTextTrailing: 2
  property int meetingSelectionOverhang: 1
  property int meetingSelectionMinimumWidth: 4
  property int meetingBandOverhang: 2
  property int meetingLegendWidth: 10
  property int meetingLegendHeight: 8
  // A busy block narrower than this leaves its title out.
  property int meetingBlockTitleMinimum: 24
  // A zone's name never takes more than this share of its row.
  property real meetingLabelShare: 0.4
  // The prompt: as wide and tall as an output allows up to these, with an
  // editing well, an attached image and the answer at fixed heights.
  property int promptWidth: 640
  property int promptMaximumHeight: 570
  property int promptEditorHeight: 112
  property int promptImageHeight: 112
  property int promptAnswerHeight: 190
  property int promptIdleHeight: 86
  // The theme switcher's centre card stays between these heights at 16:10;
  // a neighbour is a slice of its height and of that width.
  property int themesCardMinimum: 160
  property int themesCardMaximum: 300
  property real themesCardAspect: 1.6
  property real themesSliceWidthRatio: 0.19
  property real themesSliceHeightRatio: 0.86
  // The Themes panel's schedule source takes this share of its row.
  property real themeScheduleShare: 0.6
  // A Themes gallery tile is never narrower than this; a row holds as many
  // as fit, each this much wider than tall.
  property int themeGalleryCardMinimum: 140
  property real themeTileAspect: 1.25

  // Material -----------------------------------------------------------------
  // Text needs more contrast than decorative borders and inactive glyphs.
  readonly property color mutedText: subtext
  // Textured chrome. Surfaces stay translucent so the compositor's blur
  // shows through, a quiet vertical wash gives them depth, and a fixed grain
  // film keeps a large panel from reading as flat plastic. The film is fine
  // and clumped rather than raw noise, so it carries further before it is
  // seen: it is laid on a little heavier than a coarse one could be. A theme
  // may tile its own texture there instead (`surfaceTexture`), as a file URL
  // with its path segments escaped.
  readonly property string grain: surfaceTexture !== "" ? "file://" + surfaceTexture.split("/").map(encodeURIComponent).join("/") : Qt.resolvedUrl("grain.png")
  property real grainOpacity: 0.07
  // The film stops this far inside a surface's edge, so the hairline stays clean.
  property int grainInset: 3
  // On a rounded surface the film stops where the corner arc would cut it.
  readonly property real cornerGrainInset: radius * (1 - 1 / Math.sqrt(2))
  // The wash a surface carries: lit at its crown, fading out by
  // `washFadeStop`, clear from `washClearStop`, and grounded in ink at its foot.
  property real washCrownOpacity: 0.075
  property real washFadeOpacity: 0.02
  property real washFootOpacity: 0.5
  property real washFadeStop: 0.28
  property real washClearStop: 0.6
  readonly property color panelColor: supplied.panelColor ?? alpha(mantle, 0.88)  // a panel's material: mantle, translucent
  // Edges. A surface is cut out of the wallpaper by a grounding ring in the
  // palette's darkest ink, and lit again on the inside by a hairline that is
  // brightest along the top, where light would actually land. Neither edge
  // carries the accent: outlining every panel in lavender spends the accent
  // on chrome, and it is worth more kept for state.
  readonly property color panelBorder: supplied.panelBorder ?? alpha(crust, 0.9)
  readonly property color edgeLight: supplied.edgeLight ?? alpha(text, 0.08)
  readonly property color edgeCrown: supplied.edgeCrown ?? alpha(text, 0.16)
  // The trim and the glitch a cut panel carries (`panelTrim`): the accent,
  // and red knocked out of register past it.
  readonly property color panelTrimColor: supplied.panelTrimColor ?? accent
  readonly property color panelGlitchColor: supplied.panelGlitchColor ?? red
  // Interaction. The pointer is reported in neutral light and the commit is
  // reported in accent, so hovering the shell does not set it glowing and a
  // press still reads as something having been asked for. `hoverColor` is a
  // wash: filled controls composite it over their resting material instead of
  // replacing that material with a nearly transparent colour.
  readonly property color hoverColor: supplied.hoverColor ?? alpha(text, 0.07)
  // Where a tint rests on nothing at all it fades to its own colour at zero
  // alpha rather than to `transparent`. Qt interpolates a colour channel by
  // channel and `transparent` is black, so a tint animated against it is
  // dragged down through grey on the way in and back up through it on the way
  // out. The pill then reads as a smudge lifting off the strip instead of as
  // light arriving on it.
  readonly property color clearColor: supplied.clearColor ?? alpha(text, 0)
  readonly property color clearDanger: supplied.clearDanger ?? alpha(red, 0)
  readonly property color pressColor: supplied.pressColor ?? alpha(accent, 0.3)
  readonly property color selectedColor: supplied.selectedColor ?? alpha(accent, 0.2)
  readonly property color activeTint: supplied.activeTint ?? alpha(accent, 0.12)
  readonly property color fillColor: supplied.fillColor ?? alpha(accent, 0.45)
  readonly property color fillDanger: supplied.fillDanger ?? alpha(red, 0.45)
  readonly property color successColor: supplied.successColor ?? alpha(green, 0.25)
  readonly property color dangerTint: supplied.dangerTint ?? alpha(red, 0.14)
  readonly property color dangerColor: supplied.dangerColor ?? alpha(red, 0.28)
  readonly property color dangerPress: supplied.dangerPress ?? alpha(red, 0.48)
  // Elevation. A panel is translucent, so a card on it is a tint of the same
  // material rather than an opaque block, a row inside that card is a lighter
  // tint again, and a track or well is cut back to the ink. Depth then
  // comes from how much of the wallpaper each layer still lets through instead
  // of from a stack of flat greys. `floatColor` is the one nearly solid step,
  // for a control that overlaps a row whose own fill moves under the pointer.
  readonly property color cardColor: supplied.cardColor ?? alpha(surface, 0.5)
  readonly property color rowColor: supplied.rowColor ?? alpha(surface, 0.3)
  readonly property color wellColor: supplied.wellColor ?? alpha(crust, 0.62)
  readonly property color floatColor: supplied.floatColor ?? alpha(surface, 0.92)
  readonly property color cardBorder: supplied.cardBorder ?? alpha(text, 0.06)
  readonly property color separatorColor: supplied.separatorColor ?? alpha(text, 0.09)
  // The faint ring a well, a meter or a slider track is cut with.
  readonly property color wellBorder: supplied.wellBorder ?? alpha(text, 0.05)
  // The faintest wash of text: where a click would put something.
  readonly property color quietWash: supplied.quietWash ?? alpha(text, 0.05)
  // A count badge, such as a notification stack's.
  readonly property color badgeColor: supplied.badgeColor ?? alpha(text, 0.09)
  // A round control's resting fill, cut into the material.
  readonly property color knobColor: supplied.knobColor ?? alpha(crust, 0.7)
  // The primary transport control's resting fill.
  readonly property color primaryControlColor: supplied.primaryControlColor ?? alpha(accent, 0.22)
  // A ring that says something is open or suggested.
  readonly property color accentRing: supplied.accentRing ?? alpha(accent, 0.55)
  // A selected day that is not today is ringed at half the accent.
  readonly property color selectedRing: supplied.selectedRing ?? alpha(accent, 0.5)
  // A card that stands for something deliberate is edged in the accent; one
  // that destroys something is edged in red.
  readonly property color accentEdge: supplied.accentEdge ?? alpha(accent, 0.28)
  readonly property color dangerBorder: supplied.dangerBorder ?? alpha(red, 0.22)
  // The menu bar strip is ink over the wallpaper, ruled off along its foot.
  readonly property color barStripColor: supplied.barStripColor ?? alpha(crust, 0.86)
  readonly property color barRuleColor: supplied.barRuleColor ?? alpha(crust, 0.85)
  // Workspace pills: under the pointer, occupied, empty, and an empty one's
  // number.
  readonly property color workspaceHoverColor: supplied.workspaceHoverColor ?? alpha(accent, 0.5)
  readonly property color workspaceOccupiedColor: supplied.workspaceOccupiedColor ?? alpha(text, 0.16)
  readonly property color workspaceEmptyColor: supplied.workspaceEmptyColor ?? alpha(text, 0.07)
  readonly property color workspaceEmptyText: supplied.workspaceEmptyText ?? alpha(text, 0.45)
  // The muted microphone badge.
  readonly property color mutedBadgeColor: supplied.mutedBadgeColor ?? alpha(overlay, 0.35)
  // The tinted well a panel's or a finding's mark sits in.
  readonly property color markWell: supplied.markWell ?? alpha(accent, 0.1)
  readonly property color markWellBorder: supplied.markWellBorder ?? alpha(accent, 0.22)
  // A swatch is ringed in faint text so a base close to the panel still reads.
  readonly property color swatchBorder: supplied.swatchBorder ?? alpha(text, 0.14)
  // A neighbour in the theme switcher is ringed fainter than the centre.
  readonly property color themesSliceBorder: supplied.themesSliceBorder ?? alpha(text, 0.12)
  // A field whose last attempt failed is ringed in a soft warning.
  readonly property color warningBorder: supplied.warningBorder ?? alpha(yellow, 0.3)
  // Disabled actions stay legible while clearly withdrawing interaction.
  property real disabledOpacity: 0.45
  // Present but unusable now -- a switch that cannot be set, a device that is
  // gone -- withdraws a little further than a disabled action.
  property real unavailableOpacity: 0.42
  // A status chip, banner or active icon button is a wash of its own state
  // colour, ringed by a stronger one.
  property real statusFillOpacity: 0.14
  property real statusBorderOpacity: 0.24
  // A maintenance finding's mark is a lighter wash of its urgency colour.
  property real findingFillOpacity: 0.12
  // A switch track darkens to this while pressed, rings in the accent at
  // `switchHoverOpacity` under the pointer, and its knob is text at
  // `switchKnobOpacity` while off.
  property real switchPressedOpacity: 0.6
  property real switchHoverOpacity: 0.55
  property real switchKnobOpacity: 0.82
  // The handle off and on, at rest and under the pointer or held.
  readonly property color switchHandleOff: supplied.switchHandleOff ?? alpha(text, switchKnobOpacity)
  readonly property color switchHandleOffActive: supplied.switchHandleOffActive ?? switchHandleOff
  readonly property color switchHandleOn: supplied.switchHandleOn ?? crust
  readonly property color switchHandleOnActive: supplied.switchHandleOnActive ?? switchHandleOn
  // The pointer over a control that is on, such as a set switch's handle.
  readonly property color selectedHoverColor: supplied.selectedHoverColor ?? hoverColor
  // The ring that says where the keyboard is.
  readonly property color focusRingColor: supplied.focusRingColor ?? accent
  // A press ripple (`durationRipple`) starts as a disc `rippleOriginScale` of
  // the control's longer side where the press landed, and spreads to the
  // centre until it passes the far corner by `ripplePadding` and a soft edge
  // of `rippleSoftEdge`, or of `rippleSoftEdgeRatio` of the longer side where
  // that is more. Its rim fades out over `rippleFeather`, while at least
  // `rippleCore` of its radius stays solid. It is drawn in `pressColor`.
  property real rippleOriginScale: 0.2
  property int ripplePadding: 10
  property int rippleSoftEdge: 75
  property real rippleSoftEdgeRatio: 0.35
  property int rippleFeather: 70
  property real rippleCore: 0.65
  // The scroll handle at rest and while held.
  property real scrollThumbOpacity: 0.32
  property real scrollThumbPressedOpacity: 0.6
  // A meter's fill rises from this strength to full along its length.
  property real meterFadeOpacity: 0.62
  // A colour track shows its whole range faintly and its fill more strongly.
  property real spectrumWellOpacity: 0.28
  property real spectrumFillOpacity: 0.62
  // Waveform bars rise from the floor strength, oldest, to full, newest; the
  // two always add up to one.
  property real waveformFloorOpacity: 0.35
  property real waveformRiseOpacity: 0.65
  // An agenda row that has ended: its text, and its colour stripe.
  property real pastOpacity: 0.55
  property real pastMarkOpacity: 0.45
  // A calendar that is not shown keeps its colour at half strength.
  property real deselectedOpacity: 0.5
  // The meeting planner: working and edge hours as accent washes, busy time
  // in the calendar's own colour, the past veiled in ink, and the column a
  // click would place the meeting in ringed in the accent.
  property real meetingWorkOpacity: 0.34
  property real meetingEdgeOpacity: 0.13
  property real meetingBusyOpacity: 0.72
  property real meetingPastOpacity: 0.45
  property real meetingHoverBorderOpacity: 0.4
  // The theme switcher veils its neighbours in ink, lighter under the pointer.
  property real themesVeilOpacity: 0.24
  property real themesVeilHoverOpacity: 0.06
  // A Themes gallery tile's name sits on its theme's mantle at this strength,
  // and a mode pin it does not wear on that mantle at this one.
  property real themeTileShadeOpacity: 0.85
  property real themeTilePinOpacity: 0.6
  // A finish cue falls from the bezel to this share of its strength at the
  // falloff stop, measured from the bezel on either side of the output.
  property real edgeCueFalloff: 0.42
  property real edgeCueFalloffStop: 0.45
  property real edgeCueFalloffStopMirrored: 0.55
  // A bar entry being dragged off the bar, paused media art, and a tray item
  // the user hid.
  property real dragAwayOpacity: 0.4
  property real pausedOpacity: 0.4
  property real trayHiddenOpacity: 0.45
  // A level the player will not let us set.
  property real readOnlyOpacity: 0.35
  // A screen hint whose number no longer matches what is typed.
  property real unmatchedOpacity: 0.25
  // An agent that is running quietly, and the low point of a busy one's pulse.
  property real agentRunningOpacity: 0.4
  property real agentPulseOpacity: 0.25
  // The speed gauge's dial track and its ticks.
  property real gaugeTrackOpacity: 0.28
  property real gaugeTickOpacity: 0.34
  // How far a screen-edge finish cue reaches in from the bezel, as a fraction
  // of the shorter side, and how strong the Done colour is at that bezel. The
  // light falls off inside the rim, so the work in the middle stays put.
  property real edgeCueReach: 0.1
  property real edgeCueAlpha: 0.82

  // Motion -------------------------------------------------------------------
  // Only in-surface state changes animate, and they share one pair of
  // durations so the whole shell settles at the same speed. A finish cue is
  // the exception that has to be read without looking at the bar: it comes in
  // on `durationFast`, holds for `durationGlance`, and leaves on
  // `durationSettle`.
  property int durationFast: 110
  property int durationNormal: 180
  property int durationGlance: 560
  property int durationSettle: 480
  // One spring for every scrollable. Qt's default overshoot drifts long enough
  // to read as lag rather than as feedback, so the flick decelerates hard and
  // the rebound is short.
  property int scrollRebound: 130
  property int scrollDeceleration: 9000
  property int scrollFlickVelocity: 2200
  // The one curve every eased transition takes.
  property int easingStandard: Easing.OutCubic
  // The Bézier spline the standard curve follows when its easing is BezierSpline, as
  // Qt's easing.bezierCurve reads it; empty while it is a named curve.
  property var easingStandardCurve: []
  // A cue leaving the screen accelerates away instead of settling.
  property int easingExit: Easing.InCubic
  // The Bézier spline the exit curve follows when its easing is BezierSpline, as
  // Qt's easing.bezierCurve reads it; empty while it is a named curve.
  property var easingExitCurve: []
  // One turn of the activity spinner.
  property int durationSpin: 720
  // A gauge's needle sweeps to a new reading; a busy agent pulses, faster
  // while it waits for input.
  property int durationGauge: 520
  property int durationPulse: 900
  property int durationPulseFast: 600
  property int easingPulse: Easing.InOutQuad
  // The Bézier spline the pulse curve follows when its easing is BezierSpline, as
  // Qt's easing.bezierCurve reads it; empty while it is a named curve.
  property var easingPulseCurve: []
  // A state layer -- the pointer over a control -- comes and goes over this.
  readonly property int durationState: supplied.durationState ?? durationFast
  // A control's own colour changing with its state, set, selected or held,
  // takes `durationEffects` on `easingEffects`. A theme may give a spring
  // that never overshoots, sampled into the curve.
  readonly property int durationEffects: supplied.durationEffects ?? durationFast
  property int easingEffects: Easing.Linear
  // The Bézier spline the effects curve follows when its easing is BezierSpline, as
  // Qt's easing.bezierCurve reads it; empty while it is a named curve.
  property var easingEffectsCurve: []
  // Part of a control moving or changing shape -- a switch's handle, a
  // pressed button's corners -- takes `durationSpatial` on `easingSpatial`,
  // by default the standard curve at the fast duration. A theme may give a
  // spring, sampled into the curve, which may overshoot and settle.
  readonly property int durationSpatial: supplied.durationSpatial ?? durationFast
  readonly property int easingSpatial: supplied.easingSpatial ?? easingStandard
  // The Bézier spline the spatial curve follows when its easing is BezierSpline, as
  // Qt's easing.bezierCurve reads it; empty while it is a named curve.
  readonly property var easingSpatialCurve: supplied.easingSpatialCurve ?? easingStandardCurve
  // A press grows a ripple over `durationRipple` on the standard curve, fading
  // it in over `durationRippleIn`; it stays at least `durationRippleHold` from
  // the press and fades out over `durationRippleOut`. Zero grows none, and a
  // press lays its colour flat over the control instead.
  property int durationRipple: 0
  property int durationRippleIn: 105
  property int durationRippleHold: 225
  property int durationRippleOut: 375
  // The focus ring flares as it appears: it grows to `focusRingFlare` in the
  // first quarter of this and settles to its width in the rest, on the
  // standard curve. Zero shows it at once.
  property int durationFocusRing: 0
  // A cut panel's own motion. As the panel appears, its trim sweeps out from
  // the cut over `durationPanelTrim` on the standard curve; then its glitch
  // flickers `panelGlitchFlickers` times over `durationPanelGlitch`, jumping
  // `panelGlitchShift` to either side, and flickers again when the pointer
  // comes onto the panel. Each comes to rest where the still frame draws it.
  // Zero keeps it still, and the user may hold all of it still.
  property int durationPanelTrim: 0
  property int durationPanelGlitch: 0
  property int panelGlitchFlickers: 0

  // Sound --------------------------------------------------------------------
  // The cues a theme may give a sound to, each an absolute path to a file the
  // shell can play. Every one is optional and an empty path is silence, which
  // is the fallback: no surface plays a sound unless the theme supplies one.
  property string soundNotification: ""      // a notification arrives
  property string soundNotificationUrgent: "" // a critical notification arrives
  property string soundFocusDone: ""         // the focus timer runs out
  property string soundPanelOpen: ""         // a panel opens
  property string soundPanelClose: ""        // a panel closes
  property string soundLevelStep: ""         // a level OSD steps

  // Wallpaper ----------------------------------------------------------------
  // A theme may carry its own image, as an absolute path. The publication
  // names the one the host's wallpaper mapping chooses: an image mapped to the
  // theme, else its own, else the one mapped to its light or dark mode, else
  // the configured wallpaper, so switching to a theme without an image keeps
  // the one on screen. Before any theme is published the shell draws the
  // session's SEELE_SHELL_WALLPAPER, else the system image. The background is
  // drawn in `base` under any of them, so a missing file still leaves the
  // palette.
  property string wallpaper: ""
  readonly property string configuredWallpaper: Quickshell.env("SEELE_SHELL_WALLPAPER") || Palette.fallback.wallpaper
  readonly property string wallpaperImage: wallpaper !== "" ? wallpaper : configuredWallpaper
  // The outputs that draw another image than `wallpaper`, by the name the
  // compositor gives each one, such as DP-1. The host's wallpaper mapping
  // chooses them, per output and per light or dark theme, and the publication
  // names only outputs whose image differs; every other output, and every
  // output when the mapping names none, draws `wallpaperImage`.
  property var wallpaperOutputs: ({})

  // Assets -------------------------------------------------------------------
  // Files a theme carries for its look, each an absolute path. The typeface
  // a theme names in `fontFamily` may come with it, one file for each weight
  // the theme sets text in, so the theme draws the same on any machine; the
  // shell loads every file it is given. An empty slot loads nothing, which is
  // the fallback: `fontFamily` then names a typeface installed on the system.
  property string fontFileLight: ""    // the face `weightLight` sets
  property string fontFileRegular: ""  // the face `weightRegular` sets
  property string fontFileMedium: ""   // the face `weightMedium` sets
  property string fontFileStrong: ""   // the face `weightStrong` sets
  // An image tiled over every surface at `grainOpacity` in place of the
  // grain film, such as scanlines. Empty keeps the film.
  property string surfaceTexture: ""

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

  // The image the output named `output` draws: its own from
  // `wallpaperOutputs`, else `wallpaperImage`.
  function wallpaperFor(output) {
    return Palette.imageFor(wallpaperOutputs, output, wallpaperImage)
  }

  // Not a styling value: the shared keyboard navigation every surface reaches
  // through the theme. The contract lists it as a member rather than a slot.
  property KeyboardNavigation keyboardNavigation: KeyboardNavigation { theme: root }

  // The derived slots the published theme supplies itself, by slot. Each
  // such slot reads its value here first and falls back to its derivation,
  // so a theme that supplies none draws exactly the derivation.
  property var supplied: ({})

  // What the user lets a theme do, which is not the theme's to say: whether
  // its own motion plays and whether its sounds do. They come from the theme
  // helper's preferences (`seele-theme motion|sound on|off`). A theme moves
  // unless held still, and sounds only once asked to.
  property bool motionEnabled: true
  property bool soundEnabled: false
  // Whether the published theme has motion or sound of its own to let
  // through, so a choice about them is offered only for a theme that has
  // them.
  readonly property bool panelMotion: panelCut > 0 && panelTrim > 0
    && (durationPanelTrim > 0 || (panelGlitch > 0 && durationPanelGlitch > 0))
  readonly property bool themeSounds: [soundNotification, soundNotificationUrgent, soundFocusDone,
    soundPanelOpen, soundPanelClose, soundLevelStep].some(function(file) { return file !== "" })

  // A published theme sets every slot a theme may supply: its own value, or
  // the default where it leaves one out, so nothing an earlier theme set
  // outlives it. `slots` is Contract.slots of theme-contract.json.
  function applyTheme(theme, slots) {
    var values = Contract.values(slots, theme)
    for (var i = 0; i < slots.length; i++) {
      var slot = slots[i], value = values[slot.name]
      if (slot.derived) continue
      root[slot.name] = slot.type === "weight" || slot.type === "capitalization" ? Font[value] : slot.type === "easing" ? Easing[value] : value
    }
    var supplied = Contract.supplied(slots, theme)
    for (var j = 0; j < slots.length; j++) {
      if (slots[j].type === "easing" && supplied[slots[j].name] !== undefined) supplied[slots[j].name] = Easing[supplied[slots[j].name]]
    }
    root.supplied = supplied
  }

  // The typeface files the theme carries. Only a slot that names a file
  // gets a loader, so a theme without any loads nothing, and switching away
  // from one never asks Qt to load an empty source. Each path is given as a
  // file URL with its segments escaped, as the wallpaper's is, so a `%`, `#`
  // or `?` in it names the file rather than a URL part.
  Instantiator {
    model: [root.fontFileLight, root.fontFileRegular, root.fontFileMedium, root.fontFileStrong].filter(function(file) { return file !== "" })
    delegate: FontLoader {
      required property string modelData
      source: "file://" + modelData.split("/").map(encodeURIComponent).join("/")
    }
  }

  // The contract beside this file, read before any window maps. FileView takes
  // a filesystem path and only drops a `file://` scheme, while a resolved URL
  // still escapes `%`, `#` and `?`, so the path is decoded here.
  FileView {
    id: contractFile
    path: decodeURIComponent(Qt.resolvedUrl("theme-contract.json").toString().replace(/^file:\/\//, ""))
    blockLoading: true
    printErrors: false
    readonly property var slots: {
      try {
        return Contract.slots(JSON.parse(text()))
      } catch (error) {
        return null
      }
    }
  }

  FileView {
    id: themeFile
    path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/seele-shell/theme.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    // The text last applied: the reload below rereads an unchanged file.
    property string applied: ""
    onLoaded: {
      try {
        var source = text()
        if (source === applied) return
        var theme = JSON.parse(source)
        if (contractFile.slots && contractFile.slots.length > 0) {
          root.applyTheme(theme, contractFile.slots)
        } else {
          // Without the contract only the palette and typeface can be applied.
          console.warn("seele-shell/theme", "theme-contract.json is unavailable; applying the palette only")
          Palette.assign(root, theme)
        }
        applied = source
      } catch (error) {
        console.warn("seele-shell/theme", error)
      }
    }
  }

  // The theme helper's preferences: whether the theme's motion and sounds
  // are let through. A preference the file does not carry, or a file that
  // cannot be read, keeps the default.
  FileView {
    id: preferencesFile
    path: (Quickshell.env("SEELE_THEME_STATE")
      || (Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state") + "/seele-theme") + "/preferences.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    // The text last read: the reload below rereads an unchanged file.
    property string applied: ""
    onLoaded: {
      var source = text()
      if (source === applied) return
      applied = source
      try {
        var preferences = JSON.parse(source)
        root.motionEnabled = preferences.motion !== false
        root.soundEnabled = preferences.sound === true
      } catch (error) {
        root.motionEnabled = true
        root.soundEnabled = false
      }
    }
    // No preferences yet are the defaults.
    onLoadFailed: {
      applied = ""
      root.motionEnabled = true
      root.soundEnabled = false
    }
  }

  // The helper atomically replaces selection.json behind the Home Manager
  // symlink, and preferences.json in place. A watcher can remain attached to
  // the old inode after that rename.
  Timer {
    interval: 1000
    running: true
    repeat: true
    onTriggered: {
      themeFile.reload()
      preferencesFile.reload()
    }
  }

}
