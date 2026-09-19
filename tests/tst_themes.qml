import QtQuick
import QtTest
import "production" as Shell
import "production/shared" as Shared

// The production Themes panel over a fixture store. What a catalog is, how it
// is grouped and what a failure says are native policy covered elsewhere; this
// covers what a person does with the panel, and fails on any Qt warning.
TestCase {
  id: testCase
  name: "ThemesPanel"
  when: windowShown
  visible: true
  property string screenshotPath: ""
  width: 500
  height: 800
  Shared.Theme { id: theme }
  ListModel { id: rows }
  QtObject {
    id: fixture
    property var model: rows
    property string query: ""
    property string error: ""
    property string actionError: ""
    property string reloadPending: ""
    property string applying: ""
    property bool busy: false
    property string detail: "Catppuccin Mocha · 4 presets"
    property var applied: []
    property int refreshed: 0
    function refresh() { refreshed++ }
    function apply(id) { applied = applied.concat([id]); return true }
    function failure(code) { return "" }
  }
  Rectangle { anchors.fill: parent; color: theme.crust }
  Shell.ThemePanel {
    id: panel
    theme: theme
    store: fixture
    x: theme.panelMargin
    y: theme.panelMargin
    width: parent.width - theme.panelMargin * 2
  }

  function child(parentItem, name) {
    var item = findChild(parentItem, name)
    verify(item !== null, "Missing " + name)
    return item
  }
  // A row exactly as `themes.rows` shapes one, with a real palette.
  function entry(id, name, mode, section, first, current, palette) {
    return {
      id: id, name: name, mode: mode, modeLabel: mode === "light" ? "Light" : "Dark",
      section: section, first: first, current: current,
      base: palette[0], mantle: palette[1], crust: palette[0], surface: palette[2],
      overlay: palette[3], text: palette[4], subtext: palette[5], accent: palette[6],
      red: palette[7], green: palette[8], yellow: palette[9],
      swatches: [palette[6], palette[7], palette[8], palette[9]]
    }
  }
  readonly property var mocha: ["#1e1e2e", "#181825", "#313244", "#585b70", "#cdd6f4", "#a6adc8", "#89b4fa", "#f38ba8", "#a6e3a1", "#f9e2af"]
  readonly property var nord: ["#2e3440", "#3b4252", "#434c5e", "#4c566a", "#e5e9f0", "#d8dee9", "#81a1c1", "#bf616a", "#a3be8c", "#ebcb8b"]
  readonly property var dawn: ["#faf4ed", "#fffaf3", "#f2e9e1", "#9893a5", "#575279", "#797593", "#56949f", "#b4637a", "#286983", "#ea9d34"]
  function catalog() {
    rows.clear()
    rows.append({ entry: entry("catppuccin-mocha", "Catppuccin Mocha", "dark", "CURRENT", true, true, mocha) })
    rows.append({ entry: entry("nord", "Nord", "dark", "DARK", true, false, nord) })
    rows.append({ entry: entry("everforest", "Everforest", "dark", "DARK", false, false, nord) })
    rows.append({ entry: entry("rose-pine-dawn", "Rosé Pine Dawn", "light", "LIGHT", true, false, dawn) })
  }
  function init() {
    failOnWarning(/.?/)
    fixture.query = ""
    fixture.error = ""
    fixture.actionError = ""
    fixture.reloadPending = ""
    fixture.applying = ""
    fixture.busy = false
    fixture.applied = []
    fixture.refreshed = 0
    panel.maximumHeight = theme.themesMaximumHeight
    catalog()
    child(panel, "presets").currentIndex = 0
    wait(20)
  }

  function test_groups_are_headed_once_and_the_applied_theme_is_marked() {
    var list = child(panel, "presets")
    tryCompare(list, "count", 4)
    var headings = []
    for (var i = 0; i < list.count; i++) {
      var row = list.itemAtIndex(i)
      verify(row !== null, "row " + i + " is instantiated")
      var heading = child(row, "groupHeading")
      if (heading.visible) headings.push(heading.text)
      var chip = child(row, "stateChip")
      compare(chip.visible, i === 0, "only the applied theme carries a chip")
    }
    compare(headings, ["CURRENT", "DARK", "LIGHT"])
    compare(child(list.itemAtIndex(0), "stateChip").text, "Current")
  }

  function test_keyboard_moves_and_applies() {
    var list = child(panel, "presets")
    panel.handleKey({ key: Qt.Key_J, modifiers: Qt.NoModifier, accepted: false })
    compare(list.currentIndex, 1)
    panel.handleKey({ key: Qt.Key_Down, modifiers: Qt.NoModifier, accepted: false })
    compare(list.currentIndex, 2)
    panel.handleKey({ key: Qt.Key_K, modifiers: Qt.NoModifier, accepted: false })
    compare(list.currentIndex, 1)
    panel.handleKey({ key: Qt.Key_Return, modifiers: Qt.NoModifier, accepted: false })
    compare(fixture.applied, ["nord"])
    panel.handleKey({ key: Qt.Key_R, modifiers: Qt.NoModifier, accepted: false })
    compare(fixture.refreshed, 1)
    // A shortcut chord belongs to the compositor or the field, not the panel.
    panel.handleKey({ key: Qt.Key_J, modifiers: Qt.ControlModifier, accepted: false })
    compare(list.currentIndex, 1)
  }

  function test_the_search_field_filters_and_return_applies() {
    var search = child(panel, "search")
    search.forceActiveFocus()
    keyClick(Qt.Key_N)
    keyClick(Qt.Key_O)
    compare(fixture.query, "no", "typing reaches the store's query")
    child(panel, "presets").currentIndex = 1
    keyClick(Qt.Key_Return)
    compare(fixture.applied, ["nord"], "Return in the field applies the selected row")
    keyClick(Qt.Key_Down)
    compare(child(panel, "presets").currentIndex, 2, "arrows move the list from the field")
  }

  function test_a_click_applies_that_row() {
    var list = child(panel, "presets")
    var card = child(list.itemAtIndex(3), "card")
    mouseClick(card, card.width / 2, card.height / 2)
    compare(fixture.applied, ["rose-pine-dawn"])
    compare(list.currentIndex, 3, "the clicked row becomes the selection")
  }

  function test_selection_survives_a_narrowing_search() {
    var list = child(panel, "presets")
    var card = child(list.itemAtIndex(3), "card")
    mouseClick(card, card.width / 2, card.height / 2)
    compare(list.currentIndex, 3)
    // The search leaves one row, after a click has ended the declarative
    // binding: Enter must still apply a row that exists.
    rows.remove(1, 3)
    tryCompare(list, "currentIndex", 0)
    panel.handleKey({ key: Qt.Key_Return, modifiers: Qt.NoModifier, accepted: false })
    compare(fixture.applied, ["rose-pine-dawn", "catppuccin-mocha"])
    rows.clear()
    tryCompare(list, "currentIndex", -1)
    panel.handleKey({ key: Qt.Key_Return, modifiers: Qt.NoModifier, accepted: false })
    compare(fixture.applied.length, 2, "an empty list applies nothing")
    // The next query refills it, and Enter applies the first row again.
    rows.append({ entry: entry("nord", "Nord", "dark", "DARK", true, false, nord) })
    tryCompare(list, "currentIndex", 0)
    panel.handleKey({ key: Qt.Key_Return, modifiers: Qt.NoModifier, accepted: false })
    compare(fixture.applied, ["rose-pine-dawn", "catppuccin-mocha", "nord"])
  }

  function test_an_empty_list_offers_its_own_way_out() {
    var search = child(panel, "search")
    search.forceActiveFocus()
    keyClick(Qt.Key_Z)
    rows.clear()
    wait(20)
    var action = child(panel, "emptyAction")
    verify(action.visible)
    compare(action.text, "Clear search")
    mouseClick(action, action.width / 2, action.height / 2)
    compare(fixture.query, "")
    compare(search.text, "", "clearing the search clears the field the person typed into")
    compare(action.text, "Refresh")
    mouseClick(action, action.width / 2, action.height / 2)
    compare(fixture.refreshed, 1)
    fixture.busy = true
    verify(!action.enabled, "a refresh already running is not asked for again")
  }

  function test_applying_and_unreloaded_apps_are_stated_in_place() {
    fixture.applying = "nord"
    var list = child(panel, "presets")
    var chip = child(list.itemAtIndex(1), "stateChip")
    tryCompare(chip, "visible", true)
    compare(chip.text, "Applying")
    fixture.applying = ""
    fixture.reloadPending = "Ghostty and tmux still show the previous palette."
    wait(20)
    verify(panel.implicitHeight > 0)
    fixture.error = "The theme helper is unavailable. Nothing was changed."
    var retry = child(panel, "retry")
    // The banner is laid out on the next polish, and a click lands where the
    // button is drawn, so wait until it has a place before pressing it.
    tryVerify(function() { return retry.visible && retry.width > 0 && retry.mapToItem(panel, 0, 0).y > 0 })
    waitForRendering(panel)
    mouseClick(retry, retry.width / 2, retry.height / 2)
    compare(fixture.refreshed, 1)
  }

  function test_the_panel_stays_inside_the_room_it_is_given() {
    for (var i = 0; i < 16; i++)
      rows.append({ entry: entry("extra-" + i, "Extra " + i, "dark", "DARK", false, false, nord) })
    // A short output: the list gives way, the search and heading do not.
    panel.maximumHeight = 320
    wait(20)
    verify(panel.implicitHeight <= panel.maximumHeight,
      "panel " + panel.implicitHeight + " fits " + panel.maximumHeight)
    var list = child(panel, "presets")
    verify(list.height >= theme.detailRowHeight, "at least one row stays visible")
    verify(list.contentHeight > list.height, "the rest scrolls")
    // Banners take room from the list rather than pushing the panel past it.
    fixture.reloadPending = "Ghostty still shows the previous palette."
    wait(20)
    verify(panel.implicitHeight <= panel.maximumHeight)
  }

  function test_controls_fit_and_render() {
    var search = child(panel, "search")
    verify(search.width > 0 && search.mapToItem(panel, search.width, 0).x <= panel.width)
    var list = child(panel, "presets")
    var row = list.itemAtIndex(0)
    var chip = child(row, "stateChip")
    verify(chip.mapToItem(panel, chip.width, 0).x <= panel.width, "the chip stays inside the card")
    var capture = grabImage(panel)
    verify(capture.width > 0 && capture.height > 0)
    if (screenshotPath !== "") capture.save(screenshotPath)
  }
}
