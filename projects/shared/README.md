# Shared Qt scene components

`Theme.qml` is the theme contract's implementation and `theme-contract.json`
its schema; see `THEME-CONTRACT.md`. A component here takes `theme` and reads
every colour, size, opacity and duration from it.

`KeyboardNavigation.qml` and `ActionArea.qml` provide the shared keyboard
contract without changing a control's material or layout. See the
[navigation guide](../navigation/README.md) for bindings, continuous gestures,
text-entry boundaries, packaging and real Qt checks.

`Palette.js` is the sole fallback palette and the theme assignment module used
by lock, greeter and polkit. This small JavaScript file is required by Qt's
property binding API; it runs no services, subprocesses or background work.
Palette values match the original scenes exactly. Theme assignment retains
existing values for missing/falsy keys, skips colors unused by a scene, and only
applies wallpaper for the lock and greeter. Greeter identity/session settings
remain owned by the greeter.

`Theme.qml` applies a published `theme.json` itself, through `applyTheme` and
`Contract.js`: it reads `theme-contract.json` beside it once, then sets every
slot a theme may supply to the theme's value or, where the theme leaves one out,
the default, so nothing an earlier theme set outlives it. The wallpaper is
among them: a published theme names the image the host's wallpaper mapping
chooses for it (its own, or the configured one, unless the host maps another),
`wallpaperOutputs` names the outputs that draw a different image, and before
any publication the shell keeps its `SEELE_SHELL_WALLPAPER` selection.
`wallpaperFor(<output name>)` is the image an output draws, and
`Palette.outputImages` and `Palette.imageFor` give the lock screen the same
lookup. `Wallpaper.qml` draws it for each output's desktop background and keeps
the picture on screen until the next has loaded. Without the contract file it falls back to
`Palette.assign`.

Every package copies the module beside its consumer. The desktop shell and Notes
copy shared QML and JavaScript assets together, with `theme-contract.json`.
`tests/tst_palette.qml` also applies themes through the real contract: supplied
slots take effect, weights and easings arrive by name, a value of the wrong kind
leaves the default, and a later theme restores every default. Native unit/protocol tests do not
replace Qt checks: `tests/palette.js` fixes the original fallback/property
contract, and `tests/tst_palette.qml` compares actual Qt-rendered default and
updated color swatches, including their alpha values. Layout, derived material
colors and motion tokens are slots of the theme contract in `Theme.qml`.

`HoverWash.qml` is a control's state layer and `Ripple.qml` its press ripple,
`FocusRing.qml` the keyboard's ring; with `ActionButton`, `GlyphButton`,
`IconButton`, `Segment`, `ChoiceBox` and `ControlSwitch` they take their state
timing, ripple, pressed corner, focus ring, spring curves and switch handle from
the contract's slots, every one of which defaults to the behaviour these
controls had before a theme could set it (`THEME-CONTRACT.md`, "Component
behaviour"). Pass a control's press and where it landed to `HoverWash` rather
than drawing a press of its own. `tests/tst_behaviour.qml`, run by
`tests/palette.sh`, drives them under a preset and under Material Expressive.

`PanelSurface.qml` is every panel's material. For a theme that cuts panel
corners (`panelCut`), it loads `CutFrame.qml` in place of its rounded
rectangle: the outline with its two corners cut, and the trim and glitch a
theme may give it (`THEME-CONTRACT.md`, "Cut panels and capitals"), each held
inside the frame however small it is. A theme that gives no cut never loads
it, and a surface that is not a panel, such as `GlyphButton`'s tooltip, passes
`cutCorners: false` and keeps the plain material. The frame also plays the
theme's own motion -- the trim sweeping in as the panel appears, the glitch
flickering after it and under the pointer -- and rests on the still frame when
the panel is hidden, whether its window is unmapped or only its surface inside
a mapped one, and when the user holds motion still (`motionEnabled`);
`tests/tst_motion.qml`, run by `tests/palette.sh`, drives it.

`HistoryChart.qml` renders bounded native histories without owning samples or
policy. Pass `theme`, `series: [{values: [number|null], color}]`, `capacity`
(default 60), and `maximum` (default 100). Samples occupy a fixed right-aligned
window. Missing/non-finite values break strokes; finite values clamp to the
scale. CPU/memory and network inspection share this component.

`DeviceSlider.qml` is the shared level track for Home Assistant devices and
the Camera panel's Litra Glow. `current` is the device's value. Only pointer
and keyboard input emits `committed`, and pointer movement emits `changing`
for a device cheap enough to follow the drag; an incoming value update never
writes back to the device. A level that sets a colour passes `spectrum`, the
colours at the track's start, middle and end; colour-temperature tracks use
the theme's `temperatureSpectrum`, so the well shows the range and the fill
ends on the chosen colour.
