import QtQuick
import QtTest
import "../projects/shared" as Shared
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
}
