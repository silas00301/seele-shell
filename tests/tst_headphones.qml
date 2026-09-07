import QtQuick
import QtTest

TestCase {
  name: "HeadphonesIcon"
  when: windowShown
  width: 64; height: 64
  visible: true
  HeadphonesIcon { id: icon; width: 16; height: 16; tint: "#ff0000" }

  function test_separateSilhouettes() {
    icon.kind = "headphones"
    waitForRendering(icon)
    var headphones = grabImage(icon)
    verify(headphones.red(8, 2) - headphones.green(8, 2) > 200, "Over-ear headband is visible")
    icon.kind = "airpods"
    waitForRendering(icon)
    var airpods = grabImage(icon)
    verify(airpods.red(8, 2) - airpods.green(8, 2) < 100, "AirPods have no over-ear headband")
    verify(airpods.red(3, 4) - airpods.green(3, 4) > 200, "AirPods earpiece is visible")
    icon.kind = "headphones"
    waitForRendering(icon)
    var restored = grabImage(icon)
    verify(restored.equals(headphones), "Switching back leaves no AirPods delegates behind")
  }
}
