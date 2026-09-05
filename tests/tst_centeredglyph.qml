import QtQuick
import QtTest

TestCase {
  name: "CenteredGlyph"
  when: windowShown

  CenteredGlyph {
    id: subject

    width: 26
    height: 26
    font.family: "Maple Mono NF CN"
    font.pixelSize: 15
  }

  FontMetrics {
    id: metrics
    font: subject.font
  }

  function test_ink_is_centered_data() {
    return [
      { tag: "calendar", glyph: "󰃭" },
      { tag: "clock", glyph: "󰥔" },
      { tag: "agents", glyph: "󱚣" },
      { tag: "control center", glyph: "󰘮" },
      { tag: "media", glyph: "󰎆" },
      { tag: "audio", glyph: "󰕾" },
      { tag: "network", glyph: "󰤨" },
      { tag: "vpn", glyph: "󰒃" },
      { tag: "bluetooth", glyph: "󰂯" },
      { tag: "battery", glyph: "󰁹" },
      { tag: "notifications", glyph: "󰂚" },
      { tag: "dnd", glyph: "󰂛" },
      { tag: "camera", glyph: "󰄀" },
      { tag: "power", glyph: "󰐥" }
    ]
  }

  function test_ink_is_centered(data) {
    subject.text = data.glyph

    const bounds = metrics.tightBoundingRect(data.glyph)
    const inkCenterX = subject.contentItem.x
      + (subject.contentItem.width - metrics.advanceWidth(data.glyph)) / 2
      + bounds.x + bounds.width / 2
    const inkCenterY = subject.contentItem.y
      + (subject.contentItem.height - metrics.height) / 2
      + metrics.ascent + bounds.y + bounds.height / 2

    verify(Math.abs(inkCenterX - subject.width / 2) <= 0.01,
      "horizontal ink offset is " + (inkCenterX - subject.width / 2))
    verify(Math.abs(inkCenterY - subject.height / 2) <= 0.01,
      "vertical ink offset is " + (inkCenterY - subject.height / 2))
  }
}
