import QtQuick
import QtTest
import "../projects/shared" as Shared

TestCase {
  id: testCase
  name: "PlainLabels"
  when: windowShown
  visible: true
  width: 640
  height: 200
  property var palette: ({
    alpha: function(value, opacity) { var color = Qt.darker(value, 1); return Qt.rgba(color.r, color.g, color.b, opacity) },
    controlHeight: 34, spaceLarge: 16, spaceSmall: 6, spaceMedium: 10,
    spaceTight: 2, cardPadding: 16, radius: 12, durationFast: 100,
    textLabel: 14, textBody: 14, textCaption: 12, textHero: 24,
    fontFamily: "sans-serif", weightMedium: Font.Medium,
    red: "#f38ba8", accent: "#cba6f7", text: "#cdd6f4",
    overlay: "#7f849c", subtext: "#a6adc8",
    cardColor: "#302e2e46", selectedColor: "#503e365f",
    dangerTint: "#30f38ba8", pressColor: "#60454b66",
    cardBorder: "#40454b66", hoverColor: "#20cdd6f4"
  })
  Shared.ActionButton { id: subject; theme: testCase.palette; x: 10; y: 10; width: 280; text: "Laptop · 2 files" }
  Shared.ActionButton { id: baseline; theme: testCase.palette; x: 330; y: 10; width: 280; text: subject.text }
  function init() {
    failOnWarning(/.?/)
    subject.selected = false; baseline.selected = false
    subject.danger = false; baseline.danger = false
    subject.enabled = true; baseline.enabled = true
    subject.text = "Laptop · 2 files"
    baseline.contentItem.textFormat = Text.AutoText
    mouseMove(testCase, 320, 180)
  }
  function test_regular_labels_keep_identical_pixels_data() {
    return [{tag:"normal"}, {tag:"selected", selected:true}, {tag:"danger", danger:true}, {tag:"disabled", disabled:true}]
  }
  function test_regular_labels_keep_identical_pixels(data) {
    subject.selected = baseline.selected = !!data.selected
    subject.danger = baseline.danger = !!data.danger
    subject.enabled = baseline.enabled = !data.disabled
    wait(150)
    compare(subject.implicitWidth, baseline.implicitWidth)
    compare(subject.implicitHeight, baseline.implicitHeight)
    verify(grabImage(subject).equals(grabImage(baseline)), "plain labels preserve pixels, alpha and geometry")
  }
  function test_markup_in_device_names_stays_literal() {
    subject.text = "<b>Laptop</b>"
    wait(150)
    compare(subject.contentItem.textFormat, Text.PlainText)
    verify(subject.contentItem.implicitWidth > baseline.contentItem.implicitWidth, "tags occupy literal label space")
    verify(!grabImage(subject).equals(grabImage(baseline)), "untrusted tags are displayed instead of interpreted")
  }
}
