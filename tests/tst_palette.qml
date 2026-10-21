import QtQuick
import QtTest
import "../projects/shared" as Shared
import "../projects/shared/Contract.js" as Contract
import "../projects/shared/Palette.js" as Palette

TestCase {
  id: testCase
  name: "PaletteParity"
  when: windowShown
  visible: true
  width: 450
  height: 150
  readonly property var keys: ["base","mantle","crust","surface","overlay","text","subtext","accent","red","green","yellow"]
  readonly property var alphas: [1,0.88,0.5,0.3,0]
  Shared.Theme { id: subject }
  // Two outputs' backgrounds, bound as shell.qml binds each one.
  readonly property string wideImage: subject.wallpaperFor("DP-1")
  readonly property string sideImage: subject.wallpaperFor("HDMI-A-1")
  QtObject {
    id: baseline
    property color base: "#1e1e2e"
    property color mantle: "#181825"
    property color crust: "#11111b"
    property color surface: "#313244"
    property color overlay: "#6c7086"
    property color text: "#cdd6f4"
    property color subtext: "#a6adc8"
    property color accent: "#b4befe"
    property color red: "#f38ba8"
    property color green: "#a6e3a1"
    property color yellow: "#f9e2af"
  }
  component Swatches: Item {
    required property var colors
    width: 220
    height: 100
    Repeater {
      model: 55
      Rectangle {
        required property int index
        width: 20; height: 20
        x: index % 11 * 20; y: Math.floor(index / 11) * 20
        color: { var value=parent.colors[testCase.keys[index % 11]]; return Qt.rgba(value.r,value.g,value.b,testCase.alphas[Math.floor(index / 11)]) }
      }
    }
  }
  Swatches { id: actual; colors: subject }
  Swatches { id: expected; colors: baseline; x: 230 }
  function init() { failOnWarning(/.?/) }
  function test_default_and_reassigned_rgba_pixels() {
    for (var key of keys) compare(subject[key],baseline[key])
    wait(50)
    verify(grabImage(actual).equals(grabImage(expected)), "fallback pixels including transparency must stay identical")
    var theme={base:"#102030",mantle:"#223344",accent:"#aabbcc",red:"#aa1133",green:"#55aa66"}
    Palette.assign(subject,theme)
    // The previous inline assignment semantics form the independent baseline.
    for (var name of keys) baseline[name]=theme[name] || baseline[name]
    wait(50)
    verify(grabImage(actual).equals(grabImage(expected)), "theme application preserves Qt color conversion and alpha")
    Palette.assign(subject,{base:"",accent:null})
    compare(subject.base,baseline.base)
    compare(subject.accent,baseline.accent)
    compare(subject.radius,8)
    compare(subject.durationFast,110)
    compare(subject.durationNormal,180)
    compare(subject.grainOpacity,0.07)
  }
  // The contract as Theme.qml's FileView reads it, beside Theme.qml.
  function contractSlots() {
    var request = new XMLHttpRequest(), slots = null
    request.onreadystatechange = function() {
      if (request.readyState === XMLHttpRequest.DONE) slots = Contract.slots(JSON.parse(request.responseText))
    }
    request.open("GET", Qt.resolvedUrl("../projects/shared/theme-contract.json"))
    request.send()
    tryVerify(function() { return slots !== null }, 5000, "the contract loads")
    return slots
  }
  function qmlValue(slot, value) {
    return slot.type === "weight" || slot.type === "capitalization" ? Font[value] : slot.type === "easing" ? Easing[value] : value
  }
  function test_a_published_theme_sets_every_slot_and_leaves_nothing_behind() {
    var slots = contractSlots()
    verify(slots.length > 300, "every suppliable slot")
    verify(slots.some(function(slot) { return slot.name === "wallpaper" }), "a theme's own wallpaper")
    verify(slots.every(function(slot) { return slot.name !== "wallpaperImage" && slot.name !== "mutedText" && slot.name !== "iosRed" }),
      "no computed or fixed slot")
    verify(slots.some(function(slot) { return slot.name === "panelColor" && slot.derived }), "a derived colour a theme may supply")
    var roles = {base:"#101820",mantle:"#0c1218",crust:"#080c10",surface:"#1c2a36",overlay:"#5c7080",text:"#e8f0f8",
      subtext:"#a8b8c8",accent:"#f2aa4c",red:"#ff6b6b",green:"#7bd88f",yellow:"#ffd866"}
    var folder = Object.assign({id:"example",name:"Example",mode:"dark",palette:{},terminal:[],version:2,
      wallpaper:"/elsewhere.jpg",radius:14,textBody:12,grainOpacity:0.1,weightStrong:"Bold",easingStandard:"OutBack",
      fontFamily:"Example Sans"}, roles)
    subject.applyTheme(folder, slots)
    compare(subject.radius, 14)
    compare(subject.textBody, 12)
    compare(subject.grainOpacity, 0.1)
    compare(subject.weightStrong, Font.Bold)
    compare(subject.easingStandard, Easing.OutBack)
    compare(subject.fontFamily, "Example Sans")
    compare(subject.accent, Qt.color("#f2aa4c"))
    compare(subject.wallpaper, "/elsewhere.jpg", "a theme's own wallpaper is applied")
    compare(subject.wallpaperImage, "/elsewhere.jpg", "and drawn")
    // A preset supplies its roles and typeface only: everything else returns
    // to the default, exactly as the contract and Theme.qml declare it.
    var preset = {id:"catppuccin-mocha",name:"Catppuccin Mocha",mode:"dark",fontFamily:"Maple Mono NF CN"}
    for (var key of keys) preset[key] = baseline[key].toString()
    subject.applyTheme(preset, slots)
    for (var slot of slots) {
      if (slot.derived) continue
      if (slot.type === "color") compare(subject[slot.name], Qt.color(preset[slot.name]), slot.name)
      else if (slot.type === "curve") compare(subject[slot.name].length, 0, slot.name)
      else compare(subject[slot.name], qmlValue(slot, slot.value), slot.name)
    }
    compare(subject.radius, 8)
    compare(subject.weightStrong, Font.DemiBold)
    compare(subject.easingStandard, Easing.OutCubic)
    compare(subject.wallpaperImage, subject.configuredWallpaper, "a theme without an image keeps the configured wallpaper")
    // The publication names the configured image for such a theme.
    subject.applyTheme(Object.assign({}, preset, {wallpaper: Palette.fallback.wallpaper}), slots)
    compare(subject.wallpaperImage, Palette.fallback.wallpaper)
    // A value of the wrong kind is never assigned; its default stands.
    subject.applyTheme(Object.assign({}, folder, {radius:"round",accent:"lavender",textBody:null,easingStandard:7,wallpaper:false}), slots)
    compare(subject.radius, 8)
    compare(subject.wallpaperImage, subject.configuredWallpaper)
    compare(subject.accent, Qt.color(Palette.fallback.accent))
    compare(subject.textBody, 11)
    compare(subject.easingStandard, Easing.OutCubic)
    compare(subject.weightStrong, Font.Bold)
    subject.applyTheme(preset, slots)
    compare(subject.grainOpacity, 0.07)
  }
  // A derived colour a theme supplies replaces its derivation, and the next
  // theme that leaves it out gets the derivation back.
  function test_a_supplied_derived_colour_replaces_its_derivation() {
    var slots = contractSlots()
    var theme = {id:"example",name:"Example",mode:"dark",cardColor:"#202224",panelBorder:"#00181a1c",hoverColor:"#14e6e1e5"}
    for (var key of keys) theme[key] = baseline[key].toString()
    subject.applyTheme(theme, slots)
    compare(subject.cardColor, Qt.color("#202224"))
    compare(subject.panelBorder, Qt.color("#00181a1c"))
    compare(subject.hoverColor, Qt.color("#14e6e1e5"))
    compare(subject.rowColor, subject.alpha(subject.surface, 0.3), "a derived colour it leaves out stays derived")
    delete theme.cardColor
    delete theme.panelBorder
    delete theme.hoverColor
    subject.applyTheme(theme, slots)
    compare(subject.cardColor, subject.alpha(subject.surface, 0.5))
    compare(subject.panelBorder, subject.alpha(subject.crust, 0.9))
    subject.applyTheme(Object.assign({}, theme, {cardColor: "ink"}), slots)
    compare(subject.cardColor, subject.alpha(subject.surface, 0.5), "a colour of the wrong form is not applied")
  }

  // Two animations that bind the same easing and curve in either order, as a
  // surface's NumberAnimation does.
  // A cut panel's material in the subject's theme, as PanelSurface lays it,
  // over a backdrop that shows where it draws nothing.
  Rectangle {
    id: panelHost
    x: 0; y: 110; width: 120; height: 40
    color: "#00ff00"
    Shared.CutFrame { theme: subject }
  }
  // A frame much smaller than a panel, such as a tooltip's, over a backdrop
  // wider than it: nothing of it may land on the backdrop beside it.
  Rectangle {
    id: narrowBackdrop
    x: 230; y: 110; width: 200; height: 40
    color: "#00ff00"
    Item {
      id: narrowHost
      width: 48; height: 22
      Shared.CutFrame { id: narrowFrame; theme: subject }
    }
  }
  Item { id: tipHost; width: 60; height: 24 }
  Component { id: tipSurface; Shared.PanelSurface { theme: subject; cutCorners: false } }
  NumberAnimation { id: curveFirst; easing.bezierCurve: subject.easingStandardCurve; easing.type: subject.easingStandard }
  NumberAnimation { id: typeFirst; easing.type: subject.easingStandard; easing.bezierCurve: subject.easingStandardCurve }
  function test_a_spline_easing_reaches_every_transition_in_either_order() {
    var slots = contractSlots()
    var theme = {id:"example",name:"Example",mode:"dark",easingStandard:"BezierSpline",easingStandardCurve:[0.2,0,0,1,1,1]}
    for (var key of keys) theme[key] = baseline[key].toString()
    var plain = Object.assign({}, theme)
    delete plain.easingStandard
    delete plain.easingStandardCurve
    subject.applyTheme(plain, slots)
    for (var animation of [curveFirst, typeFirst]) {
      compare(animation.easing.type, Easing.OutCubic, "a named curve while the theme gives none")
    }
    var named = curveFirst.easing.valueForProgress(0.1)
    fuzzyCompare(named, 1 - Math.pow(0.9, 3), 1e-9)
    subject.applyTheme(theme, slots)
    for (var animation of [curveFirst, typeFirst]) {
      compare(animation.easing.type, Easing.BezierSpline)
      compare(animation.easing.bezierCurve.length, 6)
      // Material's standard curve, cubic-bezier(0.2, 0, 0, 1), a tenth of
      // the way in: far from where OutCubic is.
      fuzzyCompare(animation.easing.valueForProgress(0.1), 0.1562, 0.005)
    }
    subject.applyTheme(plain, slots)
    // Qt keeps a spline's points on a curve whose type moves on, but a named
    // type computes its own curve, so they are inert.
    for (var animation of [curveFirst, typeFirst]) {
      compare(animation.easing.type, Easing.OutCubic, "and the named curve again after")
      fuzzyCompare(animation.easing.valueForProgress(0.1), named, 1e-9)
    }
  }

  // The Material 3 Expressive folders, applied as the shell applies a
  // published theme: every value they supply is the value the shell draws.
  function themeFolder(id) {
    var request = new XMLHttpRequest(), document = null
    request.onreadystatechange = function() {
      if (request.readyState === XMLHttpRequest.DONE) document = JSON.parse(request.responseText)
    }
    request.open("GET", Qt.resolvedUrl("../themes/" + id + "/theme.json"))
    request.send()
    tryVerify(function() { return document !== null }, 5000, id + " loads")
    return document
  }
  function test_a_material_theme_draws_every_surface_with_its_tokens() {
    var slots = contractSlots()
    for (var id of ["material-expressive-dark", "material-expressive-light"]) {
      var document = themeFolder(id)
      subject.applyTheme(document, slots)
      var supplied = 0
      for (var slot of slots) {
        if (!Object.prototype.hasOwnProperty.call(document, slot.name)) continue
        supplied += 1
        var value = document[slot.name]
        if (slot.type === "color") compare(subject[slot.name], Qt.color(value), id + " " + slot.name)
        else if (slot.type === "curve") compare(subject[slot.name].length, value.length, id + " " + slot.name)
        else if (slot.type === "real") fuzzyCompare(subject[slot.name], value, 1e-9, id + " " + slot.name)
        else compare(subject[slot.name], qmlValue(slot, value), id + " " + slot.name)
      }
      verify(supplied > 150, id + " supplies Material's tokens")
      compare(subject.grainOpacity, 0)
      compare(subject.easingStandard, Easing.BezierSpline)
      compare(curveFirst.easing.type, Easing.BezierSpline)
    }
  }

  // The Cyberpunk folder (SIL-239), registered as `seele-theme register`
  // resolves it: its own typeface loads, headings go to capitals, panels cut
  // their corners and tile scanlines, and a preset after it takes all of
  // that back without a warning.
  function test_the_cyberpunk_theme_brings_its_typeface_and_cuts_its_panels() {
    var slots = contractSlots()
    var document = themeFolder("cyberpunk")
    for (var slot of slots) {
      var name = document[slot.name]
      if (slot.type === "path" && typeof name === "string" && name !== "")
        document[slot.name] = Qt.resolvedUrl("../themes/cyberpunk/" + name).toString().replace(/^file:\/\//, "")
    }
    subject.applyTheme(document, slots)
    compare(subject.fontFamily, "Chakra Petch")
    tryVerify(function() { return Qt.fontFamilies().indexOf("Chakra Petch") >= 0 }, 5000, "the theme's own typeface is loaded")
    compare(subject.titleCapitalization, Font.AllUppercase)
    compare(subject.labelCapitalization, Font.AllUppercase)
    compare(subject.panelCut, 14)
    compare(subject.grain, "file://" + document.surfaceTexture.split("/").map(encodeURIComponent).join("/"),
      "scanlines stand in for the grain film, as a file URL")
    compare(subject.panelTrimColor, Qt.color(document.panelTrimColor))
    compare(subject.panelGlitchColor, Qt.color(document.panelGlitchColor))
    // Inside the cuts the panel draws nothing; on its edge, the neon border;
    // along the top, from the cut, the trim.
    var backdrop = Qt.color("#00ff00")
    tryVerify(function() { return Qt.colorEqual(grabImage(panelHost).pixel(3, 3), backdrop) }, 2000, "the top left corner is cut")
    verify(Qt.colorEqual(grabImage(panelHost).pixel(116, 36), backdrop), "and so is the bottom right")
    verify(!Qt.colorEqual(grabImage(panelHost).pixel(60, 20), backdrop), "the material fills the rest")
    var edge = grabImage(panelHost).pixel(0, 25)
    verify(edge.r > 0.8 && edge.g < 0.4, "the edge is the theme's border: " + edge)
    verify(Qt.colorEqual(grabImage(panelHost).pixel(40, 1), subject.panelTrimColor), "the trim runs along the top")
    // However small the frame, its cut, trim and glitch stay inside it.
    for (var size of [[48, 22], [12, 8], [90, 30], [140, 30]]) {
      narrowHost.width = size[0]
      narrowHost.height = size[1]
      wait(20)
      verify(narrowFrame.cut <= Math.min(size[0], size[1]) / 3, size + ": the cut fits")
      for (var index = 0; index < narrowFrame.children.length; index++) {
        var part = narrowFrame.children[index]
        if (!part.visible) continue
        verify(part.x >= 0 && part.y >= 0 && part.x + part.width <= size[0] && part.y + part.height <= size[1],
          size + ": part " + index + " at " + part.x + "," + part.y + " " + part.width + "x" + part.height + " stays inside")
      }
      var beside = grabImage(narrowBackdrop)
      verify(Qt.colorEqual(beside.pixel(size[0] + 4, 1), backdrop) && Qt.colorEqual(beside.pixel(199, 3), backdrop),
        size + ": nothing is drawn beside the frame")
    }
    narrowHost.width = 48
    narrowHost.height = 22
    // A tip keeps the plain material: no cut, and no frame loaded for one.
    var tip = tipSurface.createObject(tipHost)
    verify(!tip.cut, "a tooltip's surface is not cut")
    compare(tip.color, subject.panelColor)
    // Gone before the preset below brings back the grain film, which this
    // fixture does not generate.
    tip.destroy()
    wait(50)
    // A preset afterwards: rounded panels, the grain film, mixed case, and
    // no font loader left asking for an empty file.
    var preset = {id:"catppuccin-mocha",name:"Catppuccin Mocha",mode:"dark"}
    for (var key of keys) preset[key] = baseline[key].toString()
    subject.applyTheme(preset, slots)
    compare(subject.panelCut, 0)
    compare(subject.titleCapitalization, Font.MixedCase)
    compare(subject.fontFileRegular, "")
    verify(subject.grain.toString().endsWith("grain.png"), subject.grain)
    wait(100)
  }

  // The host's wallpaper mapping (SIL-236): the publication names the outputs
  // that draw another image, and every other output draws the theme's.
  function test_each_output_draws_the_image_its_mapping_gives_it() {
    var slots = contractSlots()
    verify(slots.some(function(slot) { return slot.name === "wallpaperOutputs" && slot.type === "outputPaths" }), "a suppliable slot")
    var theme = {id:"catppuccin-mocha",name:"Catppuccin Mocha",mode:"dark",wallpaper:"/night.png",
      wallpaperOutputs:{"DP-1":"/wide.png","HDMI-A-1":"/side.png"}}
    for (var key of keys) theme[key] = baseline[key].toString()
    subject.applyTheme(theme, slots)
    compare(subject.wallpaperFor("DP-1"), "/wide.png")
    compare(subject.wallpaperFor("HDMI-A-1"), "/side.png")
    compare(subject.wallpaperFor("eDP-1"), "/night.png", "an output the mapping does not name draws the theme's image")
    compare(subject.wallpaperFor(""), "/night.png")
    compare([wideImage, sideImage], ["/wide.png", "/side.png"], "each output's binding follows the published theme")
    subject.applyTheme(Object.assign({}, theme, {mode: "light", wallpaper: "/day.png", wallpaperOutputs: {"DP-1": "/wide.png"}}), slots)
    compare([wideImage, sideImage], ["/wide.png", "/day.png"], "and a mode switch")
    // A theme whose mapping names no output leaves none behind.
    subject.applyTheme(Object.assign({}, theme, {wallpaperOutputs: undefined, wallpaper: "/day.png"}), slots)
    compare(subject.wallpaperFor("DP-1"), "/day.png")
    compare(Object.keys(subject.wallpaperOutputs).length, 0)
    // Nor is a mapping that is not one of absolute paths applied.
    subject.applyTheme(Object.assign({}, theme, {wallpaperOutputs: {"DP-1": "wide.png"}}), slots)
    compare(subject.wallpaperFor("DP-1"), "/night.png")
    subject.applyTheme(Object.assign({}, theme, {wallpaperOutputs: ["/wide.png"]}), slots)
    compare(subject.wallpaperFor("0"), "/night.png")
    // Before any theme names an image, every output draws the configured one.
    subject.applyTheme(Object.assign({}, theme, {wallpaperOutputs: undefined, wallpaper: undefined}), slots)
    compare(subject.wallpaperFor("DP-1"), subject.configuredWallpaper)
  }
}
