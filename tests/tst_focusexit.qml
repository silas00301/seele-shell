import QtQuick
import QtTest

TestCase {
  id: testCase
  name: "FocusExitCue"
  when: windowShown
  visible: true
  width: 1280
  height: 720

  QtObject {
    id: theme
    property color green: "#a6e3a1"
    property int durationFast: 40
    property int durationGlance: 80
    property int durationSettle: 50
    property real edgeCueReach: 0.1
    property real edgeCueAlpha: 0.82
    function alpha(color, opacity) { return Qt.rgba(color.r, color.g, color.b, opacity) }
  }

  FocusExitCue {
    id: cue
    theme: theme
  }

  Rectangle {
    id: desk
    anchors.fill: parent
    color: "#1e1e2e"
    FocusExitRim {
      id: rim
      anchors.fill: parent
      theme: theme
      strength: cue.strength
    }
  }

  function test_peak_is_a_green_rim_and_the_center_stays_clear() {
    compare(cue.playing, false)
    cue.strength = 1
    waitForRendering(desk)
    var image = grabImage(desk)
    image.save("__SHOT_DIR__/focus-exit-peak.png")
    var edgeG = image.green(2, 360)
    var edgeR = image.red(2, 360)
    var edgeB = image.blue(2, 360)
    var centerR = image.red(640, 360)
    var centerG = image.green(640, 360)
    var centerB = image.blue(640, 360)
    verify(edgeG > edgeR + 15, "bezel green " + edgeG + " should lead red " + edgeR)
    verify(edgeG > edgeB + 15, "bezel green " + edgeG + " should lead blue " + edgeB)
    verify(edgeG > 140, "bezel should read from across the desk")
    var bodyG = image.green(24, 360)
    verify(bodyG > 70, "the wash should carry in from the bezel, green " + bodyG)
    verify(Math.abs(centerR - 0x1e) < 8, "center red " + centerR)
    verify(Math.abs(centerG - 0x1e) < 8, "center green " + centerG)
    verify(Math.abs(centerB - 0x2e) < 8, "center blue " + centerB)
    compare(rim.activeFocus, false)
    compare(desk.activeFocus, false)
    cue.strength = 0
  }

  function test_play_rises_and_dismisses() {
    cue.play()
    tryVerify(function() { return cue.playing && cue.strength > 0.85 }, 1000, "cue should reach full strength")
    tryVerify(function() { return !cue.playing && cue.strength === 0 }, 2000, "cue should dismiss")
    waitForRendering(desk)
    var image = grabImage(desk)
    image.save("__SHOT_DIR__/focus-exit-dismissed.png")
    var edgeG = image.green(2, 360)
    var centerG = image.green(640, 360)
    verify(Math.abs(edgeG - centerG) < 8, "dismissed bezel " + edgeG + " should match the desk " + centerG)
    verify(Math.abs(centerG - 0x1e) < 8, "dismissed center " + centerG + " should still be the desk")
    compare(rim.activeFocus, false)
  }
}
