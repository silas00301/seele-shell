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

`tests/theme-folders/example` is a complete folder.

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
