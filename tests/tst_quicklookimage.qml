import QtQuick
import QtTest
import "production" as Production
import "production/shared" as Shared
TestCase {
  id: test
  name: "QuickLookImage"
  width: 600; height: 400; visible: true; when: windowShown
  Shared.Theme { id: theme }
  Production.QuickLookImage {
    id: image
    width: 300; height: 200; theme: theme
    focus: true
    Keys.onPressed: event => image.imageKey(event)
  }
  function test_pixels_pan_fit_and_source_reset() {
    image.sourceWidth = 2000; image.sourceHeight = 1000
    image.source = Qt.resolvedUrl("large.png")
    var decoded = findChild(image, "quickLookDecodedImage")
    tryCompare(decoded, "status", Image.Ready)
    compare(image.pixelMode, false)
    image.forceActiveFocus()
    keyClick(Qt.Key_Z)
    tryCompare(decoded, "implicitWidth", 2000)
    compare(image.contentWidth, 2000)
    compare(decoded.width, 2000)
    keyClick(Qt.Key_Right, Qt.AltModifier)
    compare(image.contentX, 64)
    image.pan(100000, 100000)
    compare(image.contentX, image.contentWidth-image.width)
    compare(image.contentY, image.contentHeight-image.height)
    image.pan(-100000, -100000)
    compare(image.contentX, 0)
    keyClick(Qt.Key_Z)
    compare(image.pixelMode, false)
    compare(image.contentWidth, image.width)
    keyClick(Qt.Key_Z)
    image.sourceWidth = 20; image.sourceHeight = 10
    image.source = Qt.resolvedUrl("small.png")
    tryCompare(decoded, "status", Image.Ready)
    compare(image.pixelMode, false)
    compare(image.contentX, 0)
    compare(image.contentY, 0)
  }
}
