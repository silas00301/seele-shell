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

To write one, follow `AUTHORING.md`: copy `template/`, a complete dark theme
that no line here ships, choose its colours, and check and look at it with
`theme-check` and `theme-preview` in the development shell. It takes under an
hour.

## The folder

```
example/
  theme.json
  wallpaper.jpg     # the theme's own background, a PNG or a JPEG
  fonts/Face.ttf    # the typeface it names, one TrueType or OpenType file a weight
  texture.png       # tiled over every surface in place of the grain film
  sounds/ping.wav   # only what theme.json names is registered
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
- The `sound…` slots are the theme's interface cues: a toast, a critical
  toast, the focus timer running out, a panel opening and closing, and a level
  OSD stepping. Each is a WAV file, the one format Qt's SoundEffect plays, kept
  short and well below full scale. The shell plays them only once the user has
  turned sounds on (`seele-theme sound on`, or Effects in the Themes panel).
- `fontFileLight`, `fontFileRegular`, `fontFileMedium` and `fontFileStrong`,
  optional, carry the typeface `fontFamily` names, one TrueType or OpenType
  file for each of the contract's weights the theme sets text in, so it draws
  the same on any machine. Ship only typefaces whose licence allows it, with
  that licence beside them. `surfaceTexture`, optional, is tiled over every
  surface at `grainOpacity` in place of the grain film; it is held to the
  wallpaper's rules below.
- `wallpaper`, optional, names the theme's own background image: a complete
  PNG or JPEG of at most 16 MiB on disk and 256 MiB decoded (8192 × 8192 at
  most for a square image). A file Qt could not draw fails the build. While
  the theme is selected the desktop and the lock screen draw it, cropped to
  fill each output; a theme without one keeps the configured wallpaper. Use
  only images you made or may redistribute. Which image each output shows, per
  light or dark theme or for this theme in particular, is the host's wallpaper
  mapping, declared in the parent flake's Nix (`seele.wallpaper`), because
  output names belong to a machine; a folder cannot carry `wallpaperOutputs`.

`template/` is the folder to copy, `tests/theme-folders/example` is a complete
folder the checks use, and `tests/theme-folders/pictured` one with its own
wallpaper.

## Material 3 Expressive

`material-expressive-light` and `material-expressive-dark` are grown from one
seed colour by `seele-theme material <seed> <light|dark> <id> <name>
[contrast]`, which writes the whole document: Material's colour scheme, type
scale, corner scale, tonal elevation, state layers and timed motion, each on
the contract slot that plays its role. Such a document records its source as
`material`, and registration refuses it if it is not exactly what that seed
grows. `MATERIAL.md` lists every token and the slot it reaches.

## Cyberpunk

`cyberpunk` draws the shell as in-game software: neon on near-black glass,
panels cut at two corners with a trim and a glitch along their top edge
(`panelCut`, `panelTrim`, `panelGlitch`), scanlines for a texture, square
corners everywhere else, and its own typeface, Chakra Petch, with headings in
capitals. As a panel appears its trim sweeps out from the cut and its glitch
flickers, and the glitch flickers again when the pointer comes onto the panel;
it carries six interface sounds as well, played only once sounds are turned on.
Its fonts are used under the SIL Open Font License, its wallpaper and scanlines
are drawn by `cyberpunk/art/draw.js` and its sounds synthesised by
`cyberpunk/art/sounds.js`; `CYBERPUNK.md` lists every value it sets and where
each file comes from.

## When a folder is wrong

The build that installs it fails and says why, every rule at once:

```
Seele Themes: Invalid theme folder example:
  palette.base0F: missing
  missing required slot accent
  radius: "round" is not a int
```

A Material folder edited by hand fails the same way, naming what differs:

```
Seele Themes: Invalid theme folder material-drift:
  material: theme.json is not what its seed grows (radius: 9 here, 16 from its seed); `seele-theme material` writes it again
```

`nix build .#checks.<system>.theme-folders` checks every folder listed here
without building a host.
