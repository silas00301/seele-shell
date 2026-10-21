# Writing a theme

A theme is a folder here and one line in `default.nix`. This guide takes a new
one from `template/` to the theme switcher in under an hour: about half of it
choosing colours, the rest copying, checking and looking. Nothing in the shell's
code changes; the folder is data, held to the theme contract
(`../projects/shared/THEME-CONTRACT.md`) when the flake builds it.

Work in the development shell, from the repository root of `seele-shell`:
`nix develop` provides cargo, node and the Qt the commands below run on.

## Choose the route

| You want | Do | Time |
| --- | --- | --- |
| A Base16 scheme that the pinned `base16-schemes` already has | Add its ID, name and mode to `modules/features/themes/_theme-switching/presets.json` in the parent repository (its `docs/theme-switching.md`). No folder. | minutes |
| Material 3 Expressive from another seed colour | `seele-theme material <seed> <light\|dark> <id> <name> > themes/<id>/theme.json`, then steps 3 to 6 below. Never edit the result: registration refuses a Material folder that is not exactly what its seed grows (`MATERIAL.md`). | minutes |
| Anything else | Start from `template/`, as below. | under an hour |

## The template

`template/theme.json` is a complete dark theme that supplies only what a theme
must: its identity, the eleven palette roles and the sixteen Base16 colours the
desktop's other applications are themed from. Each role is exactly the Base16
slot a preset takes it from, so the shell and every application agree, and
each clears the legibility floors in step 2. Every other slot -- type, corners,
spacing, materials, motion -- is left out, so it keeps its default and the theme
draws as the presets do, in its own colours.

The template is not in `default.nix`, so it never reaches the switcher. The
checks register it, and a copy of it under another name, on every build
(`checks.theme-folders`, `projects/config-tools/tests/theme_folders.py`), and
`tests/theme-template.js` keeps it legible and in step with this guide. Every
JSON recipe below is registered on top of the template by the same checks, so
each one is known to be accepted as written.

## 1. Copy the template (2 minutes)

```sh
cp -r themes/template themes/aurora
```

Then, in `themes/aurora/theme.json`:

- `id` is the folder's name exactly: lowercase letters and digits joined by
  single hyphens. Registration refuses a copy that still says `"template"`.
- `name` is what the switcher and the Themes panel show, at most 64
  characters.
- `mode` is `dark` or `light`: the mode the theme serves. The switcher files it
  there, choosing it from the switcher makes it the theme for that mode, and the
  other applications take it as their polarity.

## 2. Choose the colours (20 to 30 minutes)

Every colour is a `#rrggbb`, and the Base16 ones are lowercase: the flake reads
the palette while it evaluates and takes it in that form only.

### The eleven palette roles

These are what the shell draws with. All eleven are required and opaque: the
shell lays its own translucency over them.

| Role | Draws | From, in a preset | Floor against `base` |
| --- | --- | --- | --- |
| `base` | The ground the wallpaper sits on, and a panel's glass | `base00` | |
| `mantle` | Panel material, translucent over the wallpaper | `base01` | |
| `crust` | The darkest step: the grounding ring around a surface, wells, the bar strip | `base00` | |
| `surface` | Cards and rows, as tints of it | `base02` | |
| `overlay` | Quiet glyphs, rules and inactive marks | `base03` | 3:1 |
| `text` | Primary text and glyphs | `base05` | 7:1, and 4.5:1 on `surface` |
| `subtext` | Secondary text | `base04` | 4.5:1 |
| `accent` | State, selection and the one lit control | `base0D` | 3:1 |
| `red` | Danger and failure | `base08` | 3:1 |
| `green` | Success and done | `base0B` | 3:1 |
| `yellow` | Warnings and pending work | `base0A` | 3:1 |

`text` has more headroom than body text strictly needs because panels are
translucent: the wallpaper and the compositor's blur sit behind `base`.

In a light theme the ramp runs the other way: `base00` is the lightest colour
and `base07` the darkest, since the applications read `base00` as their
background and `base05` as their text.

### The Base16 palette

`palette` holds `base00` to `base0F`. GTK, Qt, KDE, Zen, Spicetify, Vicinae and
Neovim are rendered from it when the flake builds, and the terminal's sixteen
colours come from it as well:

| Slots | Are | The terminal's |
| --- | --- | --- |
| `base00` to `base07` | A ramp from the ground (`00`) through panels, selection and comments to the brightest foreground (`07`) | black, bright black (`03`), white (`05`), bright white (`07`) |
| `base08`, `base0B`, `base0A`, `base0D`, `base0E`, `base0C` | Red, green, yellow, blue, magenta and cyan | the six colours, normal and bright |
| `base09`, `base0F` | Orange, and a brown or deprecated hue | |

The simplest theme keeps every role on the slot the table above names, as the
template does: change a role and its slot together. A role may differ on
purpose -- Cyberpunk's `crust` is darker than its `base00`, and its `surface`
is lifted for its cards -- and the shell draws the role while the applications
draw the slot.

### A light start

To begin from a light theme instead, replace the template's identity, roles
and palette with this:

```json
{
  "mode": "light",
  "base": "#f6f7f9",
  "mantle": "#eceef2",
  "crust": "#f6f7f9",
  "surface": "#dde1e8",
  "overlay": "#7c8493",
  "text": "#1f2329",
  "subtext": "#4f5665",
  "accent": "#2f62c9",
  "red": "#c2383f",
  "green": "#2c7a30",
  "yellow": "#8a6700",
  "palette": {
    "base00": "#f6f7f9",
    "base01": "#eceef2",
    "base02": "#dde1e8",
    "base03": "#7c8493",
    "base04": "#4f5665",
    "base05": "#1f2329",
    "base06": "#15181d",
    "base07": "#0b0d10",
    "base08": "#c2383f",
    "base09": "#a8521a",
    "base0A": "#8a6700",
    "base0B": "#2c7a30",
    "base0C": "#0f777b",
    "base0D": "#2f62c9",
    "base0E": "#7a4bc2",
    "base0F": "#8a5a3c"
  }
}
```

### Check it

```sh
theme-check themes/aurora
```

builds the theme helper, registers the folder exactly as the flake's build
does, printing the build's own message if it is refused, and measures every
role against the floors above:

```
themes/aurora: registers; text 13.7:1, subtext 7.6:1, overlay 3.3:1, accent 7.2:1, red 5.8:1, green 9.6:1, yellow 10.5:1, card 10.5:1
```

A role under its floor, or a mode the colours contradict (a `dark` theme whose
text is darker than its ground), fails with a line saying which. A note says
where `base`, `text`, `accent` or a status colour is not the slot the
applications draw. Registration itself does not measure contrast, so this is the
place legibility is checked. Outside the development shell the same check is
`node tests/theme-check.js --helper <seele-theme> themes/aurora`.

## 3. Register it (1 minute)

Add the folder to `default.nix`, after the folders already there; the switcher
lists folders after the presets in this order:

```nix
[
  ./material-expressive-dark
  ./material-expressive-light
  ./cyberpunk
  ./aurora
]
```

`tests/theme-snapshots/registered.json` holds every listed folder as the helper
lists it, for the pixel check to draw; `theme-preview` in the next step writes it
again each time it runs, and the config-tools build fails until it is current.

## 4. Look at it (2 minutes a round)

```sh
theme-preview aurora
```

refreshes `registered.json`, then draws the theme on the ten main surfaces --
the bar, the Control Center, toasts, the notification panel, Now Playing,
Audio, Power, the volume OSD, the theme switcher and the Themes panel -- with
the real shell QML, and writes them to `/tmp/seele-theme-preview/aurora/`.
It draws only that theme (the pixel check's `--only aurora`), beside the
presets it sits with in the switcher, so a round takes well under a minute
after the first build.

Change `theme.json`, run `theme-check` and `theme-preview` again, and repeat.
Look at the toasts and the Audio panel in particular: they carry every role,
including `green`, `yellow` and `red`.

## 5. Make it your own (optional, 0 to 30 minutes)

Any slot the contract lets a theme supply can go in `theme.json` beside the
roles; `../projects/shared/theme-contract.json` lists every one with its type,
default and what it draws. Each recipe here is a set of slots to add, and
`theme-check` and `theme-preview` show the result.

### Corners

```json
{
  "radius": 14,
  "radiusSmall": 10,
  "meetingCellRadius": 4,
  "meetingSelectionRadius": 6
}
```

`radius` rounds panels, buttons and bar entries; `radiusSmall` rounds what sits
inside them. `0` everywhere gives square corners, as Cyberpunk has.

### A typeface of its own

```json
{
  "fontFamily": "Example Sans",
  "fontFileLight": "fonts/ExampleSans-Light.ttf",
  "fontFileRegular": "fonts/ExampleSans-Regular.ttf",
  "fontFileMedium": "fonts/ExampleSans-Medium.ttf",
  "fontFileStrong": "fonts/ExampleSans-SemiBold.ttf"
}
```

`fontFamily` is the family name inside the files. The four files are the faces
`weightLight`, `weightRegular`, `weightMedium` and `weightStrong` set text in,
each a TrueType or OpenType file in the folder; the shell loads them while the
theme is selected, so the theme draws the same on a machine that never installed
the face. Without the files, `fontFamily` must name a face the system already
has, or Qt falls back to another; without `fontFamily`, the theme keeps the
configured monospace face the presets use. Ship one static file per weight, since Qt's
font loader ignores a variable font's weight axis, and prefer a build with its
TrueType hinting intact: Cyberpunk's Chakra Petch from Google Fonts lacks it, and
small text then opens gaps inside words. Ship only faces whose licence allows
it, with the licence beside them, and record the source.

A face that sits smaller or larger than the default monospace one needs the
type ramp moved with it; Cyberpunk moves the small end up a pixel:

```json
{
  "textMicro": 9,
  "textCaption": 10,
  "textLabel": 11,
  "textBody": 12,
  "textStrong": 13,
  "textLead": 14
}
```

Larger type widens the bar. Check that it still fits a 1920-pixel output with
every indicator showing, and narrow `barLabelMaximumWidth` if it does not, as the
Material folders do.

### Headings in capitals

```json
{
  "titleCapitalization": "AllUppercase",
  "labelCapitalization": "AllUppercase",
  "trackingLabel": 2.0
}
```

Each capitalization is one of `MixedCase`, `AllUppercase`, `AllLowercase`,
`SmallCaps` and `Capitalize`. Weights are named the same way, from `Thin` to
`Black`: `"weightStrong": "Bold"`.

### Flat glass, its own texture and wallpaper

```json
{
  "washCrownOpacity": 0.0,
  "washFadeOpacity": 0.0,
  "washFootOpacity": 0.0,
  "panelColor": "#f016181d",
  "surfaceTexture": "texture.png",
  "grainOpacity": 0.05,
  "wallpaper": "wallpaper.png"
}
```

The three wash opacities take the lit crown off every surface. `panelColor` is
a derived slot a theme may supply in place of its derivation (here the
template's `base` at 94%); such colours may be `#aarrggbb`, with alpha first as
Qt reads it, while the eleven roles stay opaque. `surfaceTexture` is a PNG or
JPEG tiled over every surface at `grainOpacity` in place of the grain film.
`wallpaper` is a PNG or JPEG of at most 16 MiB, drawn on the desktop and the
lock screen while the theme is selected. Registration decodes it and the
texture whole, so a damaged image, or one larger than Qt decodes (about
8192 × 8192), is refused. A theme without one keeps the wallpaper already
configured, and the host's `seele.wallpaper` mapping in the
parent flake can still choose another per output. A folder cannot carry
`wallpaperOutputs`: output names belong to a machine. Use only images you made
or may redistribute; Cyberpunk draws its own with `cyberpunk/art/draw.js`.

### Cut panels

```json
{
  "panelCut": 14,
  "panelTrim": 64,
  "panelGlitch": 22,
  "panelGlitchShift": 6,
  "durationPanelTrim": 260,
  "durationPanelGlitch": 240,
  "panelGlitchFlickers": 3
}
```

`panelCut` cuts a panel's top left and bottom right corners at 45 degrees
instead of rounding them, `panelTrim` lights the top cut and runs a bar along
the top edge, and `panelGlitch` adds a slice past its end. The last three make
the trim sweep in and the glitch flicker as a panel appears and when the pointer
comes onto it; leave them out for a still frame. The frame never draws outside
the panel: the cut takes at most a third of the shorter side, and the trim and
glitch only what is left of the top edge. Only panels are cut; toasts, cards
and tooltips keep the plain material. The user can hold all motion
still under Effects in the Themes panel. `panelTrimColor` and
`panelGlitchColor` default to `accent` and `red`.

### Motion

```json
{
  "durationFast": 150,
  "durationNormal": 300,
  "easingStandard": "BezierSpline",
  "easingStandardCurve": [0.2, 0.0, 0.0, 1.0, 1.0, 1.0]
}
```

Durations are milliseconds. An easing names one of Qt's curves, such as
`OutCubic`, or `BezierSpline`, which it is exactly when its `…Curve` slot gives
the spline: six numbers a segment (two control points and the end point),
ending at 1, 1, at most eight segments. A suppliable pair such as
`easingSpatial` and `easingSpatialCurve` is given together or not at all.

### Material's press and focus

```json
{
  "durationRipple": 450,
  "radiusPressed": 8,
  "durationFocusRing": 600,
  "focusRingWidth": 3,
  "focusRingGap": 2,
  "focusRingFlare": 8
}
```

A press then ripples from where it landed and closes the button's corners to
`radiusPressed`, and the keyboard focus ring sits outside the control and flares
as it appears. These are Material Expressive Dark's values.

### Sounds

```json
{
  "soundNotification": "sounds/notification.wav",
  "soundNotificationUrgent": "sounds/notification-urgent.wav",
  "soundFocusDone": "sounds/focus-done.wav",
  "soundPanelOpen": "sounds/panel-open.wav",
  "soundPanelClose": "sounds/panel-close.wav",
  "soundLevelStep": "sounds/level-step.wav"
}
```

Each cue is a WAV file, the one format Qt's SoundEffect plays; registration
refuses any other. Keep them short and well below full scale -- Cyberpunk's are
16-bit mono at 48 kHz, under 0.75 seconds, peaking between -13 and -22 dBFS,
and ending on silence. Any cue may be left out. A notification whose sender
sets the specification's `suppress-sound` hint, as the focus timer's own does,
plays no cue. Nothing plays until the user
turns sounds on.

### Sizes that move together

Sizes are slots too, but some are laid out against each other. The Control
Center sets its three connectivity rows beside its two audio sliders, so keep
`3 × rowHeight + 2 × spaceTight` no taller than
`2 × audioSliderHeight + 3 × spaceLarge`; the defaults and every shipped theme
make both 128. Change sizes last, and look at the Control Center when you do.

## 6. Check and ship it (10 minutes)

```sh
theme-check themes/aurora
nix build ".#checks.$(nix eval --impure --raw --expr builtins.currentSystem).theme-folders"
theme-snapshots --update
jj diff --stat
```

`theme-snapshots --update` records the theme's ten pixel baselines beside the
others. The diff should add `tests/theme-snapshots/baseline/aurora/` and change
only `manifest.json` and `registered.json` besides the folder and its line: any
other changed image means something beyond this theme moved, and `theme-snapshots
--keep <dir>` shows what. Write down, beside the folder, where every file it
carries comes from and under which licence, as `CYBERPUNK.md` does.

The theme reaches the desktop through the parent flake: once the shell change
is merged, the parent's `seele-shell` gitlink moves to it, a rebuild adds the
theme to the catalog, and Super + Ctrl + Shift + T, the Control Center's Themes
panel and Vicinae's Seele Themes offer it after the presets.

## When something is refused

The build names the folder and every rule it broke at once, for example:

```
Seele Themes: Invalid theme folder aurora:
  id: "template" is not the folder's name
  palette.base0D: "#7AA6F0" is not a lowercase #rrggbb
  radius: "round" is not a int
```

| Message | Means |
| --- | --- |
| `id: "…" is not the folder's name` | The copied `id` was not changed, or the folder was renamed. |
| `missing required slot …` | One of the eleven roles is missing. |
| `…: "#…" is translucent; a palette role is an opaque #rrggbb` | A role has alpha. Only derived colours such as `panelColor` may. |
| `palette.…: … is not a lowercase #rrggbb` | A Base16 colour has capitals or another form. |
| `…: not a contract slot` | A misspelt or invented key. The contract lists every slot. |
| `…: derived, so a theme cannot supply it` | The slot is computed from others; change those instead. |
| `…: "…" is not a int` | A value of the wrong type. |
| `…: "…" is not a path inside the theme folder` | A file named outside the folder. |
| `…: "…" is not a file in the theme folder` | The file a slot names is missing, or is not a plain file. |
| `…: "…" is larger than 16 MiB` | A named file is over the cap every file a theme carries is held to. |
| `…: "…" is not a PNG or JPEG image`, `… is not a TrueType or OpenType font`, `… is not a WAV sound` | A file whose first bytes are not that format; a renamed file is refused. |
| `…: "…" is not a complete PNG image (…)`, `… is not a complete JPEG image`, `… is an image with no pixels` | An image is decoded whole when it registers, so one cut short or damaged is refused rather than drawn as nothing. |
| `…: "…" is W×H pixels, more than the 256 MiB Qt decodes one image into` | An image larger than Qt will decode, about 8192 × 8192; scale it down. |
| `theme.json is not a file of at most 64 KiB` | The document is missing or far larger than a theme needs. |

## What a theme cannot do

A theme sets values the contract names; it cannot add one. A look that needs a
value the shell does not yet read -- a new size, a new colour, a new kind of
motion -- is a change to the contract and to `Theme.qml`, made as
`THEME-CONTRACT.md`'s "Adding or changing a value" describes, with every
existing theme proved pixel-identical before the new theme uses it. Outside the
shell and Notes, the lock screen takes the palette and the wallpaper and the
polkit agent the palette alone; the greeter, the boot screen and the cursor keep
their configured look.
