import QtQuick
import "Palette.js" as Palette
import Quickshell
import Quickshell.Io

// Shared by the desktop shell and standalone Seele applications.
ShellRoot {
  id: root

  // Seele's native desktop shell.
  property color base: Palette.fallback.base
  property color mantle: Palette.fallback.mantle
  // The darkest step in the palette. Chrome is cut out of the wallpaper with
  // it, wells are cut back to it, and every surface is grounded on it, so the
  // shell's depth comes from ink rather than from grey.
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
  property string wallpaper: Quickshell.env("SEELE_SHELL_WALLPAPER") || Palette.fallback.wallpaper

  // Shared shape and surface tokens. Hyprland rounds windows at 8px, so every
  // panel, button, and bar entry rounds the same way, and each hover, press,
  // and selection tint is defined once instead of per widget.
  readonly property int radius: 8
  readonly property int radiusSmall: 6
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
  readonly property int panelHeaderHeight: 28
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
  // Uppercase section labels are the one place tracking earns its width, and
  // they earn more of it than a run of capitals at ordinary spacing would: a
  // rule reads as a rule, rather than as a shouted word, once the letters are
  // far enough apart to be seen individually.
  readonly property real trackingLabel: 1.4
  // Spacing ramp inside a card. Panels keep `panelMargin` and `panelSpacing`.
  readonly property int spaceTight: 4
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
  // A Control Center tile carries a label over its own detail line beside a
  // glyph, so it stands above the row ramp. Every tile in the grid takes it,
  // because a grid whose tiles disagree on height reads as a mistake.
  readonly property int controlTileHeight: 55
  // A notification card is as tall as what it holds, but never shorter than
  // this: one line of summary over one line of body, beside the app icon. An
  // empty list is measured against it too, so "nothing here" costs one card.
  readonly property int notificationRowHeight: 54
  // Text needs more contrast than decorative borders and inactive glyphs.
  readonly property color mutedText: subtext
  // Textured chrome. Surfaces stay translucent so the compositor's blur
  // shows through, a quiet vertical wash gives them depth, and a fixed grain
  // film keeps a large panel from reading as flat plastic. The film is fine
  // and clumped rather than raw noise, so it carries further before it is
  // seen: it is laid on a little heavier than a coarse one could be.
  readonly property string grain: Qt.resolvedUrl("grain.png")
  readonly property real grainOpacity: 0.07
  readonly property color panelColor: alpha(mantle, 0.88)
  // Edges. A surface is cut out of the wallpaper by a grounding ring in the
  // palette's darkest ink, and lit again on the inside by a hairline that is
  // brightest along the top, where light would actually land. Neither edge
  // carries the accent: outlining every panel in lavender spends the accent
  // on chrome, and it is worth more kept for state.
  readonly property color panelBorder: alpha(crust, 0.9)
  readonly property color edgeLight: alpha(text, 0.08)
  readonly property color edgeCrown: alpha(text, 0.16)
  // Interaction. The pointer is reported in neutral light and the commit is
  // reported in accent, so hovering the shell does not set it glowing and a
  // press still reads as something having been asked for. `hoverColor` is a
  // wash: filled controls composite it over their resting material instead of
  // replacing that material with a nearly transparent colour.
  readonly property color hoverColor: alpha(text, 0.07)
  // Where a tint rests on nothing at all it fades to its own colour at zero
  // alpha rather than to `transparent`. Qt interpolates a colour channel by
  // channel and `transparent` is black, so a tint animated against it is
  // dragged down through grey on the way in and back up through it on the way
  // out. The pill then reads as a smudge lifting off the strip instead of as
  // light arriving on it.
  readonly property color clearColor: alpha(text, 0)
  readonly property color clearDanger: alpha(red, 0)
  readonly property color pressColor: alpha(accent, 0.3)
  readonly property color selectedColor: alpha(accent, 0.2)
  readonly property color activeTint: alpha(accent, 0.12)
  readonly property color fillColor: alpha(accent, 0.45)
  readonly property color fillDanger: alpha(red, 0.45)
  readonly property color successColor: alpha(green, 0.25)
  readonly property color dangerTint: alpha(red, 0.14)
  readonly property color dangerColor: alpha(red, 0.28)
  readonly property color dangerPress: alpha(red, 0.48)
  // Elevation. A panel is translucent, so a card on it is a tint of the same
  // material rather than an opaque block, a row inside that card is a lighter
  // tint again, and a track or well is cut back to the ink. Depth then
  // comes from how much of the wallpaper each layer still lets through instead
  // of from a stack of flat greys. `floatColor` is the one nearly solid step,
  // for a control that overlaps a row whose own fill moves under the pointer.
  readonly property color cardColor: alpha(surface, 0.5)
  readonly property color rowColor: alpha(surface, 0.3)
  readonly property color wellColor: alpha(crust, 0.62)
  readonly property color floatColor: alpha(surface, 0.92)
  readonly property color cardBorder: alpha(text, 0.06)
  readonly property color separatorColor: alpha(text, 0.09)
  // Motion. Only in-surface state changes animate, and they share one pair of
  // durations so the whole shell settles at the same speed.
  readonly property int durationFast: 110
  readonly property int durationNormal: 180
  // The media block is one object at one size, so its height is decided here
  // rather than by whichever surface happens to be holding it.
  readonly property int mediaBodyHeight: 148
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
  readonly property int clockWidth: 480
  readonly property int clockRows: 7

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
        console.warn("seele-shell/theme", error)
      }
    }
  }

}
