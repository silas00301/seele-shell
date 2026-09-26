# Shared Qt scene components

`Palette.js` is the sole fallback palette and theme assignment module used by
`Theme.qml`, lock, greeter and polkit. This small JavaScript file is required by
Qt's property binding API; it runs no services, subprocesses or background work.
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

`DeviceSlider.qml` is the shared level track for Home Assistant devices and
the Camera panel's Litra Glow. `current` is the device's value. Only pointer
and keyboard input emits `committed`, and pointer movement emits `changing`
for a device cheap enough to follow the drag; an incoming value update never
writes back to the device. A level that sets a colour passes `spectrum`, the
colours at the track's start, middle and end; colour-temperature tracks use
the theme's `temperatureSpectrum`, so the well shows the range and the fill
ends on the chosen colour.
