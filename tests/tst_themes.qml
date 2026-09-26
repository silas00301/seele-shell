import QtQuick
import QtTest
import "production" as Shell
import "production/shared" as Shared

// The production Themes carousel over a fixture store. What the carousel holds,
// where the keys lead and what the schedule says are native policy covered by
// qml-core's tests and themes.js; this covers what a person does with the
// picker, and fails on any Qt warning.
TestCase {
  id: testCase
  name: "ThemesPicker"
  when: windowShown
  visible: true
  property string screenshotPath: ""
  width: 1100
  height: 900
  Shared.Theme { id: theme }

  // An entry exactly as `themes.carousel` shapes one, with a real palette.
  function preset(id, name, mode, colors, current) {
    return {
      id: id, name: name, mode: mode, modeLabel: mode === "light" ? "Light" : "Dark",
      detail: name.indexOf(mode === "light" ? "Light" : "Dark") >= 0 ? "" : (mode === "light" ? "Light" : "Dark"),
      current: current, base: colors[0], mantle: colors[1], crust: colors[0], surface: colors[2],
      overlay: colors[3], text: colors[4], subtext: colors[5], accent: colors[6],
      red: colors[7], green: colors[8], yellow: colors[9]
    }
  }
  readonly property var mocha: ["#1e1e2e", "#181825", "#313244", "#6c7086", "#cdd6f4", "#a6adc8", "#b4befe", "#f38ba8", "#a6e3a1", "#f9e2af"]
  readonly property var nord: ["#2e3440", "#3b4252", "#434c5e", "#7f848e", "#e5e9f0", "#d8dee9", "#81a1c1", "#bf616a", "#a3be8c", "#ebcb8b"]
  readonly property var flexoki: ["#100f0f", "#1c1b1a", "#282726", "#878580", "#cecdc3", "#878580", "#4385be", "#d14d41", "#879a39", "#d0a215"]
  function strip(count) {
    var items = [
      preset("catppuccin-mocha", "Catppuccin Mocha", "dark", mocha, true),
      preset("nord", "Nord", "dark", nord, false),
      preset("flexoki-dark", "Flexoki Dark", "dark", flexoki, false)
    ]
    for (var i = 0; i < (count || 0); i++) items.push(preset("extra-" + i, "Extra " + i, "dark", nord, false))
    return { items: items, order: items.map(function (item) { return item.id }), count: items.length, scoped: items.length, total: 13 }
  }
  function fill(carousel) {
    fixture.carousel = carousel
    rows.clear()
    for (var i = 0; i < carousel.items.length; i++) rows.append({ entry: carousel.items[i] })
  }
  ListModel { id: rows }
  QtObject {
    id: fixture
    readonly property var model: rows
    property var carousel: ({ items: [], order: [], count: 0, scoped: 0, total: 0 })
    property string focusedId: "catppuccin-mocha"
    property string mode: "dark"
    property var appearance: ({ mode: "dark", place: "Berlin", auto: { source: "off", lightAt: "07:00", darkAt: "19:00" } })
    readonly property string autoSource: appearance.auto.source
    property string scheduleText: ""
    property int total: 13
    property string query: ""
    property bool showAll: false
    property bool busy: false
    property string error: ""
    property string actionError: ""
    property string reloadPending: ""
    property var calls: []
    function record(call) { calls = calls.concat([call]) }
    function step(direction) { record("step " + direction) }
    function choose(id, now) { record("choose " + id + (now ? " now" : "")) }
    function setMode(value) { record("mode " + value) }
    function setAuto(source, lightAt, darkAt) { record(["auto", source].concat(lightAt ? [lightAt, darkAt] : []).join(" ")) }
    function toggleAll() { record("all") }
    function cancel() { record("cancel") }
    function resetFilters() { record("reset"); query = ""; showAll = false }
    function refresh() { record("refresh") }
    function failure(code) { return "" }
  }
  Rectangle { anchors.fill: parent; color: theme.crust }
  Shell.ThemePanel {
    id: panel
    theme: theme
    store: fixture
    x: theme.panelMargin
    y: theme.panelMargin
    width: 1008
  }
  SignalSpy { id: closes; target: panel; signalName: "closeRequested" }

  function child(parentItem, name) {
    var item = findChild(parentItem, name)
    verify(item !== null, "Missing " + name)
    return item
  }
  function press(key, modifiers, text) {
    panel.handleKey({ key: key, modifiers: modifiers || Qt.NoModifier, text: text || "", accepted: false })
  }
  function init() {
    failOnWarning(/.?/)
    fixture.focusedId = "catppuccin-mocha"
    fixture.mode = "dark"
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "off", lightAt: "07:00", darkAt: "19:00" } })
    fixture.scheduleText = ""
    fixture.total = 13
    fixture.query = ""
    fixture.showAll = false
    fixture.busy = false
    fixture.error = ""
    fixture.actionError = ""
    fixture.reloadPending = ""
    fixture.calls = []
    fill(strip(0))
    panel.width = 1008
    panel.maximumHeight = theme.themesMaximumHeight
    closes.clear()
    wait(20)
  }

  function test_the_centre_is_a_live_scene_and_its_neighbours_are_slices() {
    var centre = child(panel, "card-catppuccin-mocha")
    tryCompare(centre, "width", panel.cardWidth)
    verify(child(centre, "scene"), "the centre draws its preset")
    compare(child(centre, "scene").preset.id, "catppuccin-mocha")
    var neighbour = child(panel, "card-nord")
    compare(neighbour.width, panel.sliceWidth)
    verify(findChild(neighbour, "scene") === null, "a neighbour is a slice, not a second scene")
    verify(neighbour.x > centre.x + centre.width, "later entries sit to the right")
    compare(child(panel, "name").text, "Catppuccin Mocha")
    compare(child(panel, "scope").text, "Dark theme · 1 of 3")
    fixture.focusedId = "nord"
    tryCompare(child(panel, "card-nord"), "width", panel.cardWidth)
    verify(child(panel, "card-catppuccin-mocha").x < child(panel, "card-nord").x, "the previous entry slid to the left")
    compare(child(panel, "scope").text, "Dark theme · 2 of 3")
  }

  function test_keys_choose_switch_modes_filter_keep_and_put_back() {
    press(Qt.Key_Right); press(Qt.Key_Tab); press(Qt.Key_Left); press(Qt.Key_Backtab)
    press(Qt.Key_Up); press(Qt.Key_Down)
    compare(fixture.calls, ["step right", "step right", "step left", "step left", "mode light", "mode dark"])
    fixture.calls = []
    press(Qt.Key_N, Qt.NoModifier, "n"); press(Qt.Key_O, Qt.NoModifier, "o")
    compare(fixture.query, "no", "typing filters")
    press(Qt.Key_Space, Qt.NoModifier, " ")
    compare(fixture.query, "no ", "a space joins words once there is a filter")
    press(Qt.Key_Backspace)
    compare(fixture.query, "no")
    press(Qt.Key_Escape)
    compare(fixture.query, "", "Escape first takes back the filter")
    compare(closes.count, 0)
    press(Qt.Key_Space, Qt.NoModifier, " ")
    compare(fixture.query, "", "a leading space is not a filter")
    press(Qt.Key_A, Qt.ControlModifier, "\u0001")
    compare(fixture.calls, ["all"], "Ctrl + A opens the carousel to every preset")
    press(Qt.Key_J, Qt.ControlModifier)
    compare(fixture.calls, ["all"], "other chords belong to the compositor")
    press(Qt.Key_Escape)
    compare(fixture.calls, ["all", "cancel"], "then Escape puts everything back")
    compare(closes.count, 1)
    fixture.focusedId = "nord"
    press(Qt.Key_Return)
    compare(fixture.calls.slice(-1), ["choose nord now"], "Enter keeps the centre, even one a filter moved")
    compare(closes.count, 2)
  }

  function test_clicking_a_neighbour_chooses_it_and_clicking_the_centre_keeps_it() {
    var neighbour = child(panel, "card-flexoki-dark")
    mouseClick(neighbour, neighbour.width / 2, neighbour.height / 2)
    compare(fixture.calls, ["choose flexoki-dark now"])
    compare(closes.count, 0, "and leaves the picker open for the next one")
    var centre = child(panel, "card-catppuccin-mocha")
    mouseClick(centre, centre.width / 2, centre.height / 2)
    compare(fixture.calls.slice(-1), ["choose catppuccin-mocha now"])
    compare(closes.count, 1)
  }

  function test_the_mode_and_the_schedule_are_one_click_each() {
    var light = child(panel, "modeLight")
    mouseClick(light, light.width / 2, light.height / 2)
    for (var name of ["autoOff", "autoSun", "autoSchedule"]) {
      var choice = child(panel, name)
      mouseClick(choice, choice.width / 2, choice.height / 2)
    }
    compare(fixture.calls, ["mode light", "auto off", "auto sun", "auto schedule"])
    fixture.appearance = ({ mode: "dark", place: null, auto: { source: "off", lightAt: "07:00", darkAt: "19:00" } })
    wait(20)
    verify(!child(panel, "autoSun").enabled, "a timezone without a city cannot follow the sun")
  }

  function test_the_schedule_line_and_its_times() {
    verify(!child(panel, "lightAt").visible, "times appear only for a fixed schedule")
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "schedule", lightAt: "07:00", darkAt: "19:00" } })
    fixture.scheduleText = "Light at 07:00 · on a schedule"
    wait(20)
    compare(child(panel, "schedule").text, "Light at 07:00 · on a schedule")
    var lightAt = child(panel, "lightAt")
    verify(lightAt.visible)
    compare(lightAt.text, "07:00")
    lightAt.forceActiveFocus()
    lightAt.selectAll()
    keyClick(Qt.Key_0); keyClick(Qt.Key_6); keyClick(Qt.Key_Colon); keyClick(Qt.Key_3); keyClick(Qt.Key_0)
    keyClick(Qt.Key_Return)
    compare(fixture.calls, ["auto schedule 06:30 19:00"], "Enter sends the time and nothing else")
    compare(closes.count, 0, "Enter in a time field does not close the picker")
    compare(fixture.query, "", "typing a time is not a filter")
    verify(panel.activeFocus, "the keyboard is handed back to the carousel")
    // The field loses focus again as the row changes: an edit already sent
    // is not sent twice.
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "off", lightAt: "07:00", darkAt: "19:00" } })
    wait(20)
    compare(fixture.calls.length, 1)
    // The helper's word differs from the edit (another picker, a refusal):
    // the field shows what is saved, not what was typed.
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "schedule", lightAt: "05:45", darkAt: "19:00" } })
    wait(20)
    compare(lightAt.text, "05:45", "the field follows the saved time")
    var darkAt = child(panel, "darkAt")
    darkAt.forceActiveFocus()
    darkAt.selectAll()
    keyClick(Qt.Key_2); keyClick(Qt.Key_5)
    keyClick(Qt.Key_Return)
    compare(fixture.calls.length, 1, "an unfinished time is not sent")
    darkAt.forceActiveFocus()
    keyClick(Qt.Key_Escape)
    compare(darkAt.text, "19:00", "Escape in a field puts its saved time back")
    compare(closes.count, 0, "and does not close the picker")
    compare(fixture.calls.slice(-1)[0].indexOf("cancel"), -1, "nor put the theme back")
  }

  function test_the_scope_toggle_says_what_it_adds() {
    var toggle = child(panel, "scopeToggle")
    compare(toggle.text, "Show all 13")
    mouseClick(toggle, toggle.width / 2, toggle.height / 2)
    compare(fixture.calls, ["all"])
    fixture.showAll = true
    wait(20)
    compare(toggle.text, "Only dark presets")
  }

  function test_an_empty_carousel_offers_its_own_way_out() {
    fixture.query = "zzz"
    fill({ items: [], order: [], count: 0, scoped: 3, total: 13 })
    fixture.focusedId = ""
    wait(20)
    verify(!child(panel, "carousel").visible)
    verify(!child(panel, "name").visible)
    var action = child(panel, "emptyAction")
    compare(action.text, "Show all presets")
    mouseClick(action, action.width / 2, action.height / 2)
    compare(fixture.calls, ["reset"])
    verify(fixture.showAll, "and every preset is offered")
    fixture.total = 0
    wait(20)
    compare(action.text, "Refresh")
    mouseClick(action, action.width / 2, action.height / 2)
    compare(fixture.calls.slice(-1), ["refresh"])
  }

  function test_problems_are_stated_in_place() {
    fixture.reloadPending = "Ghostty still shows the previous palette."
    fixture.error = "The theme helper is unavailable. Nothing was changed."
    var retry = child(panel, "retry")
    tryVerify(function() { return retry.visible && retry.width > 0 && retry.mapToItem(panel, 0, 0).y >= 0 })
    waitForRendering(panel)
    mouseClick(retry, retry.width / 2, retry.height / 2)
    compare(fixture.calls, ["refresh"])
  }

  function test_the_carousel_fits_the_room_it_is_given() {
    fill(strip(8))
    // A short output: the card shrinks, keeping its shape, before anything
    // is cut off.
    panel.maximumHeight = 420
    wait(20)
    verify(panel.implicitHeight <= panel.maximumHeight + 0.5, "picker " + panel.implicitHeight + " fits " + panel.maximumHeight)
    compare(Math.round(panel.cardWidth / panel.cardHeight * 10), 16, "the card keeps 16:10")
    // A narrow output shows fewer slices rather than squeezing them.
    panel.width = 640
    wait(20)
    var shown = 0
    for (var i = 0; i < rows.count; i++) if (child(panel, "card-" + rows.get(i).entry.id).visible) shown++
    compare(shown, 1 + panel.sideCount, "the centre and the slices the width holds")
    // Cards slide to their places; once they have, each is inside the strip.
    for (var j = 0; j < rows.count; j++) {
      var card = child(panel, "card-" + rows.get(j).entry.id)
      if (!card.visible) continue
      tryVerify(function() { return card.x >= -0.5 && card.x + card.width <= child(panel, "carousel").width + 0.5 },
        1000, card.objectName + " stays inside")
    }
  }

  function test_controls_fit_and_render() {
    var auto = child(panel, "autoSchedule")
    verify(auto.mapToItem(panel, auto.width, 0).x <= panel.width + 0.5, "the schedule control stays inside the picker")
    var centre = child(panel, "card-catppuccin-mocha")
    verify(Math.abs(centre.x + centre.width / 2 - child(panel, "carousel").width / 2) < 1, "the centre is centred")
    var capture = grabImage(panel)
    verify(capture.width > 0 && capture.height > 0)
    if (screenshotPath !== "") capture.save(screenshotPath)
  }
}
