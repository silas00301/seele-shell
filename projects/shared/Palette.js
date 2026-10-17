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

// The images a published theme gives particular outputs, by the name the
// compositor gives each one: its `wallpaperOutputs`, keeping only absolute
// paths. A theme without them gives none, so nothing an earlier theme mapped
// outlives it.
function outputImages(theme) {
  var result = {}
  var outputs = theme && theme.wallpaperOutputs
  if (!outputs || typeof outputs !== "object" || Array.isArray(outputs)) return result
  Object.keys(outputs).forEach(function(output) {
    if (typeof outputs[output] === "string" && outputs[output].charAt(0) === "/") result[output] = outputs[output]
  })
  return result
}

// What the output named `output` draws: its own image from `outputs`, else
// `fallback`, the image every other output draws.
function imageFor(outputs, output, fallback) {
  var own = outputs && output && Object.prototype.hasOwnProperty.call(outputs, output) ? outputs[output] : ""
  return typeof own === "string" && own !== "" ? own : fallback
}
