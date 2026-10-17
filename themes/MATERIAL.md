# Material 3 Expressive

`material-expressive-light` and `material-expressive-dark` are drop-in theme
folders that draw the shell with Material 3 Expressive's tokens: a light and a
dark colour scheme grown from one seed colour, the type scale, the corner
scale, tonal elevation, state layers and timed motion. They are themes like
any other: every preset keeps its look, and switching to one of these changes
only the values the contract's slots take, never a surface's code.

Both are grown from Material's baseline seed, `#6750a4`. Neither folder is
written by hand:

```sh
seele-theme material '#6750a4' dark material-expressive-dark 'Material Expressive Dark' > material-expressive-dark/theme.json
seele-theme material '#6750a4' light material-expressive-light 'Material Expressive Light' > material-expressive-light/theme.json
```

Each `theme.json` records its source as `"material": {"seed": …, "contrast": …}`,
and registration grows the document again from that source and refuses the
folder if any value differs, naming the slot. A Material theme is therefore
always exactly what its seed grows; to change one, change the seed (or the
contrast, from -1 to 1) and write it again. Another seed is another theme: a new
folder, the same command, and one line in `default.nix`.

`seele-theme material tokens '#6750a4' [contrast]` prints every token the seed
grows: both schemes' colour roles and the reference scales below.

## Colour

`config-tools/src/material/` ports Material Color Utilities' HCT colour space
and its `SchemeExpressive` dynamic scheme under the 2025 colour spec, the spec
Material 3 Expressive introduced, on Material's phone platform (the watch
platform is for round black screens). The palettes are Expressive's: the
primary at the seed's hue, secondary and tertiary turned away from it, and
neutrals tinted toward it, with contrast curves holding every content colour
to its background. All 53 roles are produced: the surface container ramp,
`on_surface` and its variant, outlines, the primary, secondary, tertiary and
error groups with their containers, fixed and dim roles, and the inverse
roles. `config-tools/tests/material-oracle.json` holds the library's own
output for 96 schemes (twelve seeds, light and dark, four contrast levels) and
the Rust tests compare every role with it; `material-oracle.mjs` beside it
regenerates the file from the library's sources at the pinned revision.

The shell's eleven roles take these:

| Role | Material role |
| --- | --- |
| `base` | `surface` |
| `mantle` | `surface_container` |
| `crust` | the dimmest surface: `surface_container_lowest` in dark, `surface_dim` in light |
| `surface` | `surface_container_high` |
| `overlay` | `outline` |
| `text` | `on_surface` |
| `subtext` | `on_surface_variant` |
| `accent` | `primary` |
| `red` | `error` |
| `green`, `yellow` | Material custom colours, harmonized toward the seed |

Custom colours follow Material's recipe: a design colour, turned toward the seed
by half their hue difference and at most 15 degrees, at tone 40 in a light
scheme and 80 in a dark one, its container at 90 and 30. The design colours are
the Material Design palette's Green 600 and Yellow 700, and Orange 700, Cyan 600
and Brown 500 for the Base16 palette.

The Base16 palette the other applications are themed from is
`surface`, `surface_container`, `surface_container_highest`, `outline`,
`on_surface_variant`, `on_surface`, `inverse_surface`, white or black, then
`error`, orange, yellow, green, cyan, `primary` as blue (a preset's accent is
its blue too), `tertiary` as magenta, and brown.

## Elevation

Material 3 expresses height through tone: each level is drawn in a surface
container role, and a shadow only where a surface floats. The shell's surfaces
are opaque, and each layer is one step further from the surface:

| Level | Height | Material role | In the shell |
| --- | --- | --- | --- |
| 0 | 0 | `surface` | the bar |
| 1 | 1 | `surface_container_low` | |
| 2 | 3 | `surface_container` | a panel |
| 3 | 6 | `surface_container_high` | a card |
| 4, 5 | 8, 12 | `surface_container_highest` | a row, and a well, as a filled field's container |

A control floating over a row is on `surface_bright`. Panels, cards and wells
carry no border, texture, wash or lit edge, so the theme sets those to clear
and their opacities to zero.

To make this possible the contract lets a theme supply a derived material
colour in place of its derivation (`suppliable` in `theme-contract.json`): a
theme that supplies none draws exactly the translucent material every preset
draws.

## State and selection

Hover is `on_surface` at Material's hover state layer, 8%; a press is the
primary at the pressed layer, 10%; disabled content is at 38%. Selection is
`secondary_container`, activity `primary_container`, and a level's fill the
primary container, because its label and value sit on it in the surface's
content colour. Danger is `error_container`, with the hover and pressed layers of
`on_error_container` flattened onto it; success is the green container.

## Type

Each step of the shell's type ramp takes the style of Material's type scale
that plays its role, at Material's size in pixels (1sp = 1px):

| Slot | Style | Size |
| --- | --- | --- |
| `textMicro` | label small | 11 |
| `textCaption` | body small | 12 |
| `textLabel` | label small | 11 |
| `textBody` | body medium | 14 |
| `textStrong` | title small | 14 |
| `textLead`, `textSubhead` | title medium | 16 |
| `textTitle` | title large | 22 |
| `textDisplay` | headline small | 24 |
| `textCode` | headline large | 32 |
| `textHero` | display small | 36 |
| `textIcon` | a button's and a chip's icon | 18 |
| `textCard` | the standard icon | 24 |

Weights are the scale's: regular for body, medium for labels, and the
Expressive scale's emphasized bold for a strong label or title. Tracking is
label large's, and prose is set at body medium's line height. Material lets a
brand choose its typeface, so these themes name none and the configured one
stands.

## Shape and spacing

The corner scale runs none 0, extra small 4, small 8, medium 12, large 16,
large increased 20, extra large 28, extra large increased 32, extra extra large
48 and full. Panels, cards and buttons take large, smaller controls small, and
the meeting planner's cells extra small and small. Strong outlines take the
focus indicator's 3 pixels.

Spacing keeps the shell's own ramp with every step on Material's 4dp grid
(rounded to the nearest multiple of four, a half step up), so a dense panel
keeps its rhythm.

## Components and their heights

Chips are 32, buttons and fields 40, two-line list items 72, switches 52 by 32,
plain tooltips 24 with 8 of padding, menu items 48, small icon buttons 40,
extra-small ones 32 in the bar, badges 6 and 16, linear and circular progress
4, list icons 24. A row keeps a small control's 40: the Control Center sets
three rows beside two levels, and its levels, the slider rows and the battery
rows keep the shell's heights so that column still adds up. The bar's elided
labels narrow to 120 to give back what the larger type takes, so the bar still
fits a 1920-pixel output with every indicator showing. Every other size is the
shell's own: Material has no token for a panel's width or a chart's height.

## Motion

Durations are Material's: short 3 (150 ms) for an in-surface change, medium 2
(300) for a standard transition, long 1 and 2 (450, 500) for a finish cue and a
gauge, long 4 and extra long 3 (600, 900) for a pulse. Curves are Material's
cubic Béziers, carried by the contract's `curve` slots beside each easing:
standard `(0.2, 0, 0, 1)` for every eased transition, emphasized accelerate
`(0.3, 0, 0.8, 0.15)` for a cue leaving, and the symmetric legacy standard
`(0.4, 0, 0.2, 1)` for a pulse.

The Expressive motion scheme's springs are part of the token set (`seele-theme
material tokens`), with damping ratio and stiffness: fast spatial 0.6 and 800,
default spatial 0.8 and 380, slow spatial 0.8 and 200, and fast, default and
slow effects 1.0 with 3800, 1600 and 800. No surface animates on a spring yet;
that is component behaviour (SIL-238), which can sample them into the same
`curve` slots.

## Where the values come from

Colour: Material Color Utilities, `typescript/` at revision
`dd62982dfdefdfc9232ce8d76e122fb1c6d188ae` (Apache-2.0). Type, shape,
elevation, state and timed motion: Material Web's system tokens
(`tokens/versions/latest`). Expressive springs: Compose Material 3's
`ExpressiveMotionTokens`. The values are compiled into
`config-tools/src/material/tokens.rs`, each slot naming the token it takes.
