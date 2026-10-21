# Theme contract

One contract names every styling value the desktop shell and Seele Notes draw
with: colours, typography, shape, spacing, sizes, materials, motion, sound,
wallpaper and the files a theme carries for its look. A surface reads all of them from `Shared.Theme` and writes none of
its own, so a theme can restyle the shell without a code change.

| File | Role |
| --- | --- |
| `theme-contract.json` | The schema: every slot with its group, type, default or derivation and description; the rules a theme document follows; the fallbacks; what counts as structure rather than style. |
| `Theme.qml` | The one implementation. Every slot is a property here, in the same order and with the same default. Its `applyTheme` sets every suppliable slot from a published `theme.json`. |
| `Contract.js` | Reads the slots a theme may supply out of the JSON at run time and decides each one between the theme's value and the default. |
| `tests/theme-contract.js` | Holds the two to each other, validates theme documents, and fails on any styling literal outside the contract. |
| `../config-tools/src/theme_contract.rs` | The same document rules in the theme helper, which embeds this JSON and refuses a preset that breaks them. |
| `../config-tools/src/theme_folder.rs` | Registers a drop-in theme folder at build time, and holds a registered document to the same rules whenever the catalog is read. |
| `../../themes/` | The drop-in theme folders, one line each in `default.nix`; `AUTHORING.md` writes a new one from `template/`. |
| `tests/theme-snapshots/` | Pixel baselines of every preset (SIL-232). A refactor of the contract proves it changed nothing there. |

## Slots

456 slots in twelve groups, each a `// <Section> ----` header in `Theme.qml`:

| Group | What it holds | Kind |
| --- | --- | --- |
| `palette` | The eleven colour roles. | required |
| `signal` | The privacy indicator colours and a light's colour temperature, which belong to what they report. | fixed |
| `typography` | Typeface, type ramp, weights, tracking, line spacing. | |
| `shape` | Corner radii and strokes. | |
| `spacing` | The spacing ramp and the gaps between bar, panels and output. | |
| `metrics` | Sizes of controls, rows, marks and the parts of each surface. | |
| `layout` | Extents of panels, windows and their larger blocks. | |
| `material` | Translucent materials, edges, interaction tints, elevation, texture, withdrawal opacities. | |
| `motion` | Durations and easing curves. | |
| `sound` | Optional cues. | optional |
| `wallpaper` | An optional image of the theme's own, and the images the host maps per output. | optional |
| `assets` | Optional files a theme carries for its look: its typeface, one file per weight, and a texture tiled over its surfaces. | optional |

A writable property is a slot a theme may supply. A readonly one is *derived*:
computed from the slots it names, such as `panelColor: alpha(mantle, 0.88)`, so
a theme changes it through those slots. Every default reproduces the shell as it
was drawn before themes could set more than colours.

A derived slot may also be *suppliable*: a theme may give its own value, of the
slot's type, in place of the derivation, as a Material theme gives its opaque
tonal surfaces, its pressed corner and its springs. Theme.qml reads such a slot
as `supplied.<name> ?? <derivation>`, where `supplied` holds the derived slots
the published theme gave, so a theme that gives none draws exactly the
derivation, and the next theme that leaves one out gets it back. Only a colour,
an `int`, a `real`, an easing or a curve of the shape, metrics, material and
motion groups can be suppliable; other derived slots, such as `mutedText` or
`colorLensSize`, cannot be supplied. A slot derived from one other slot alone,
such as `easingSpatial: easingStandard`, is of that slot's type.

An easing slot names one of Qt's curves, or `BezierSpline`. Beside each one is
a `curve` slot, empty by default, holding the cubic Bézier that spline follows
as Qt's `easing.bezierCurve` reads it: six numbers a segment, ending at 1, 1, at
most eight segments. An easing is `BezierSpline` exactly when its curve is
given, and every animation that takes an easing slot takes its curve too, so a
theme's spline reaches every transition; an empty curve leaves Qt's curve as
it is, so a named easing draws exactly as before. A suppliable easing and its
suppliable curve, such as `easingSpatial` and `easingSpatialCurve`, which
follow `easingStandard` and its curve, are supplied together or not at all, so
the pair either is the theme's or follows its derivation whole.

## Component behaviour

The shared controls report state and move through these slots, each of which
defaults to what the shell did before themes could set them (SIL-238):

- **State layers.** `HoverWash` is a control's state layer: the pointer's wash
  comes and goes over `durationState`. A control that passes it `pressed`, and
  where the press landed, gets a press ripple (`Ripple.qml`) for a theme that
  gives `durationRipple`, shaped by the `ripple…` slots and drawn in the
  control's own corners; with none, the control lays `pressColor` flat as
  before.
- **Shape.** A pressed `ActionButton` or `IconButton` closes its corners to
  `radiusPressed` while held, which is `radius` unless a theme gives its own.
- **Focus.** `FocusRing` is `focusRingWidth` wide, `focusRingGap` outside the
  control's edge, in `focusRingColor`, and flares to `focusRingFlare` over
  `durationFocusRing`. By default the gap is minus the width, so the ring is
  the hairline on the control's own edge; where a control drew that edge as its
  own border, it still does, and lays a `FocusRing` only for a ring outside it,
  shown, as Material shows it, only where the keyboard put the focus. A ring
  outside the edge that the control or an ancestor would clip, as a card or a
  scrolled list does at its edge, lies on the control's own edge instead, so it
  is always drawn whole.
- **Motion.** A part of a control moving or changing shape takes
  `durationSpatial` on `easingSpatial`; a control's own colour changing with its
  state takes `durationEffects` on `easingEffects`. A theme gives a spring by
  sampling it into the curve, as `seele-theme material` does.
- **Switch.** `ControlSwitch`'s handle is `switchHandleSize`, `…OnSize` and
  `…PressedSize` off, on and held, in the `switchHandle…` colours, and rides
  between the centres of the track's round ends; `switchStateLayerSize` centres
  a round state layer on it, and the track then keeps its colour.

`tests/tst_behaviour.qml` drives the real controls under a preset and under
Material Expressive Dark: the preset's slots equal what it drew before and a
press lays its colour flat; Material's press ripples from where it landed,
holds for a tap, closes the corners and spreads to cover the control; the
switch's handle grows and overshoots on the spring; and the focus ring flares
and settles outside the edge.

## Cut panels and capitals

A theme may give the shell a harder outline than rounded glass (SIL-239), and
each part of it is a slot whose default draws exactly what the presets draw:

- **Cut corners.** `panelCut` takes a panel's top left and bottom right corners
  off at 45 degrees, that far along each edge. `PanelSurface` then lays its
  material as `CutFrame.qml`, a `QtQuick.Shapes` outline filled with
  `panelColor` and stroked in `panelBorder`, loaded only while the cut is
  given, under everything the panel holds; a cut panel carries no wash or lit
  edge, which would spill past the cuts, and its texture stops half the cut in.
  The frame never draws outside itself: the cut takes at most a third of the
  shorter side, and the trim and glitch only what is left of the top edge. A
  surface that is not a panel passes `cutCorners: false` and keeps the plain
  material, as `GlyphButton`'s tooltip does, like the shell's own tips.
- **Trim and glitch.** On a cut panel, `panelTrim` lights the top cut and runs a
  bar `strokeStrong` thick along the top edge from it in `panelTrimColor`, and
  `panelGlitch` adds a slice past its end, knocked down off the edge, in
  `panelGlitchColor`.
- **Capitals.** `titleCapitalization` sets a panel's title (`PanelHeader`) and
  `labelCapitalization` a section's label (`SectionLabel`), each one of Qt's
  capitalizations by name.
- **Typeface and texture.** The `assets` group's `fontFile…` slots load the
  typeface `fontFamily` names, and `surfaceTexture` stands in for the grain film
  (below).

`themes/cyberpunk` gives all of them; `themes/CYBERPUNK.md` lists its values.

## A theme's own motion and sound

A theme may make its panels move and the shell sound (SIL-240). Both are the
theme's, and whether they happen is the user's: the theme helper's preferences
carry `motion` (on until turned off) and `sound` (off until turned on), set with
`seele-theme motion|sound on|off` or under Effects in the Control Center's
Themes panel, which offers each only for a theme that has it. `Theme.qml` reads
them from `preferences.json` into `motionEnabled` and `soundEnabled`, beside
the slots rather than among them: no theme can supply them.

- **Motion.** On a cut, trimmed panel, `durationPanelTrim` sweeps the trim out
  from the cut as the panel appears, and `durationPanelGlitch` then flickers
  the glitch `panelGlitchFlickers` times, `panelGlitchShift` to either side; it
  flickers again when the pointer comes onto the panel. `CutFrame` starts each
  only on those events and ends each on the still frame, so nothing runs while
  a panel is left alone, and with motion held still it draws the still frame at
  once. A panel appears when its window shows or, where the window stays
  mapped, when its surface does; hiding either puts it at rest. The sweep and
  the jolt stay inside a frame too small for the whole trim. Zero, the default,
  keeps a panel still.
- **Sound.** The `sound…` slots below are played by the shell's
  `ThemeSounds.qml` on the event each is named for, through Qt's SoundEffect,
  which decodes WAV alone; registration refuses a cue in any other format. The
  player is loaded only while sound is on and the theme carries a cue, and
  presentation mode holds every cue back. A notification whose sender sets the
  specification's `suppress-sound` hint gets no cue.

`tests/tst_motion.qml` drives the motion and proves an open panel at rest draws
no frame; `tests/theme-sounds.sh` drives when the cues play.

## Theme documents

A theme is a JSON object: `id` (kebab-case), `name` and `mode` (`light` or
`dark`), then any slots it supplies by name. It must supply the eleven palette
roles. Anything else it leaves out takes its default. A derived or fixed slot
cannot be supplied; neither can a key the contract does not name, except the
extensions, which are not styling: the Base16 `palette` and `terminal` colours
the theme helper keeps for other applications, the publication's format
`version`, and `material`, the seed and contrast a Material 3 Expressive theme
was grown from. Types are checked: an `int` slot rejects `8.5`, a `color` slot
rejects a name, a `weight`, `easing` or `capitalization` slot takes a name from
`enums`, and a `curve` slot takes whole segments of finite numbers.

A `path` slot holds an absolute path or nothing. A theme folder names its own
files relative to itself; registration resolves them before the shell sees
them. An `outputPaths` slot is an object from output names, as the compositor
gives them (letters, digits, `.`, `_` and `-`, at most 64), to absolute paths.

## The existing presets

The thirteen presets of the theme switcher are contract documents. `seele-theme`
builds each one from the preset's Base16 palette in the catalog the flake
generates:

| Key | From |
| --- | --- |
| `id`, `name`, `mode` | the catalog entry |
| `base`, `crust` | `base00` |
| `mantle` | `base01` |
| `surface` | `base02` |
| `overlay` | `base03`, held to 3:1 against `base` |
| `text` | `base05`, or `base06` or `base07` where a scheme files a grey in `base05` |
| `subtext` | `base04`, held to 4.5:1 against `base` |
| `accent` | `base0D`, which is the configured accent in the Catppuccin presets |
| `red`, `green`, `yellow` | `base08`, `base0B`, `base0A` |
| `fontFamily` | the configured monospace font |
| `palette`, `terminal` | the Base16 palette and the sixteen terminal colours, as extensions |

A preset supplies nothing else, so every other slot takes its default, and the
defaults are the look these presets were drawn with. Base16 stays their source
rather than literal colours in a file, because the flake renders each preset's
GTK, Qt, KDE, Zen, Spicetify, Vicinae and Neovim themes from the same palette,
and the Catppuccin presets carry the accent the flake is configured with.

The helper embeds `theme-contract.json` and checks every document, and the
`theme.json` each becomes, whenever it reads the catalog. One invalid preset
refuses the whole catalog, naming the document and the rule it broke, so it
never reaches a surface; the parent flake's `checks.theme-presets` lists every
generated preset, which makes that a build failure. `theme.json` is the
document plus `version` and `wallpaper`, the image the desktop background and
the lock screen draw. No preset has an image of its own, so for a preset that is
the configured wallpaper the fallback below names.

`tests/theme-snapshots/themes.json` holds the thirteen documents as `seele-theme
list` prints them for the pinned schemes. The pixel baselines are rendered from
exactly those documents, `tests/theme-contract.js` checks each of them and its
`theme.json` against the contract, and `config-tools/tests/themes.py` checks
the helper's listing, its publication and its refusals.

## Theme folders

A theme that is not a Base16 preset is a folder: a `theme.json` and the files it
names. Adding one is the folder and one line in `themes/default.nix`; no shell
code changes. `themes/README.md` describes the folder. The document is a theme
document as above, with three more rules, because the theme reaches the whole
desktop and not only the shell:

- It carries `palette`, the sixteen Base16 colours `base00` to `base0F` as
  lowercase `#rrggbb`, the form Stylix writes back. The flake renders the
  theme's GTK, Qt, KDE, Zen, Spicetify, Vicinae and Neovim themes from it, and
  the terminal colours come from it.
- Its palette roles are opaque `#rrggbb`, as the switcher and the applications
  draw them. A folder chooses its roles itself; nothing is projected.
- Its `id` is the folder's name. `terminal` and `version` are the helper's to
  add.
- Its `wallpaper` and its `surfaceTexture`, if it has them, are each a complete
  PNG or JPEG, the formats the desktop's Qt decodes without an image-format
  plugin, of at most the 256 MiB Qt decodes one image into (four bytes a pixel,
  eight for a 16-bit PNG). Registration decodes a PNG to its last row and walks
  a JPEG's markers to its end, so a file that is another format, cut short or
  damaged is refused there rather than drawn as nothing; reading the catalog
  rechecks the registered copy's signature. The background, the lock screen
  and the surfaces escape the name into the file URL, so it may hold any
  character a file name can. Each `fontFile…` is a TrueType or an OpenType
  font, which Qt's font loader takes, and each `sound…` cue is a WAV file,
  which Qt's SoundEffect plays; either is refused by its first bytes otherwise,
  rather than loaded as nothing or played as silence.
- It carries no `wallpaperOutputs`: output names belong to a machine, so which
  image each output shows is the host's wallpaper mapping.
- If it records a `material` source, it is exactly the document `seele-theme
  material` grows from that seed and contrast, slot for slot
  (`themes/MATERIAL.md`).

A `path` slot names a file relative to the folder, such as
`"soundNotification": "sounds/ping.wav"` or `"wallpaper": "wallpaper.jpg"`. The
name has to stay inside the folder and lead to a file of at most 16 MiB.

`seele-theme register <id> <folder> <out>` runs at build time
(`packages/core/themes.nix`). It holds the folder to the contract and these
rules, and writes the registered copy: the document with every named file
resolved to its absolute path there, and those files. A folder that breaks a
rule fails that build, and the message names the folder and every rule at once:

```
Seele Themes: Invalid theme folder example:
  palette.base0F: missing
  missing required slot accent
  radius: "round" is not a int
```

The flake reads a folder's name, mode and Base16 palette while it evaluates, to
render those application themes, so it refuses a folder missing one of them
before anything else, in the same form. The catalog entry names the registered
`theme.json`, and the helper holds that document to the same rules, and to the
entry's identity and palette, whenever it reads the catalog. `list` publishes
it as it was registered, plus the configured typeface unless it names one and
the terminal colours of its palette; the selection adds `version`, and its
`wallpaper` is the folder's own image or, without one, the configured wallpaper
as for a preset.

`tests/theme-folders/` holds the example folder the checks register, the
pictured folder that carries its own wallpaper, and the invalid folders each
check refuses, among them a Material folder edited by hand. `checks.theme-folders` builds every registered folder, both
fixtures and `themes/template`, lists them beside a preset, switches between the pictured folder, the
example and the preset to prove which wallpaper each publishes, and proves each
invalid fixture fails its own build with its message;
`config-tools/tests/theme_folders.py` does the same through the CLI, and also
registers a renamed copy of the template and the template with each recipe of
`themes/AUTHORING.md` added, so the guide's recipes are known to be accepted;
the pixel baselines draw the example in the switcher and the Themes panel and
the folders the shell ships on every surface.

## Material 3 Expressive

`themes/material-expressive-light` and `-dark` set every slot Material 3
Expressive has a token for: the eleven roles and the suppliable material
colours from a seed-grown scheme, the type ramp, corner scale, spacing on the
4dp grid, component heights, state layers, durations and Bézier curves.
`themes/MATERIAL.md` lists each token and the slot it reaches, and what is kept
at its default and why. `tests/tst_palette.qml` applies both folders through
`applyTheme` and checks every value they supply is the value the shell draws,
and that a spline reaches an animation whichever order Qt binds its easing and
curve in.

## Optional slots and their fallbacks

**Sound.** `soundNotification`, `soundNotificationUrgent`, `soundFocusDone`,
`soundPanelOpen`, `soundPanelClose` and `soundLevelStep` are each a file to play
for that cue, a WAV file. An empty or omitted slot is silence, and silence is
the fallback: nothing plays unless the theme supplies a sound, and nothing plays
at all until the user turns sound on.

**Assets.** `fontFileLight`, `fontFileRegular`, `fontFileMedium` and
`fontFileStrong` are the typeface files a theme carries, the faces its
`weightLight`, `weightRegular`, `weightMedium` and `weightStrong` set; the shell
loads each one given, so the face `fontFamily` names draws on a machine that
never installed it. `surfaceTexture` is an image tiled over every surface at
`grainOpacity` in place of the grain film. Nothing is the fallback: an empty
typeface file loads nothing, so `fontFamily` names a typeface installed on the
system, and an empty texture keeps the film. A loader exists only for a file
that is given, so switching away from a theme that carried one loads nothing.

**Wallpaper.** `wallpaper` is the theme's own image. A theme without one is
published with the configured wallpaper, the catalog's `wallpaper`, so switching
to it keeps the image already on screen; the flake configures that image and
the session's `$SEELE_SHELL_WALLPAPER` as the same file. Before any theme is
published, or if a published one names none, the shell draws
`configuredWallpaper`: `$SEELE_SHELL_WALLPAPER` when the session sets it, else
`/etc/wallpaper/wallpaper.jpg`. `wallpaperImage` is the one the background
draws, over `base`, so a missing file still leaves the palette.

The host's wallpaper mapping (SIL-236), declared in the parent flake's Nix and
carried by the catalog, changes which image that is. `seele-theme` resolves it
for the theme it publishes: an image mapped to that theme on an output, then to
that theme, then the theme's own, then one mapped to the output for the theme's
own light or dark mode, then to the output, then to that mode, then the
configured wallpaper. `wallpaper` is what every output draws unless
`wallpaperOutputs` names it, and the publication names an output only when its
image differs, so a theme the mapping says nothing about publishes exactly what
it did before. Light and dark are the theme's own mode, so a dark theme worn in
light mode keeps a dark wallpaper. `config-tools/README.md` lists the rules the
helper holds a mapping to.

The pickers show the image a theme is linked to before it is worn: the image
mapped to that theme, else its own, else the one mapped to its mode, which
`seele-theme list` reports beside the documents as `wallpapers`. The
switcher's miniature desktops and the Themes gallery's tiles draw it through
`ThemePicture.qml`, a Canvas that crops it to fill and clips it to the tile's
corners without a shader, so the software renderer the pixel baselines use
draws it too. A theme linked to nothing shows its `base`. A tile's name sits
on its own `mantle` at `themeTileShadeOpacity`, in its own `text`, so it
reads on any picture the way the theme's own panels do.

Each output's background draws `wallpaperFor(<its name>)` through
`Wallpaper.qml`, which loads a new image behind the one on screen and shows it
only once it has decoded, so a switch goes from one picture to the next without
the bare base colour in between. The lock screen takes the same published
`wallpaper` and `wallpaperOutputs`, through `Palette.imageFor`; both escape each
path segment into the file URL. The greeter runs before any user theme exists
and keeps the system's image.
`tests/tst_wallpaper.qml` drives the component through those switches with real
image files, `tests/tst_palette.qml` checks that a published theme's own image is
applied, that one without leaves the configured wallpaper, and that each
output's binding follows its mapped image through a mode switch, and
`config-tools/tests/wallpapers.py` drives the mapping through the helper.

## What counts as styling

Any colour, any colour arithmetic on a literal (`alpha(text, 0.3)`), any easing
or weight enum, and any number bound to a geometry, type, shape, material or
motion property. The check allows only structure: zero; an integer count that
multiplies or divides a measure (`cardPadding * 2`, `width / 3`); a half; one as
full opacity, a gradient's end, a clamp or a full colour channel; a full
turn of a rotation; comparisons,
indices and remainders; and `"transparent"`, which paints nothing.

Drawing coordinates of an icon or an illustration are not slots: they are the
artwork's own, sized by a slot and coloured from slots. Such a region is marked
in the source and states why:

```qml
// theme-contract: artwork -- the headphone and AirPods silhouettes, drawn on their own 16-unit grid
...
// theme-contract: end
```

The check lists every region with `--report`. Today there are four: the
headphone silhouettes, the theme switcher's miniature desktop and its slices,
and the speed gauge's dial.

### What the check reads

The check is a static scan of the QML and JavaScript source, not an evaluator.
It follows a binding across lines while its brackets are open, its value has
not started, its line ends on an operator, or the next line carries it on, and
it judges a property by its enclosing object, so a multi-line `GradientStop`
or `SpringAnimation` is read the same as a one-line one. It recognises
compound and bracketed assignments, colour-typed declarations, every numeric
literal form and every SVG colour name. It reads a `Binding` or
`PropertyAction` as the property it targets. It also catches a numeric
constant or a colour name kept under a neutral name -- a QML property or a
JavaScript `const`/`let`/`var`, through any chain of aliases -- when a styling
value is made of it; a property the file reassigns is state, and a constant
combined with live data is an input to a computation, not a style. Inside an
artwork region only drawing coordinates are exempt.

What it cannot see is a value computed at run time: a function that returns a
number, a value built with `Qt.binding`, or styling set from C++. Those are
review's to catch, as is a slot whose name does not say what it is for.

## Adding or changing a value

1. Add the property to `Theme.qml` in its group, after a comment that says what
   it is for. Name it for its role, not its value.
2. Use it from the surface through `root.`, `theme.` or `<id>.theme.`.
3. Run `node tests/theme-contract.js projects --update` and review the JSON
   diff, then `node tests/theme-contract.js projects`.
4. Run the pixel baselines. A value that only moved into the contract changes no
   pixel; a new look is recorded with `--update` there and reviewed image by
   image.

A shell root that declares a property with a slot's name fails the check,
because the shell would draw its own value while the contract claimed another.

`Theme.qml` also carries a few members that are not styling values, such as
`keyboardNavigation`, and `supplied`, the derived colours a published theme
gives in place of their derivations. They are declared after the helper functions and named in
the contract's `members`; the check scans the whole file, so any other property,
wherever it is declared, has to be a slot.

## Scope and what comes next

The contract covers `projects/shell`, `projects/shared` and `projects/notes`.
The greeter, lock screen and polkit agent are separate scenes that take only
the palette, through `Palette.js`; their own dimensions are outside this
version of the contract.

The shell applies every slot a theme may supply from `theme.json`, its own
wallpaper included: Theme.qml's `applyTheme` sets each one to the theme's value
or, where the theme leaves it out, the default, so a preset that follows a
folder theme looks exactly as it always did. The pixel baselines run every
preset through that same path; they capture surfaces rather than the
background, so a wallpaper changes none of them. One image per theme is SIL-235,
images per output and per light and dark theme are SIL-236, a theme's own
typeface, texture, cut panels and capitals are SIL-239, and a theme's own
motion and its sound cues are SIL-240. Each builds on these rules: the contract says
what a theme may set and what an omitted slot falls back to.
