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
