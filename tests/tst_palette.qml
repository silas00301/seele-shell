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
    return slot.type === "weight" ? Font[value] : slot.type === "easing" ? Easing[value] : value
  }
  function test_a_published_theme_sets_every_slot_and_leaves_nothing_behind() {
    var slots = contractSlots()
    verify(slots.length > 300, "every suppliable slot")
    verify(slots.every(function(slot) { return slot.name !== "wallpaper" && slot.name !== "panelColor" && slot.name !== "iosRed" }),
      "no wallpaper, derived or fixed slot")
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
    compare(subject.wallpaper, "", "the wallpaper is left to its consumers")
    // A preset supplies its roles and typeface only: everything else returns
    // to the default, exactly as the contract and Theme.qml declare it.
    var preset = {id:"catppuccin-mocha",name:"Catppuccin Mocha",mode:"dark",fontFamily:"Maple Mono NF CN"}
    for (var key of keys) preset[key] = baseline[key].toString()
    subject.applyTheme(preset, slots)
    for (var slot of slots) {
      if (slot.type === "color") compare(subject[slot.name], Qt.color(preset[slot.name]), slot.name)
      else compare(subject[slot.name], qmlValue(slot, slot.value), slot.name)
    }
    compare(subject.radius, 8)
    compare(subject.weightStrong, Font.DemiBold)
    compare(subject.easingStandard, Easing.OutCubic)
    // A value of the wrong kind is never assigned; its default stands.
    subject.applyTheme(Object.assign({}, folder, {radius:"round",accent:"lavender",textBody:null,easingStandard:7}), slots)
    compare(subject.radius, 8)
    compare(subject.accent, Qt.color(Palette.fallback.accent))
    compare(subject.textBody, 11)
    compare(subject.easingStandard, Easing.OutCubic)
    compare(subject.weightStrong, Font.Bold)
    subject.applyTheme(preset, slots)
    compare(subject.grainOpacity, 0.07)
  }
}
