import QtQuick
import QtTest
import "production" as Shell
import "production/shared" as Shared

// The production Themes panel over a fixture store. Grouping, movement and
// what the preview shows are native policy covered by qml-core's tests and
// themes.js; this covers what a person does with the panel itself, and fails
// on any Qt warning.
TestCase {
  id: testCase
  name: "ThemesPanel"
  when: windowShown
  visible: true
  property string screenshotPath: ""
  width: 520
  height: 900
  Shared.Theme { id: theme }

  // A preset exactly as the native layout carries one, with a real palette.
  function preset(id, name, variant, mode, colors, current) {
    return {
      id: id, name: name, variant: variant, mode: mode, modeLabel: mode === "light" ? "Light" : "Dark",
      current: current, base: colors[0], mantle: colors[1], crust: colors[0], surface: colors[2],
      overlay: colors[3], text: colors[4], subtext: colors[5], accent: colors[6],
      red: colors[7], green: colors[8], yellow: colors[9]
    }
  }
  readonly property var mocha: ["#1e1e2e", "#181825", "#313244", "#6c7086", "#cdd6f4", "#a6adc8", "#b4befe", "#f38ba8", "#a6e3a1", "#f9e2af"]
  readonly property var latte: ["#eff1f5", "#e6e9ef", "#ccd0da", "#8c8fa1", "#4c4f69", "#6c6f85", "#7287fd", "#d20f39", "#40a02b", "#df8e1d"]
  readonly property var nord: ["#2e3440", "#3b4252", "#434c5e", "#7f848e", "#e5e9f0", "#d8dee9", "#81a1c1", "#bf616a", "#a3be8c", "#ebcb8b"]
  function catalog(current) {
    var rows = [
      { family: "Catppuccin", first: true, members: [
        preset("catppuccin-mocha", "Catppuccin Mocha", "Mocha", "dark", mocha, current === "catppuccin-mocha"),
        preset("catppuccin-latte", "Catppuccin Latte", "Latte", "light", latte, current === "catppuccin-latte")] },
      { family: "More", first: true, members: [
        preset("nord", "Nord", "Nord", "dark", nord, current === "nord")] }
    ]
    return { rows: rows, order: ["catppuccin-mocha", "catppuccin-latte", "nord"], count: 3, total: 3 }
  }
  QtObject {
    id: fixture
    property int columns: 0
    property bool panelOpen: true
    property string query: ""
    property string mode: "all"
    property string error: ""
    property string actionError: ""
    property string reloadPending: ""
    property string applying: ""
    property bool busy: false
    property string current: "catppuccin-mocha"
    property string currentName: "Catppuccin Mocha"
    readonly property bool filtered: query !== "" || mode !== "all"
    property var layout: testCase.catalog("catppuccin-mocha")
    property string focusedId: "catppuccin-mocha"
    readonly property var preview: {
      for (var r = 0; r < layout.rows.length; r++)
        for (var m = 0; m < layout.rows[r].members.length; m++)
          if (layout.rows[r].members[m].id === focusedId) return layout.rows[r].members[m]
      return null
    }
    property var steps: []
    property var applied: []
    property int refreshed: 0
    property int resets: 0
    function refresh() { refreshed++ }
    function highlight(id) { focusedId = id }
    function step(direction) { steps = steps.concat([direction]) }
    function resetFilters() { resets++; query = ""; mode = "all" }
    function apply(id) { applied = applied.concat([id]); return true }
    function applyFocused() { return apply(focusedId) }
    function failure(code) { return "" }
  }
  Rectangle { anchors.fill: parent; color: theme.crust }
  Shell.ThemePanel {
    id: panel
    theme: theme
    store: fixture
    x: theme.panelMargin
    y: theme.panelMargin
    width: 448
  }

  function child(parentItem, name) {
    var item = findChild(parentItem, name)
    verify(item !== null, "Missing " + name)
    return item
  }
  function press(key) {
    panel.handleKey({ key: key, modifiers: Qt.NoModifier, accepted: false })
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
    fixture.current = "catppuccin-mocha"
    fixture.currentName = "Catppuccin Mocha"
    fixture.layout = catalog("catppuccin-mocha")
    fixture.focusedId = "catppuccin-mocha"
    fixture.steps = []
    fixture.applied = []
    fixture.refreshed = 0
    fixture.resets = 0
    panel.maximumHeight = theme.themesMaximumHeight
    // Every test starts as the panel does when it opens: the pointer unseen.
    fixture.panelOpen = false
    fixture.panelOpen = true
    wait(20)
  }
  // A person's pointer arrives somewhere and keeps moving.
  function point(item) {
    mouseMove(item, item.width / 2 - 4, item.height / 2)
    mouseMove(item, item.width / 2, item.height / 2)
  }

  function test_the_panel_tells_the_layout_how_wide_a_row_is() {
    compare(fixture.columns, panel.columns)
  }

  function test_families_label_their_rows_and_tiles_sample_themselves() {
    var labels = []
    var grid = child(panel, "grid")
    for (var name of ["catppuccin-mocha", "catppuccin-latte", "nord"]) verify(child(grid, "tile-" + name).visible)
    compare(child(child(grid, "tile-catppuccin-mocha"), "appliedMark").visible, true)
    compare(child(child(grid, "tile-catppuccin-latte"), "appliedMark").visible, false)
    compare(child(child(grid, "tile-nord"), "appliedMark").visible, false)
  }

  function test_the_preview_names_what_it_shows_and_what_it_would_replace() {
    compare(child(panel, "previewName").text, "Catppuccin Mocha")
    compare(child(panel, "apply").text, "Reapply", "the applied theme can be republished")
    fixture.focusedId = "nord"
    wait(20)
    compare(child(panel, "previewName").text, "Nord")
    compare(child(panel, "apply").text, "Apply")
    compare(child(panel, "previewCaption").text, "Dark · Currently Catppuccin Mocha")
    compare(child(panel, "scene").preset.id, "nord", "the scene draws the previewed palette")
    fixture.applying = "nord"
    wait(20)
    compare(child(panel, "apply").text, "Applying…")
  }

  function test_arrow_keys_and_hjkl_step_and_enter_applies_the_preview() {
    press(Qt.Key_Right); press(Qt.Key_L); press(Qt.Key_Down); press(Qt.Key_J)
    press(Qt.Key_Left); press(Qt.Key_H); press(Qt.Key_Up); press(Qt.Key_K)
    compare(fixture.steps, ["right", "right", "down", "down", "left", "left", "up", "up"])
    fixture.focusedId = "nord"
    press(Qt.Key_Return)
    compare(fixture.applied, ["nord"])
    press(Qt.Key_R)
    compare(fixture.refreshed, 1)
    // A chord belongs to the compositor, not the panel.
    panel.handleKey({ key: Qt.Key_J, modifiers: Qt.ControlModifier, accepted: false })
    compare(fixture.steps.length, 8)
  }

  function test_the_search_field_steps_until_there_is_text_to_edit() {
    var search = child(panel, "search")
    search.forceActiveFocus()
    keyClick(Qt.Key_Down)
    keyClick(Qt.Key_Right)
    compare(fixture.steps, ["down", "right"], "with nothing typed, arrows move the preview")
    keyClick(Qt.Key_N)
    keyClick(Qt.Key_O)
    compare(fixture.query, "no", "typing reaches the store's query")
    keyClick(Qt.Key_Left)
    compare(fixture.steps, ["down", "right"], "with text typed, left and right move the caret")
    compare(search.cursorPosition, 1)
    keyClick(Qt.Key_Up)
    compare(fixture.steps, ["down", "right", "up"], "up and down still move the preview")
    fixture.focusedId = "nord"
    keyClick(Qt.Key_Return)
    compare(fixture.applied, ["nord"], "Return in the field applies the preview")
  }

  function test_pointing_previews_and_clicking_applies() {
    var tile = child(child(panel, "grid"), "tile-nord")
    point(tile)
    compare(fixture.focusedId, "nord", "pointing at a tile previews it")
    mouseClick(tile, tile.width / 2, tile.height / 2)
    compare(fixture.applied, ["nord"])
    var applied = child(child(panel, "grid"), "tile-catppuccin-mocha")
    mouseClick(applied, applied.width / 2, applied.height / 2)
    compare(fixture.applied, ["nord"], "clicking the applied theme previews it rather than republishing")
    compare(fixture.focusedId, "catppuccin-mocha")
    var button = child(panel, "apply")
    mouseClick(button, button.width / 2, button.height / 2)
    compare(fixture.applied, ["nord", "catppuccin-mocha"], "Reapply is the deliberate way to republish")
  }

  function test_a_resting_pointer_never_takes_the_preview_from_the_keyboard() {
    var grid = child(panel, "grid")
    // The panel opens under a pointer that is already over a tile.
    var nord = child(grid, "tile-nord")
    mouseMove(nord, nord.width / 2, nord.height / 2)
    compare(fixture.focusedId, "catppuccin-mocha", "a pointer that was already there has chosen nothing")
    point(nord)
    compare(fixture.focusedId, "nord", "moving it does")
    // The keyboard moves on, and the grid is laid out again beneath the
    // motionless pointer, as it is on every keystroke of a search.
    fixture.focusedId = "catppuccin-mocha"
    fixture.layout = catalog("catppuccin-mocha")
    wait(50)
    nord = child(grid, "tile-nord")
    mouseMove(nord, nord.width / 2, nord.height / 2)
    compare(fixture.focusedId, "catppuccin-mocha", "a tile arriving under the pointer does not take the preview")
    fixture.focusedId = "catppuccin-latte"
    press(Qt.Key_Return)
    compare(fixture.applied, ["catppuccin-latte"], "so Enter applies what the keyboard chose")
    // Reopening forgets where the pointer was.
    fixture.panelOpen = false
    fixture.panelOpen = true
    mouseMove(nord, nord.width / 2 + 3, nord.height / 2)
    compare(fixture.focusedId, "catppuccin-latte")
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
  }

  function test_an_empty_grid_offers_its_own_way_out() {
    fixture.query = "zzz"
    fixture.layout = { rows: [], order: [], count: 0, total: 3 }
    fixture.focusedId = ""
    wait(20)
    verify(!child(panel, "hero").visible, "nothing shown previews nothing")
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

  function test_the_panel_stays_inside_the_room_it_is_given_and_keeps_the_preview_in_view() {
    var rows = catalog("catppuccin-mocha").rows
    var order = ["catppuccin-mocha", "catppuccin-latte", "nord"]
    for (var i = 0; i < 6; i++) {
      var id = "extra-" + i
      rows.push({ family: "Extra " + i, first: true, members: [preset(id, "Extra " + i, "Extra " + i, "dark", nord, false)] })
      order.push(id)
    }
    fixture.layout = { rows: rows, order: order, count: order.length, total: order.length }
    // A short output: the grid gives way, the preview and controls do not.
    panel.maximumHeight = 420
    wait(20)
    verify(panel.implicitHeight <= panel.maximumHeight, "panel " + panel.implicitHeight + " fits " + panel.maximumHeight)
    var grid = child(panel, "grid")
    verify(grid.height >= panel.tileHeight, "at least one row of tiles stays visible")
    verify(grid.contentHeight > grid.height, "the rest scrolls")
    // Moving the preview to a tile below the fold brings that tile, and its
    // ring, into view.
    fixture.focusedId = "extra-5"
    var tile = child(grid, "tile-extra-5")
    tryVerify(function() {
      var top = tile.mapToItem(grid, 0, 0).y
      return top - panel.ringReach >= -0.5 && top + tile.height + panel.ringReach <= grid.height + 0.5
    }, 1000, "the highlighted tile scrolls into view")
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
    verify(last.mapToItem(panel, last.width + panel.ringReach, 0).x <= panel.width + 0.5, "a full row and its ring fit")
    var capture = grabImage(panel)
    verify(capture.width > 0 && capture.height > 0)
    if (screenshotPath !== "") capture.save(screenshotPath)
  }
}
