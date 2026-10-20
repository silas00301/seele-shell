# Drop-in themes

A theme beyond the Base16 presets is a folder here and one line in
`default.nix`:

```nix
[
  ./example
]
```

The flake then checks the folder when it builds, renders the desktop's
application themes from it, and adds it to the theme switcher, the Control
Center's Themes panel and Vicinae's Seele Themes after the presets. No shell
code changes.

## The folder

```
example/
  theme.json
  wallpaper.jpg     # the theme's own background, a PNG or a JPEG
  sounds/ping.oga   # only what theme.json names is registered
```

`theme.json` is a document of the theme contract
(`projects/shared/THEME-CONTRACT.md`):

- `id`, the folder's name; `name`, as the switcher shows it; `mode`, `light` or
  `dark`.
- The eleven palette roles, each an opaque `#rrggbb`: `base`, `mantle`, `crust`,
  `surface`, `overlay`, `text`, `subtext`, `accent`, `red`, `green`, `yellow`.
- `palette`: the sixteen Base16 colours `base00` to `base0F`, as lowercase
  `#rrggbb`, which GTK, Qt, KDE, Zen, Spicetify, Vicinae, Neovim and the
  terminal are themed from.
- Any other slot the contract lets a theme supply, such as `"radius": 14` or
  `"weightStrong": "Bold"`. Everything left out keeps its default.
- A file slot, such as `soundNotification`, names a file relative to the folder.
- `wallpaper`, optional, names the theme's own background image: a complete
  PNG or JPEG of at most 16 MiB on disk and 256 MiB decoded (8192 × 8192 at
  most for a square image). A file Qt could not draw fails the build. While
  the theme is selected the desktop and the lock screen draw it, cropped to
  fill each output; a theme without one keeps the configured wallpaper. Use
  only images you made or may redistribute. Which image each output shows, per
  light or dark theme or for this theme in particular, is the host's wallpaper
  mapping, declared in the parent flake's Nix (`seele.wallpaper`), because
  output names belong to a machine; a folder cannot carry `wallpaperOutputs`.

`tests/theme-folders/example` is a complete folder, and
`tests/theme-folders/pictured` one with its own wallpaper.

## When a folder is wrong

The build that installs it fails and says why, every rule at once:

```
Seele Themes: Invalid theme folder example:
  palette.base0F: missing
  missing required slot accent
  radius: "round" is not a int
```

`nix build .#checks.<system>.theme-folders` checks every folder listed here
without building a host.
