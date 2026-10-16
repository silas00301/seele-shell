# Theme contract

One contract names every styling value the desktop shell and Seele Notes draw
with: colours, typography, shape, spacing, sizes, materials, motion, sound and
wallpaper. A surface reads all of them from `Shared.Theme` and writes none of
its own, so a theme can restyle the shell without a code change.

| File | Role |
| --- | --- |
| `theme-contract.json` | The schema: every slot with its group, type, default or derivation and description; the rules a theme document follows; the fallbacks; what counts as structure rather than style. |
| `Theme.qml` | The one implementation. Every slot is a property here, in the same order and with the same default. |
| `tests/theme-contract.js` | Holds the two to each other, validates theme documents, and fails on any styling literal outside the contract. |
| `tests/theme-snapshots/` | Pixel baselines of every preset (SIL-232). A refactor of the contract proves it changed nothing there. |

## Slots

399 slots in eleven groups, each a `// <Section> ----` header in `Theme.qml`:

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
| `wallpaper` | An optional image of the theme's own. | optional |

A writable property is a slot a theme may supply. A readonly one is *derived*:
computed from the slots it names, such as `panelColor: alpha(mantle, 0.88)`, so
a theme changes it through those slots. Every default reproduces the shell as it
was drawn before themes could set more than colours.

## Theme documents

A theme is a JSON object: `id` (kebab-case), `name` and `mode` (`light` or
`dark`), then any slots it supplies by name. It must supply the eleven palette
roles. Anything else it leaves out takes its default. A derived or fixed slot
cannot be supplied; neither can a key the contract does not name, except the
`palette` and `terminal` extensions the theme helper keeps for other
applications. Types are checked: an `int` slot rejects `8.5`, a `color` slot
rejects a name, a `weight` or `easing` slot takes a name from `enums`. Every
preset `seele-theme` publishes today is a valid document.

A `path` slot holds an absolute path or nothing. A theme folder names its own
files relative to itself; registration resolves them before the shell sees
them.

## Optional slots and their fallbacks

**Sound.** `soundNotification`, `soundNotificationUrgent`, `soundFocusDone`,
`soundPanelOpen`, `soundPanelClose` and `soundLevelStep` are each a file to play
for that cue. An empty or omitted slot is silence, and silence is the fallback:
nothing plays unless the theme supplies a sound. No surface plays one yet.

**Wallpaper.** `wallpaper` is the theme's own image. Without one the shell keeps
the configured wallpaper, `configuredWallpaper`: `$SEELE_SHELL_WALLPAPER` when
the session sets it, else `/etc/wallpaper/wallpaper.jpg`. `wallpaperImage` is
the one the background draws, over `base`, so a missing file still leaves the
palette.

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
`keyboardNavigation`. They are declared after the helper functions and named in
the contract's `members`; the check scans the whole file, so any other property,
wherever it is declared, has to be a slot.

## Scope and what comes next

The contract covers `projects/shell`, `projects/shared` and `projects/notes`.
The greeter, lock screen and polkit agent are separate scenes that take only
the palette, through `Palette.js`; their own dimensions are outside this
version of the contract.

Today the shell still applies only the palette roles and `fontFamily` from
`theme.json`, through `Palette.assign`. Expressing the existing presets as
contract documents is SIL-233, registering theme folders and failing invalid
ones at build time is SIL-234, swapping wallpapers per theme is SIL-235, and
playing the sound cues is SIL-240. Each builds on these rules: the contract says
what a theme may set and what an omitted slot falls back to.
