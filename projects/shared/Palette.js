.pragma library

// The small Qt binding boundary shared by every Seele scene. Native services
// never load this module; Qt owns color conversion and property assignment.
var fallback = Object.freeze({
  base: "#1e1e2e", mantle: "#181825", crust: "#11111b",
  surface: "#313244", overlay: "#6c7086", text: "#cdd6f4",
  subtext: "#a6adc8", accent: "#b4befe", red: "#f38ba8",
  green: "#a6e3a1", yellow: "#f9e2af", fontFamily: "Maple Mono NF CN",
  wallpaper: "/etc/wallpaper/wallpaper.jpg"
})

// Missing/falsy values retain the current property, including an earlier
// theme assignment. Some auth clients use a smaller palette; their absent
// properties are skipped. Wallpaper remains an explicit consumer opt-in.
function assign(target, theme, wallpaper) {
  if (!theme || typeof theme !== "object" || Array.isArray(theme)) throw Error("Invalid theme")
  Object.keys(fallback).forEach(function(key) {
    if (key === "wallpaper" && !wallpaper) return
    if (typeof target[key] !== "undefined" && Object.prototype.hasOwnProperty.call(theme,key) && theme[key])
      target[key] = theme[key]
  })
}

// Material 3 colour roles, derived from the eleven palette colours a theme
// carries. Material names a colour by what it does -- the container a control
// sits on and the colour drawn on it -- and every Seele client takes those
// roles from this one derivation, so the shell, Notes and the auth clients
// cannot drift apart. A role is the palette's own colour where one plays it,
// and otherwise a tone of a palette colour laid over the theme's base, which
// is how Material's tonal containers read on a dark and a light scheme alike.
// Colours arrive as `#rrggbb` strings or as Qt colours, and leave as
// `#aarrggbb` strings either side of the Qt boundary accepts.
function channels(color) {
  if (typeof color === "string") {
    var value = color.replace("#", "")
    if (value.length === 6) value = "ff" + value
    if (!/^[0-9a-fA-F]{8}$/.test(value)) throw Error("Invalid colour " + color)
    return {
      a: parseInt(value.slice(0, 2), 16) / 255, r: parseInt(value.slice(2, 4), 16) / 255,
      g: parseInt(value.slice(4, 6), 16) / 255, b: parseInt(value.slice(6, 8), 16) / 255
    }
  }
  return { r: color.r, g: color.g, b: color.b, a: color.a === undefined ? 1 : color.a }
}

function hex(color) {
  function byte(value) { return ("0" + Math.round(Math.max(0, Math.min(1, value)) * 255).toString(16)).slice(-2) }
  return "#" + byte(color.a) + byte(color.r) + byte(color.g) + byte(color.b)
}

// `amount` of `color` laid over an opaque `base`.
function tone(base, color, amount) {
  var under = channels(base)
  var over = channels(color)
  return hex({
    a: 1, r: under.r + (over.r - under.r) * amount,
    g: under.g + (over.g - under.g) * amount, b: under.b + (over.b - under.b) * amount
  })
}

function withAlpha(color, opacity) {
  var value = channels(color)
  value.a = opacity
  return hex(value)
}

// The relative luminance WCAG and Material's contrast rules are written in.
function luminance(color) {
  var value = channels(color)
  function linear(channel) { return channel <= 0.03928 ? channel / 12.92 : Math.pow((channel + 0.055) / 1.055, 2.4) }
  return 0.2126 * linear(value.r) + 0.7152 * linear(value.g) + 0.0722 * linear(value.b)
}

// The palette's own ink that reads on `color`: its darkest step on a light
// fill and its lightest on a dark one, chosen by contrast rather than by
// mode, so a pale accent on a light theme still takes dark content.
function inkOn(color, palette) {
  var dark = luminance(palette.crust) < luminance(palette.text) ? palette.crust : palette.text
  var light = luminance(palette.base) > luminance(palette.text) ? palette.base : palette.text
  var fill = luminance(color)
  var againstDark = (fill + 0.05) / (luminance(dark) + 0.05)
  var againstLight = (luminance(light) + 0.05) / (fill + 0.05)
  return hex(channels(againstDark >= againstLight ? dark : light))
}

function roles(palette) {
  var dark = luminance(palette.base) < luminance(palette.text)
  var base = palette.base
  var text = palette.text
  var highest = tone(base, text, 0.12)
  // Containers step from the wallpaper side towards the text: lighter on a
  // dark scheme, darker on a light one, so `surfaceContainerHigh` is a card
  // on either. The lowest step is the palette's own ink on a dark scheme and
  // close to white on a light one.
  return {
    darkScheme: dark,
    primary: hex(channels(palette.accent)),
    textOnPrimary: inkOn(palette.accent, palette),
    primaryContainer: tone(base, palette.accent, dark ? 0.3 : 0.24),
    textOnPrimaryContainer: tone(palette.accent, text, dark ? 0.45 : 0.65),
    // The accent with its chroma taken down: what is selected without being
    // the one thing on, and where the keyboard is.
    secondary: tone(palette.accent, palette.subtext, 0.4),
    secondaryContainer: tone(highest, palette.accent, dark ? 0.16 : 0.2),
    textOnSecondaryContainer: tone(text, palette.accent, 0.2),
    error: hex(channels(palette.red)),
    textOnError: inkOn(palette.red, palette),
    errorContainer: tone(base, palette.red, dark ? 0.28 : 0.2),
    textOnErrorContainer: tone(palette.red, text, dark ? 0.4 : 0.6),
    // Material leaves success and warning to an application's own custom
    // colours. They take the error's container treatment, so a finished
    // transfer and a failed one are drawn the same way in different hues.
    successContainer: tone(base, palette.green, dark ? 0.24 : 0.22),
    textOnSuccessContainer: tone(palette.green, text, dark ? 0.35 : 0.6),
    warningContainer: tone(base, palette.yellow, dark ? 0.22 : 0.3),
    textOnWarningContainer: tone(palette.yellow, text, dark ? 0.35 : 0.65),
    outline: hex(channels(palette.overlay)),
    outlineVariant: tone(base, text, 0.18),
    inverseSurface: hex(channels(text)),
    inverseOnSurface: hex(channels(base)),
    scrim: withAlpha(palette.crust, 0.5),
    surfaceContainerLowest: dark ? hex(channels(palette.crust)) : tone(base, "#ffffff", 0.6),
    surfaceContainerLow: dark ? hex(channels(palette.mantle)) : hex(channels(base)),
    surfaceContainer: tone(base, text, 0.035),
    surfaceContainerHigh: tone(base, text, 0.075),
    surfaceContainerHighest: highest
  }
}
