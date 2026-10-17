# Cyberpunk

`cyberpunk` is a drop-in theme folder that draws the shell as the software
inside a cyberpunk city's machines might look: neon on near-black glass, panels
cut at the corners like machined plates, a trim along their top edge with a
slice of it knocked out of register, scanlines over every surface, and a squared
typeface set in capitals for headings. It is a theme like any other: every
preset keeps its look, and selecting it changes only the values the contract's
slots take.

Everything in the folder was made for it or is used under its licence (below).
No screenshot, logo, typeface, colour file or other asset of any game went into
it; the look is described in words and drawn from scratch.

## Colour

Near-black ground, red neon for structure, cyan for anything that has state,
yellow for warnings. Each role clears the legibility floor the presets are held
to (subtext 4.5:1, quiet marks 3:1, against `base`).

| Role | Colour | Contrast on `base` | Drawn as |
| --- | --- | --- | --- |
| `base` | `#0b0c12` | | the ground, and a panel's glass at 93% |
| `mantle` | `#0f111a` | | |
| `crust` | `#050508` | | wells |
| `surface` | `#181c28` | | cards and rows, as `#131722` |
| `overlay` | `#626b82` | 3.7:1 | quiet glyphs and section labels |
| `text` | `#e6f6f7` | 17.6:1 | primary text |
| `subtext` | `#8fb8c2` | 9.1:1 | secondary text |
| `accent` | `#00e5ff` | 12.7:1 | state: selection, fills, levels, focus, the glitch |
| `red` | `#ff3355` | 5.5:1 | danger, and every panel's frame and trim |
| `green` | `#1ef2a0` | 13.3:1 | success |
| `yellow` | `#f3e600` | 15.0:1 | warnings |

Panels and the bar are framed in red at 90%, cards at 30% and rules at 25%.
Selection, hover and press are cyan washes (20%, 10%, 30%), and a level's fill
is cyan at 55%. Neither panels nor the bar carry a wash or a lit edge: their
glass is flat and the frame does the work. The Base16 palette the other
applications take adds orange `#ff8a3d`, a light cyan `#7af7ff`, magenta
`#ff2bd6` and rust `#b3471f`, with `accent` as its blue.

## Type

[Chakra Petch](https://github.com/cadsondemak/Chakra-Petch), a squared sans
whose letters are cut at the corners as the panels are. The folder carries its
Light, Regular, Medium and SemiBold faces as the contract's four typeface files,
so the theme draws the same on any machine without installing anything; the
shell loads them when the theme is selected. Panel titles and section labels are
set in capitals (`titleCapitalization`, `labelCapitalization`), labels tracked
at 2.

Chakra Petch sits smaller than the monospace face the presets use, so the small
end of the ramp is a pixel larger: micro 9, caption 10, label 11, body 12, strong
13, lead 14. Titles and above keep the shell's sizes.

The faces are the designer's own build. The copy Google Fonts distributes has
its TrueType hinting stripped, and Qt's default full hinting then lets
FreeType's autohinter round every advance at caption sizes, opening gaps inside
words ("Ta ils ca le"); the hinted build sets the same text evenly.

## Shape

Corners are square: `radius`, `radiusSmall` and the meeting planner's radii are
0. Panels cut their top left and bottom right corners 14 pixels along each edge
(`panelCut`). The top cut is lit in red, a 3-pixel trim runs 64 pixels along the
top edge from it (`panelTrim`, `strokeStrong`), and past its end a 22-pixel
cyan glitch sits one trim below the edge (`panelGlitch`, `panelGlitchColor`).
Emphasised outlines take the same 3 pixels.

## Texture

`scanlines.png` is a 4×3 tile, one white row in three, laid over every surface
at 5% in place of the grain film (`surfaceTexture`, `grainOpacity`). On a cut
panel it stops half the cut inside the edge, where the cut would clip it.

## Motion

The panels move only when something happens to them. As a panel appears, its
trim sweeps out from the lit cut to its full 64 pixels over 260 ms on the
standard curve (`durationPanelTrim`); the glitch waits for it, then flickers
three times over 240 ms (`panelGlitchFlickers`, `durationPanelGlitch`), each
flicker knocking it 6 pixels one way, blanking it, and knocking it 6 the other
(`panelGlitchShift`). When the pointer comes onto a panel the glitch flickers
again. Both come to rest exactly where the still frame draws them, and nothing
runs while a panel is left alone, so an open panel draws no frame at all.

The motion is the theme's, and the user may hold it still: **Motion** under
Effects in the Themes panel, or `seele-theme motion off`, draws the still frame
at once, including a sweep already under way.

## Sound

Six cues, synthesised for the theme, sound only once the user turns them on
(**Sounds** under Effects, or `seele-theme sound on`); sound is off until then.
Presentation mode holds them all back, as it holds back the toasts.

| Cue | File | When | Sound |
| --- | --- | --- | --- |
| `soundNotification` | `sounds/notification.wav` | a new toast | two rising blips, 200 ms |
| `soundNotificationUrgent` | `sounds/notification-urgent.wav` | a new critical toast | three hard pulses, 420 ms |
| `soundFocusDone` | `sounds/focus-done.wav` | the focus timer runs out | a rising arpeggio onto the octave, 640 ms |
| `soundPanelOpen` | `sounds/panel-open.wav` | a panel opens, or another takes its place | static, then a sweep up, 110 ms |
| `soundPanelClose` | `sounds/panel-close.wav` | the last panel closes | a sweep down, 100 ms |
| `soundLevelStep` | `sounds/level-step.wav` | a volume, microphone or zoom OSD steps | one tick, 35 ms |

A toast that only comes back -- a reminder, a pin, the end of focus -- is not a
new arrival and stays silent, and so does one whose sender asks for silence with
the notification specification's `suppress-sound` hint, as the focus timer's own
notification does, since `soundFocusDone` has already sounded for it. A panel
swapped for another plays only the open cue: the shell closes one and opens the
next in the same turn, and the change is judged once that turn is over. Each file is a 16-bit mono WAV at 48 kHz,
PipeWire's own rate, peaking between -13 and -22 dBFS and ending on silence;
Qt plays it as an `event` stream, which the Audio panel's mixer leaves out.

## Wallpaper

`wallpaper.png` (3840×2160): a night city under a magenta haze, a few neon
signs, a cyan perspective grid on the ground, HUD brackets whose top left one
is cut and trimmed as the panels are, a barcode block, a tick ruler, a handful
of glitch bands with red and blue pulled apart, and scanlines. While the theme
is selected the desktop and the lock screen draw it, unless the host's
wallpaper mapping says otherwise.

## Assets and licences

| File | Source | Licence |
| --- | --- | --- |
| `fonts/ChakraPetch-Light.ttf`, `-Regular`, `-Medium`, `-SemiBold` | [cadsondemak/Chakra-Petch](https://github.com/cadsondemak/Chakra-Petch) `fonts/` at `3fc2b8f9443f871cc431ec4feb7c1a538c30e634`, unmodified | SIL Open Font License 1.1, `fonts/OFL.txt`; no Reserved Font Name |
| `wallpaper.png`, `scanlines.png` | drawn by `art/draw.js` from rectangles, lines and one seeded random sequence | EUPL-1.2, with this repository |
| `sounds/*.wav` | synthesised by `art/sounds.js` from oscillators, an envelope, a bit crusher and one seeded noise sequence | EUPL-1.2, with this repository |

The fonts' SHA-256 sums, as fetched:

```
007a71793288c1dbf5913e5acd90dae22b76363f07d4a029779c9ade8b9da035  ChakraPetch-Light.ttf
bfaff92c373d2365e33701c0fa1fa0fa2475486af77c83dbc4f8dadab5207db3  ChakraPetch-Regular.ttf
0c08fcaedf812a509037ee4246703b55ee1a834150d6b1333e680497cb76fd6f  ChakraPetch-Medium.ttf
155c3449755f44172a1b6b565376fc2bf4ccd2ead5437079b2fef97b89f665d1  ChakraPetch-SemiBold.ttf
```

`node themes/cyberpunk/art/draw.js themes/cyberpunk` draws both images again,
and `node themes/cyberpunk/art/sounds.js themes/cyberpunk` writes every sound
again, byte for byte; change the scripts, not the files. `tests/theme-sounds.js`
checks the sounds are what the script writes.

## Reference renders

`tests/theme-snapshots/baseline/cyberpunk/` holds the theme on the ten main
surfaces: the bar, the Control Center, toasts, the notification panel, Now
Playing, Audio, Power, the volume OSD, the theme switcher and the Themes panel.
They are the reference the look is checked against: `theme-snapshots` fails on
any pixel that moves, as it does for every preset. `tests/tst_palette.qml`
applies the folder as it is registered and checks that its typeface loads, its
headings go to capitals, its panels are cut and trimmed and its scanlines stand
in for the grain, that a frame as small as a tooltip draws nothing outside
itself and a tooltip's surface stays uncut, and that a preset afterwards takes
all of it back. Each surface is captured once its motion has come to rest,
which is the still frame. `tests/tst_motion.qml` drives the motion itself: the
sweep and the flicker on appearing, the flicker under the pointer, rest on
hiding, a panel held still, a frame too small for its trim, and no frame drawn
at rest. `tests/tst_themesounds.qml` checks when the cues play.

## What is not here

Toasts, cards and tooltips are square rather than cut, and do not move: the cut
and its motion belong to panels, the surfaces that frame everything else. A
frame smaller than its trim would need shrinks the cut, the trim and the glitch
to fit rather than drawing outside itself, and its motion sweeps and jolts
within that. There is no idle animation: a glitch that flickered on its own
would cost a frame every time, on a panel nobody is looking at.
