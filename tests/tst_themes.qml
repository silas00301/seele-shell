import QtQuick
import QtTest
import "production" as Shell
import "production/shared" as Shared

// The production Themes panel over a fixture store. Grouping, movement and the
// switching itself are covered by qml-core's tests and themes.js; this covers
// what a person does with the panel, and fails on any Qt warning.
TestCase {
  id: testCase
  name: "ThemesPanel"
  when: windowShown
  visible: true
  property string screenshotPath: ""
  width: 640
  height: 900
  Shared.Theme { id: theme }

  // A preset exactly as the native layout carries one, with a real palette.
  function preset(id, name, variant, detail, mode, colors, current) {
    return {
      id: id, name: name, variant: variant, detail: detail, mode: mode,
      modeLabel: mode === "light" ? "Light" : "Dark", current: current,
      base: colors[0], mantle: colors[1], crust: colors[0], surface: colors[2],
      overlay: colors[3], text: colors[4], subtext: colors[5], accent: colors[6],
      red: colors[7], green: colors[8], yellow: colors[9]
    }
  }
  readonly property var mocha: ["#1e1e2e", "#181825", "#313244", "#6c7086", "#cdd6f4", "#a6adc8", "#b4befe", "#f38ba8", "#a6e3a1", "#f9e2af"]
  readonly property var latte: ["#eff1f5", "#e6e9ef", "#ccd0da", "#8c8fa1", "#4c4f69", "#6c6f85", "#7287fd", "#d20f39", "#40a02b", "#df8e1d"]
  readonly property var flexoki: ["#100f0f", "#1c1b1a", "#282726", "#878580", "#cecdc3", "#878580", "#4385be", "#d14d41", "#879a39", "#d0a215"]
  function catalog(current) {
    return {
      rows: [
        { family: "Catppuccin", first: true, members: [
          preset("catppuccin-mocha", "Catppuccin Mocha", "Mocha", "Dark", "dark", mocha, current === "catppuccin-mocha"),
          preset("catppuccin-latte", "Catppuccin Latte", "Latte", "Light", "light", latte, current === "catppuccin-latte")] },
        { family: "Flexoki", first: true, members: [
          preset("flexoki-dark", "Flexoki Dark", "Dark", "", "dark", flexoki, current === "flexoki-dark")] }
      ],
      order: ["catppuccin-mocha", "catppuccin-latte", "flexoki-dark"], count: 3, total: 3
    }
  }
  readonly property var names: ({ "catppuccin-mocha": "Catppuccin Mocha", "catppuccin-latte": "Catppuccin Latte", "flexoki-dark": "Flexoki Dark" })
  QtObject {
    id: fixture
    property int columns: 0
    property string query: ""
    property string mode: "all"
    property string error: ""
    property string actionError: ""
    property string reloadPending: ""
    property string applying: ""
    property bool busy: false
    property bool switching: false
    property string current: "catppuccin-mocha"
    property string opened: "catppuccin-mocha"
    property string desired: "catppuccin-mocha"
    readonly property string currentName: testCase.names[current] || ""
    readonly property string openedName: testCase.names[opened] || ""
    readonly property bool filtered: query !== "" || mode !== "all"
    property var layout: testCase.catalog("catppuccin-mocha")
    property string focusedId: "catppuccin-mocha"
    property var steps: []
    property var chosen: []
    property int refreshed: 0
    property int resets: 0
    property int reverted: 0
    function nameOf(id) { return testCase.names[id] || "" }
    function refresh() { refreshed++ }
    function step(direction) { steps = steps.concat([direction]) }
    function choose(id, now) { chosen = chosen.concat([id + (now ? " now" : " settled")]) }
    function revert() { reverted++ }
    function resetFilters() { resets++; query = ""; mode = "all" }
    function failure(code) { return "" }
  }
  Rectangle { anchors.fill: parent; color: theme.crust }
  Shell.ThemePanel {
    id: panel
    theme: theme
    store: fixture
    x: theme.panelMargin
    y: theme.panelMargin
    width: 568
  }
  SignalSpy { id: closes; target: panel; signalName: "closeRequested" }

  function child(parentItem, name) {
    var item = findChild(parentItem, name)
    verify(item !== null, "Missing " + name)
    return item
  }
  function press(key, modifiers) {
    panel.handleKey({ key: key, modifiers: modifiers || Qt.NoModifier, accepted: false })
  }
  function init() {
    failOnWarning(/.?/)
    fixture.query = ""
    fixture.mode = "all"
    fixture.error = ""
    fixture.actionError = ""
    fixture.reloadPending = ""
    fixture.applying = ""
    fixture.busy = false
    fixture.switching = false
    fixture.current = "catppuccin-mocha"
    fixture.opened = "catppuccin-mocha"
    fixture.desired = "catppuccin-mocha"
    fixture.layout = catalog("catppuccin-mocha")
    fixture.focusedId = "catppuccin-mocha"
    fixture.steps = []
    fixture.chosen = []
    fixture.refreshed = 0
    fixture.resets = 0
    fixture.reverted = 0
    panel.maximumHeight = theme.themesMaximumHeight
    closes.clear()
    wait(20)
  }

  function test_the_panel_tells_the_layout_how_wide_a_row_is() {
    compare(fixture.columns, panel.columns)
  }

  function test_tiles_sample_themselves_and_mark_the_applied_theme() {
    var grid = child(panel, "grid")
    verify(child(child(grid, "tile-catppuccin-mocha"), "appliedMark").visible)
    verify(!child(child(grid, "tile-catppuccin-latte"), "appliedMark").visible)
    verify(!child(child(grid, "tile-flexoki-dark"), "appliedMark").visible)
    compare(child(child(grid, "tile-catppuccin-mocha"), "ring").border.width, 2, "the keyboard's tile wears the ring")
    compare(child(child(grid, "tile-catppuccin-latte"), "ring").border.width, 1)
  }

  function test_arrow_keys_switch_as_they_move() {
    press(Qt.Key_Right); press(Qt.Key_L); press(Qt.Key_Down); press(Qt.Key_J)
    press(Qt.Key_Left); press(Qt.Key_H); press(Qt.Key_Up); press(Qt.Key_K)
    compare(fixture.steps, ["right", "right", "down", "down", "left", "left", "up", "up"])
    press(Qt.Key_J, Qt.ControlModifier)
    compare(fixture.steps.length, 8, "a chord belongs to the compositor, not the panel")
    press(Qt.Key_R)
    compare(fixture.refreshed, 1)
    compare(closes.count, 0)
  }

  function test_enter_keeps_the_ringed_tile_and_escape_closes() {
    fixture.focusedId = "flexoki-dark"
    press(Qt.Key_Return)
    compare(fixture.chosen, ["flexoki-dark now"], "a ring moved by a search is switched to at once")
    compare(closes.count, 1)
    press(Qt.Key_Escape)
    compare(closes.count, 2)
    compare(fixture.chosen.length, 1, "Escape keeps what is applied and switches nothing")
  }

  function test_the_search_field_steps_until_there_is_text_to_edit() {
    var search = child(panel, "search")
    search.forceActiveFocus()
    keyClick(Qt.Key_Down)
    keyClick(Qt.Key_Right)
    compare(fixture.steps, ["down", "right"], "with nothing typed, arrows switch")
    keyClick(Qt.Key_N)
    keyClick(Qt.Key_O)
    compare(fixture.query, "no", "typing reaches the store's query")
    keyClick(Qt.Key_Left)
    compare(fixture.steps, ["down", "right"], "with text typed, left and right move the caret")
    compare(search.cursorPosition, 1)
    keyClick(Qt.Key_Up)
    compare(fixture.steps, ["down", "right", "up"], "up and down still switch")
    keyClick(Qt.Key_Return)
    compare(fixture.chosen, ["catppuccin-mocha now"])
    compare(closes.count, 1, "Return in the field picks and closes")
    search.forceActiveFocus()
    keyClick(Qt.Key_Escape)
    compare(closes.count, 2, "Escape in the field closes")
  }

  function test_clicking_switches_and_hovering_only_lights() {
    var tile = child(child(panel, "grid"), "tile-flexoki-dark")
    mouseMove(tile, tile.width / 2 - 4, tile.height / 2)
    mouseMove(tile, tile.width / 2, tile.height / 2)
    compare(fixture.chosen, [], "pointing at a tile switches nothing")
    tryVerify(function() { return child(tile, "ring").border.color.a > 0 }, 500, "but lights it")
    mouseClick(tile, tile.width / 2, tile.height / 2)
    compare(fixture.chosen, ["flexoki-dark now"], "a click switches at once")
    compare(closes.count, 0, "and leaves the picker open for the next one")
  }

  function test_the_close_button_closes() {
    var close = child(panel, "close")
    mouseClick(close, close.width / 2, close.height / 2)
    compare(closes.count, 1)
  }

  function test_the_footer_says_what_is_applied_and_offers_the_way_back() {
    var status = child(panel, "status")
    var back = child(panel, "back")
    compare(status.text, "Catppuccin Mocha is applied")
    verify(!back.visible, "nothing to go back to yet")
    fixture.current = "flexoki-dark"
    fixture.desired = "flexoki-dark"
    wait(20)
    compare(status.text, "Flexoki Dark is applied")
    verify(back.visible)
    compare(back.text, "Back to Catppuccin Mocha")
    mouseClick(back, back.width / 2, back.height / 2)
    compare(fixture.reverted, 1)
    fixture.desired = "catppuccin-mocha"
    fixture.switching = true
    wait(20)
    compare(status.text, "Switching to Catppuccin Mocha…")
    verify(!back.visible, "going back is already under way")
  }

  function test_a_variant_that_names_its_mode_is_not_told_it_again() {
    var grid = child(panel, "grid")
    var texts = function(tile) {
      var out = []
      for (var i = 0; i < tile.children.length; i++) collect(tile.children[i], out)
      return out
    }
    var collect = function(item, out) {
      if (item.text !== undefined && item.visible && item.text !== "" && item.text !== "\u{f012c}") out.push(item.text)
      for (var i = 0; i < item.children.length; i++) collect(item.children[i], out)
    }
    compare(texts(child(grid, "tile-catppuccin-latte")), ["Latte", "Light"])
    compare(texts(child(grid, "tile-flexoki-dark")), ["Dark"], "Flexoki's Dark is not followed by 'Dark'")
  }

  function test_the_mode_filter_is_one_click() {
    var dark = child(panel, "modeDark")
    mouseClick(dark, dark.width / 2, dark.height / 2)
    compare(fixture.mode, "dark")
    verify(dark.selected)
    var light = child(panel, "modeLight")
    light.forceActiveFocus()
    keyClick(Qt.Key_Return)
    compare(fixture.mode, "light")
    compare(fixture.chosen, [], "filtering switches nothing")
  }

  function test_an_empty_grid_offers_its_own_way_out() {
    fixture.query = "zzz"
    fixture.layout = { rows: [], order: [], count: 0, total: 3 }
    fixture.focusedId = ""
    wait(20)
    verify(!child(panel, "grid").visible)
    var action = child(panel, "emptyAction")
    verify(action.visible)
    compare(action.text, "Show all presets")
    mouseClick(action, action.width / 2, action.height / 2)
    compare(fixture.resets, 1)
    fixture.layout = { rows: [], order: [], count: 0, total: 0 }
    wait(20)
    compare(action.text, "Refresh")
    mouseClick(action, action.width / 2, action.height / 2)
    compare(fixture.refreshed, 1)
    fixture.busy = true
    verify(!action.enabled, "a refresh already running is not asked for again")
  }

  function test_unavailable_catalog_and_unreloaded_apps_are_stated_in_place() {
    fixture.reloadPending = "Ghostty and tmux still show the previous palette."
    fixture.error = "The theme helper is unavailable. Nothing was changed."
    var retry = child(panel, "retry")
    tryVerify(function() { return retry.visible && retry.width > 0 && retry.mapToItem(panel, 0, 0).y >= 0 })
    waitForRendering(panel)
    mouseClick(retry, retry.width / 2, retry.height / 2)
    compare(fixture.refreshed, 1)
  }

  function test_the_panel_stays_inside_the_room_it_is_given_and_keeps_the_ring_in_view() {
    var layout = catalog("catppuccin-mocha")
    for (var i = 0; i < 6; i++) {
      var id = "extra-" + i
      layout.rows.push({ family: "Extra " + i, first: true, members: [preset(id, "Extra " + i, "Extra " + i, "Dark", "dark", flexoki, false)] })
      layout.order.push(id)
    }
    layout.count = layout.order.length
    layout.total = layout.order.length
    fixture.layout = layout
    // A short output: the grid gives way, the header, controls and footer do not.
    panel.maximumHeight = 420
    wait(20)
    verify(panel.implicitHeight <= panel.maximumHeight, "panel " + panel.implicitHeight + " fits " + panel.maximumHeight)
    var grid = child(panel, "grid")
    verify(grid.height >= panel.tileHeight, "at least one row of tiles stays visible")
    verify(grid.contentHeight > grid.height, "the rest scrolls")
    fixture.focusedId = "extra-5"
    var tile = child(grid, "tile-extra-5")
    tryVerify(function() {
      var top = tile.mapToItem(grid, 0, 0).y
      return top - panel.ringReach >= -0.5 && top + tile.height + panel.ringReach <= grid.height + 0.5
    }, 1000, "the ringed tile scrolls into view")
    fixture.focusedId = "catppuccin-mocha"
    var first = child(grid, "tile-catppuccin-mocha")
    tryVerify(function() { return first.mapToItem(grid, 0, 0).y - panel.ringReach >= -0.5 }, 1000, "and back up")
  }

  function test_controls_fit_and_render() {
    var search = child(panel, "search")
    var modes = child(panel, "modeLight")
    verify(modes.mapToItem(panel, modes.width, 0).x <= panel.width + 0.5, "the mode filter stays inside the panel")
    verify(search.width > modes.width, "the search keeps the room it needs")
    var last = child(child(panel, "grid"), "tile-catppuccin-latte")
    verify(last.mapToItem(panel, last.width + panel.ringReach, 0).x <= panel.width + 0.5, "a row and its ring fit")
    verify(panel.tileWidth * panel.columns + panel.tileGap * (panel.columns - 1) + panel.familyWidth + theme.spaceMedium <= panel.width + 0.5,
      "a full family of four fits")
    var capture = grabImage(panel)
    verify(capture.width > 0 && capture.height > 0)
    if (screenshotPath !== "") capture.save(screenshotPath)
  }
}
