import QtQuick
import QtTest
import "../projects/shared" as Shared

// The desktop background through a theme switch (SIL-235). A theme with its
// own image replaces the configured wallpaper, a theme without one brings the
// configured wallpaper back, and neither switch ever shows the bare base colour
// between the two pictures: the image on screen stays until the next one has
// decoded. A file that cannot be read leaves the base colour, as the contract's
// fallback says.
TestCase {
  id: testCase
  name: "Wallpaper"
  when: windowShown
  visible: true
  width: 160
  height: 90

  readonly property string here: decodeURIComponent(Qt.resolvedUrl(".").toString().replace(/^file:\/\//, ""))
  readonly property string configured: here + "wallpaper/configured.png"
  readonly property string pictured: here + "theme-folders/pictured/wallpaper.png"
  readonly property color base: "#101820"
  // Every status the image on screen passed through, and what was on screen
  // whenever that changed.
  property var statuses: []
  property var swaps: []
  // A copy the pixmap cache has not seen, so each load is asynchronous the way
  // the first one of a theme's image is; a cached image is ready at once and
  // takes over at once. tests/wallpaper.sh stages the copies.
  property int loads: 0
  function fresh(path) {
    return here + "wallpaper/fresh/" + (++loads) + "/" + path.slice(path.lastIndexOf("/") + 1)
  }

  Rectangle {
    id: ground
    width: 160
    height: 90
    color: testCase.base

    Shared.Wallpaper {
      id: subject
      anchors.fill: parent
      onStatusChanged: testCase.statuses.push(status)
      onShownPathChanged: testCase.swaps.push({path: shownPath, status: status})
    }
  }
  // The two fixture images, decoded on their own, to compare frames with.
  Image { id: configuredImage; width: 160; height: 90; x: 200; source: "file://" + testCase.configured }
  Image { id: picturedImage; width: 160; height: 90; x: 400; source: "file://" + testCase.pictured }

  function init() {
    failOnWarning(/.?/)
  }
  function showing(image) {
    var frame = grabImage(ground), expected = grabImage(image)
    for (var y = 5; y < 90; y += 20)
      for (var x = 5; x < 160; x += 30)
        if (!Qt.colorEqual(frame.pixel(x, y), expected.pixel(x, y))) return false
    return true
  }
  function settled(path) {
    tryCompare(subject, "shownPath", path, 5000)
    tryVerify(function() { return subject.status !== Image.Loading }, 5000)
    wait(50)
  }
  function begin(path) {
    tryCompare(configuredImage, "status", Image.Ready)
    tryCompare(picturedImage, "status", Image.Ready)
    subject.path = path
    settled(path)
    statuses = []
    swaps = []
  }

  function test_a_theme_image_replaces_the_configured_one_without_a_gap() {
    var start = fresh(configured), own = fresh(pictured), back = fresh(configured)
    begin(start)
    verify(showing(configuredImage), "the configured wallpaper is drawn")
    subject.path = own
    compare(subject.shownPath, start, "the old picture stays while the new one loads")
    verify(showing(configuredImage), "and is still drawn")
    settled(own)
    verify(showing(picturedImage), "the theme's own image is drawn")
    subject.path = back
    compare(subject.shownPath, own)
    settled(back)
    verify(showing(configuredImage), "a theme without an image brings the configured wallpaper back")
    compare(swaps.map(function(swap) { return swap.path }), [own, back])
    verify(swaps.every(function(swap) { return swap.status === Image.Ready }), "each picture is complete when it takes over")
    verify(statuses.every(function(status) { return status === Image.Ready }), "the picture on screen is never loading or empty: " + statuses)
  }

  function test_changing_back_before_the_new_image_loads_keeps_the_picture() {
    var start = fresh(configured)
    begin(start)
    subject.path = fresh(pictured)
    subject.path = start
    wait(200)
    compare(subject.shownPath, start)
    compare(swaps, [])
    verify(showing(configuredImage))
  }

  function test_the_latest_choice_wins_a_quick_succession() {
    begin(fresh(configured))
    subject.path = fresh(pictured)
    subject.path = fresh(configured)
    var last = fresh(pictured)
    subject.path = last
    settled(last)
    compare(swaps.map(function(swap) { return swap.path }), [last])
    verify(showing(picturedImage))
  }

  function test_a_name_with_url_characters_draws() {
    // A registered name may hold anything a file name can, `#` and `?` too.
    var named = here + "wallpaper/dusk #1 ?%é.png"
    begin(fresh(configured))
    subject.path = named
    settled(named)
    compare(subject.status, Image.Ready)
    verify(showing(picturedImage))
  }

  function test_a_file_that_cannot_be_read_leaves_the_base_colour() {
    begin(pictured)
    ignoreWarning(/Cannot open/)
    subject.path = "/nonexistent/wallpaper.png"
    settled("/nonexistent/wallpaper.png")
    compare(subject.status, Image.Error)
    verify(Qt.colorEqual(grabImage(ground).pixel(80, 45), base), "the base colour shows")
    subject.path = ""
    settled("")
    compare(subject.status, Image.Null)
    verify(Qt.colorEqual(grabImage(ground).pixel(80, 45), base))
    subject.path = configured
    settled(configured)
    verify(showing(configuredImage))
  }
}
