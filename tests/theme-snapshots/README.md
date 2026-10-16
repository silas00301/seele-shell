# Theme snapshots

Pixel baselines of every existing theme on the shell's main surfaces, taken
before the theme system is reworked (SIL-232, ahead of SIL-233). The check
renders each theme on each surface and fails on any pixel that differs from
`baseline/`, so a refactor that is meant to change nothing can prove it.

## What is covered

Fourteen themes: the 13 presets of the theme switcher and `fallback`, the
palette the shell draws with before any `theme.json` is published
(`projects/shared/Palette.js`). Ten surfaces, each a production layer window
from `projects/shell/shell.qml`:

| Snapshot | Window |
| --- | --- |
| `bar` | the menu bar strip on a 1920×1080 output |
| `control-center` | Control Center |
| `notifications` | toasts |
| `notification-center` | the notification panel |
| `media` | Now Playing |
| `audio` | Audio, including the microphone test card |
| `session` | Power |
| `osd` | the volume level OSD |
| `theme-switcher` | the floating theme switcher |
| `theme-settings` | the Control Center's Themes panel |

The launcher is Vicinae, a separate application; its colours come from the
Stylix-generated theme that the parent flake's `checks.theme-presets` compares
byte for byte, so it is not captured here. The theme switcher is the shell's
own picker and is.

## How the scene is built

`tests/theme-snapshots.js` does not copy the shell. It lifts, from the
`shell.qml` being checked, every root property, function and inline component,
and each surface's whole `PanelWindow` with only its layer placement removed
(screen, anchors, margins, mask, `WlrLayershell.*`). Production files beside it
-- `SystemState.qml`, `ThemeStore.qml`, `ThemePanel.qml`, `MicTestStore.qml`
and the rest -- are staged unchanged apart from their Quickshell imports, and
the real `Seele.Core` plugin runs the native presentation and notification
policy. `shared/Theme.qml` loses only its config IO, as in the other Qt
fixtures; each theme reaches it through `Palette.assign`, the path
`theme.json` takes.

What Quickshell owns is replaced by `fixture/`: inert `Process`, `FileView`,
`Scope`, `StdioCollector` and `SplitParser` types, and Hyprland, Mpris, the
tray, desktop entries and icon lookup as fixed data. `scene.qml` holds the
stores' state on one fixed morning, notifications received through the
production native policy, and the render loop. Each frame is saved only once
two consecutive grabs, further apart than the longest transition, are
identical, and any Qt warning fails the run, because a binding the scene left
unsatisfied would draw a default instead of the theme.

`themes.json` is the `themes` array of `seele-theme list`: each preset's theme
document, as the contract defines it (`projects/shared/THEME-CONTRACT.md`),
and so exactly what the shell receives. It was projected by the real
`seele-theme` from the pinned `base16-schemes` (tinted-theming/schemes
`43dd14f6`) with the configured Catppuccin lavender accent. On nerv,
`--catalog` proves it still matches the installed catalog document for
document:

```sh
seele-theme list > /tmp/catalog.json
theme-snapshots --catalog /tmp/catalog.json
```

## Running it

`nix flake check` builds `checks.theme-snapshots`, and the development shell's
`test-shell` runs the same comparison after its workspace build. Neither shell
package build depends on it, so a rebuild never waits on a pixel comparison.
On its own, from the repository root:

```sh
nix develop -c theme-snapshots                 # compare
nix develop -c theme-snapshots --keep /tmp/ts  # also write actual, baseline and diff images
nix develop -c theme-snapshots --update        # re-record
```

Without Nix, pass the same inputs directly with Qt's `qmltestrunner` on `PATH`:

```sh
node tests/theme-snapshots.js projects/shell <Qt qml dir> <dir holding Seele/Core> \
  <Maple Mono NF CN 7.9 TTF dir> <grain.png from seele-grain> [--update] [--keep <dir>]
```

The plugin directory holds `Seele/Core` (`projects/qml` built against that Qt
and `libseele_qml_core.a`) and `Seele/Navigation` (`projects/navigation`), as
the `qml` package installs them together.

## The rendering environment

The baselines in this directory were rendered offscreen with Qt Quick's
software backend and the Fusion style by Qt 6.11.2 (the version nixpkgs pins)
and Maple Mono NF CN 7.9 from the same release archive nixpkgs fetches.
`manifest.json` records the renderer and the SHA-256 of every font file. The
check refuses any other font build outright, since moving every glyph would
bury a real change.

Text is rasterized unhinted and in greyscale through the harness's own
fontconfig file. Hinting is where FreeType releases disagree: with it, Ubuntu's
2.13.2 and nixpkgs' 2.14.3 differ by up to a few hundred pixels per surface,
all of them in text; without it, both reproduce every baseline exactly. Other
parts of the Qt build can still differ between machines; the failure message
names both renderers when they disagree. To check a change in such an
environment, record the unchanged base revision into a directory outside the
tree, then compare the change against it:

```sh
jj edit <base>   && nix develop -c theme-snapshots --update --baseline /tmp/theme-base
jj edit <change> && nix develop -c theme-snapshots --baseline /tmp/theme-base
```

Re-record the committed baselines only for a change whose new look is
intended, and review every changed image.
