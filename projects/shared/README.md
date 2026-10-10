# Shared Qt scene components

The shared set draws Material 3 Expressive. `Theme.qml` holds the tokens: the
palette, the colour roles, the corner scale, tonal elevation, state layers and
springs. The `seele-style` skill in the parent repository is the guide to which
token and part a surface takes.

`Palette.js` is the sole fallback palette, theme assignment and colour-role
module used by `Theme.qml`, lock, greeter and polkit. `roles(palette)` derives
Material's roles — the primary and its container, secondary, error, success and
warning containers with their `textOn…` content colours, the outlines, the
inverse surface and the surface container ramp — from the eleven palette
colours, stepping towards the text on a dark and a light scheme alike, and
returns `#aarrggbb` strings. `Motion.js` samples Material's Expressive springs
into eight-segment Bézier splines for Qt's `BezierSpline` easing, which corrupts
memory past ten segments. `Shapes.js` is the Expressive shape library as radii
sampled around a centre, so any two shapes morph. These small JavaScript files
are required by Qt's property binding and Canvas APIs; they run no services,
subprocesses or background work.
Palette values match the original scenes exactly. Theme assignment retains
existing values for missing/falsy keys, skips colors unused by a scene, and only
applies wallpaper for the lock and greeter. The main shell retains its existing
wallpaper environment selection. Greeter identity/session settings remain owned
by the greeter.

Every package copies the module beside its consumer. The desktop shell and Notes
copy shared QML and JavaScript assets together. Native unit/protocol tests do not
replace Qt checks: `tests/palette.js` fixes the original fallback/property
contract, and `tests/tst_palette.qml` compares actual Qt-rendered default and
updated color swatches, including their alpha values. Layout, derived material
colors and motion tokens remain owned by the existing scenes.

`HistoryChart.qml` renders bounded native histories without owning samples or
policy. Pass `theme`, `series: [{values: [number|null], color}]`, `capacity`
(default 60), and `maximum` (default 100). Samples occupy a fixed right-aligned
window. Missing/non-finite values break strokes; finite values clamp to the
scale. CPU/memory and network inspection share this component.

`LevelTrack.qml` draws a level as Material 3 Expressive's large slider: the
filled part, an upright handle and the rest of the track with gaps between,
the name and reading inside it in the ink of whichever part they sit on. It
draws only; the owner keeps input. `DeviceSlider.qml` is the shared level for
Home Assistant devices and the Camera panel's Litra Glow, a Qt Slider drawn on
`LevelTrack`. `current` is the device's value. Only pointer
and keyboard input emits `committed`, and pointer movement emits `changing`
for a device cheap enough to follow the drag; an incoming value update never
writes back to the device. A level that sets a colour passes `spectrum`, the
colours at the track's start, middle and end; colour-temperature tracks use
the theme's `temperatureSpectrum`, so the rest of the track shows the range and
the fill ends on the chosen colour.

`MaterialShape.qml` fills one of the library's shapes and morphs to another on
the fast spatial spring. `LoadingIndicator.qml` is Material's loading indicator,
seven shapes morphing as it turns; it needs no theme, so the lock and greeter
copy it with `Motion.js` and `Shapes.js`. `RefreshGlyph.qml` shows it while work
is live.
