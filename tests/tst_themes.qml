import QtQuick
import QtTest
import "production" as Shell
import "production/shared" as Shared

// The production Themes surfaces over one fixture store: the floating
// switcher and the Control Center panel. What the carousel holds, where the keys lead and what the schedule
// says are native policy covered by qml-core's tests and themes.js; this covers
// what a person does with the picker, and fails on any Qt warning.
TestCase {
  id: testCase
  name: "ThemesPicker"
  when: windowShown
  visible: true
  property string screenshotPath: ""
  property string settingsScreenshotPath: ""
  // A real image a theme is linked to, copied in beside this file.
  property string wallpaperPath: ""
  width: 1100
  height: 1500
  Shared.Theme { id: theme }

  // An entry exactly as `themes.carousel` shapes one, with a real palette.
  function preset(id, name, mode, colors, current, wallpaper) {
    return {
      id: id, name: name, mode: mode, modeLabel: mode === "light" ? "Light" : "Dark", wallpaper: wallpaper || "",
      current: current, base: colors[0], mantle: colors[1], crust: colors[0], surface: colors[2],
      overlay: colors[3], text: colors[4], subtext: colors[5], accent: colors[6],
      red: colors[7], green: colors[8], yellow: colors[9]
    }
  }
  readonly property var mocha: ["#1e1e2e", "#181825", "#313244", "#6c7086", "#cdd6f4", "#a6adc8", "#b4befe", "#f38ba8", "#a6e3a1", "#f9e2af"]
  readonly property var latte: ["#eff1f5", "#e6e9ef", "#ccd0da", "#6c6f85", "#4c4f69", "#5c5f77", "#7287fd", "#d20f39", "#40a02b", "#df8e1d"]
  readonly property var nord: ["#2e3440", "#3b4252", "#434c5e", "#7f848e", "#e5e9f0", "#d8dee9", "#81a1c1", "#bf616a", "#a3be8c", "#ebcb8b"]
  function strip(count) {
    var items = [
      preset("catppuccin-mocha", "Catppuccin Mocha", "dark", mocha, true),
      preset("catppuccin-latte", "Catppuccin Latte", "light", latte, false),
      preset("nord", "Nord", "dark", nord, false, wallpaperPath)
    ]
    for (var i = 0; i < (count || 0); i++) items.push(preset("extra-" + i, "Extra " + i, "dark", nord, false))
    return { items: items, order: items.map(function (item) { return item.id }), count: items.length }
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
    property var carousel: ({ items: [], order: [], count: 0 })
    property bool panelOpen: true
    property string focusedId: "catppuccin-mocha"
    property string current: "catppuccin-mocha"
    property string mode: "dark"
    property var slots: ({ dark: "catppuccin-mocha", light: "catppuccin-latte" })
    property var appearance: ({ mode: "dark", place: "Berlin", auto: { source: "off", lightAt: "07:00", darkAt: "19:00" } })
    readonly property string autoSource: appearance.auto.source
    readonly property string appearanceChoice: autoSource !== "off" ? "auto" : mode
    property string scheduleText: ""
    property bool busy: false
    property string error: ""
    property string actionError: ""
    property string reloadPending: ""
    property bool motion: true
    property bool sound: false
    property var calls: []
    function record(call) { calls = calls.concat([call]) }
    function find(id) {
      for (var i = 0; i < carousel.items.length; i++) if (carousel.items[i].id === id) return carousel.items[i]
      return null
    }
    function nameOf(id) { var found = find(id); return found ? found.name : id }
    function step(direction) { record("step " + direction) }
    function choose(id, now) { record("choose " + id + (now ? " now" : "")) }
    function wear(id) { record("wear " + id) }
    function keep() { record("keep") }
    function cancel() { record("cancel") }
    function setAppearance(value) { record("appearance " + value) }
    function setAuto(source, lightAt, darkAt) { record(["auto", source].concat(lightAt ? [lightAt, darkAt] : []).join(" ")) }
    function assign(slot, id) { record("assign " + slot + " " + id) }
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
  Shell.ThemeSettingsPanel {
    id: settings
    theme: theme
    store: fixture
    x: theme.panelMargin
    y: 440
    width: theme.themeSettingsWidth - theme.panelMargin * 2
  }
  // A picture on its own, for what it paints rather than where a panel puts it.
  Rectangle {
    id: loneHolder
    x: 900
    y: 1380
    width: 100
    height: 80
    color: theme.crust
    Shared.ThemePicture { id: lone; anchors.fill: parent }
  }
  SignalSpy { id: closes; target: panel; signalName: "closeRequested" }
  SignalSpy { id: browses; target: settings; signalName: "browseRequested" }

  function child(parentItem, name) {
    var item = findChild(parentItem, name)
    verify(item !== null, "Missing " + name)
    return item
  }
  function press(key, modifiers, text) {
    var event = { key: key, modifiers: modifiers || Qt.NoModifier, text: text || "", accepted: false }
    panel.handleKey(event)
    return event.accepted
  }
  function click(item) {
    mouseClick(item, item.width / 2, item.height / 2)
  }
  function init() {
    failOnWarning(/.?/)
    fixture.panelOpen = true
    fixture.focusedId = "catppuccin-mocha"
    fixture.current = "catppuccin-mocha"
    fixture.mode = "dark"
    fixture.slots = ({ dark: "catppuccin-mocha", light: "catppuccin-latte" })
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "off", lightAt: "07:00", darkAt: "19:00" } })
    fixture.scheduleText = ""
    fixture.busy = false
    fixture.error = ""
    fixture.actionError = ""
    fixture.reloadPending = ""
    fill(strip(0))
    panel.width = 1008
    panel.maximumHeight = theme.themesMaximumHeight
    settings.maximumHeight = theme.themeSettingsMaximumHeight
    child(settings, "gallery").contentY = 0
    panel.forceActiveFocus()
    fixture.calls = []
    closes.clear()
    browses.clear()
    wait(20)
  }

  function test_the_switcher_shows_every_preset_around_a_live_scene() {
    verify(findChild(panel, "appearanceAuto") === null && findChild(panel, "settings") === null,
      "the switcher is the whole window: no mode, schedule or settings on it")
    var centre = child(panel, "card-catppuccin-mocha")
    tryCompare(centre, "width", panel.cardWidth)
    verify(child(centre, "scene"), "the centre draws its preset")
    compare(child(centre, "scene").preset.id, "catppuccin-mocha")
    var neighbour = child(panel, "card-catppuccin-latte")
    compare(neighbour.width, panel.sliceWidth)
    verify(neighbour.visible, "a light preset sits beside a dark one")
    verify(findChild(neighbour, "scene") === null, "a neighbour is a slice, not a second scene")
    verify(neighbour.x > centre.x + centre.width, "later entries sit to the right")
    compare(child(panel, "name").text, "Catppuccin Mocha")
    compare(child(panel, "kind").text, "󰖔  Dark · 1 of 3")
    fixture.focusedId = "catppuccin-latte"
    tryCompare(child(panel, "card-catppuccin-latte"), "width", panel.cardWidth)
    verify(child(panel, "card-catppuccin-mocha").x < child(panel, "card-catppuccin-latte").x, "the previous entry slid to the left")
    compare(child(panel, "kind").text, "󰖙  Light · 2 of 3", "what the preset on screen is the theme for")
  }

  function test_keys_switch_keep_and_put_back() {
    press(Qt.Key_Right); press(Qt.Key_Tab); press(Qt.Key_Left); press(Qt.Key_Backtab)
    compare(fixture.calls, ["step right", "step right", "step left", "step left"])
    fixture.calls = []
    verify(!press(Qt.Key_N, Qt.NoModifier, "n"), "typing is not a filter")
    verify(!press(Qt.Key_Up), "nor is there a mode on the switcher")
    verify(!press(Qt.Key_J, Qt.ControlModifier), "chords belong to the compositor")
    compare(fixture.calls, [])
    press(Qt.Key_Return)
    compare(fixture.calls, ["keep"], "Enter keeps what is on screen")
    compare(closes.count, 1)
    press(Qt.Key_Escape)
    compare(fixture.calls, ["keep", "cancel"], "Escape puts everything back")
    compare(closes.count, 2)
  }

  function test_clicking_a_neighbour_chooses_it_and_clicking_the_centre_keeps_it() {
    click(child(panel, "card-nord"))
    compare(fixture.calls, ["choose nord now"])
    compare(closes.count, 0, "and leaves the picker open for the next one")
    click(child(panel, "card-catppuccin-mocha"))
    compare(fixture.calls.slice(-1), ["keep"])
    compare(closes.count, 1)
  }

  function test_appearance_is_light_dark_or_auto() {
    for (var name of ["appearanceLight", "appearanceDark", "appearanceAuto"]) click(child(settings, name))
    compare(fixture.calls, ["appearance light", "appearance dark", "appearance auto"])
    verify(child(settings, "appearanceDark").selected)
    verify(!child(settings, "autoSun").visible, "the schedule's choices wait for Auto")
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "sun", lightAt: "07:00", darkAt: "19:00" } })
    fixture.scheduleText = "Light at 07:12 · sunrise in Berlin"
    wait(20)
    verify(child(settings, "appearanceAuto").selected)
    verify(child(settings, "autoSun").visible && child(settings, "autoSun").selected)
    compare(child(settings, "schedule").text, "Light at 07:12 · sunrise in Berlin")
    verify(!child(settings, "lightAt").visible, "times appear only for a fixed schedule")
    fixture.calls = []
    click(child(settings, "autoSchedule"))
    click(child(settings, "autoSun"))
    compare(fixture.calls, ["auto schedule", "auto sun"])
    fixture.appearance = ({ mode: "dark", place: null, auto: { source: "schedule", lightAt: "07:00", darkAt: "19:00" } })
    wait(20)
    verify(!child(settings, "autoSun").enabled, "a timezone without a city cannot follow the sun")
  }

  function test_the_schedule_times() {
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "schedule", lightAt: "07:00", darkAt: "19:00" } })
    fixture.scheduleText = "Light at 07:00 · on a schedule"
    wait(20)
    var lightAt = child(settings, "lightAt")
    verify(lightAt.visible)
    compare(lightAt.text, "07:00")
    lightAt.forceActiveFocus()
    lightAt.selectAll()
    keyClick(Qt.Key_0); keyClick(Qt.Key_6); keyClick(Qt.Key_Colon); keyClick(Qt.Key_3); keyClick(Qt.Key_0)
    keyClick(Qt.Key_Return)
    compare(fixture.calls, ["auto schedule 06:30 19:00"], "Enter sends the time and nothing else")
    // The field loses focus again as the row changes: an edit already sent
    // is not sent twice.
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "sun", lightAt: "07:00", darkAt: "19:00" } })
    wait(20)
    compare(fixture.calls.length, 1)
    // The helper's word differs from the edit (another picker, a refusal):
    // the field shows what is saved, not what was typed.
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "schedule", lightAt: "05:45", darkAt: "19:00" } })
    wait(20)
    compare(lightAt.text, "05:45", "the field follows the saved time")
    var darkAt = child(settings, "darkAt")
    darkAt.forceActiveFocus()
    darkAt.selectAll()
    keyClick(Qt.Key_2); keyClick(Qt.Key_5)
    keyClick(Qt.Key_Return)
    compare(fixture.calls.length, 1, "an unfinished time is not sent")
    darkAt.forceActiveFocus()
    keyClick(Qt.Key_Escape)
    compare(darkAt.text, "19:00", "Escape in a field puts its saved time back")
    verify(!darkAt.activeFocus && settings.activeFocus, "and hands the keyboard to the panel, whose Escape closes it")
  }

  function test_the_switcher_pictures_a_preset_over_its_wallpaper() {
    fixture.focusedId = "nord"
    var centre = child(panel, "card-nord")
    tryCompare(centre, "width", panel.cardWidth)
    tryVerify(function() { return child(child(centre, "scene"), "wallpaper").shown }, 2000, "the linked image is drawn behind the scene")
    fixture.focusedId = "catppuccin-mocha"
    var plain = child(panel, "card-catppuccin-mocha")
    tryCompare(plain, "width", panel.cardWidth)
    verify(!child(child(plain, "scene"), "wallpaper").shown, "a preset linked to nothing shows its base")
  }

  function test_the_gallery_shows_every_preset_with_its_modes() {
    for (var id of ["catppuccin-mocha", "catppuccin-latte", "nord"]) verify(child(settings, "theme-" + id).visible, id + " has a tile")
    compare(settings.columns, 3, "three tiles to a row at the panel's width")
    var mocha = child(settings, "theme-catppuccin-mocha")
    compare(child(mocha, "name").text, "Catppuccin Mocha")
    verify(child(mocha, "caption").visible, "the preset on screen says so")
    compare(child(mocha, "caption").text, "On screen")
    verify(!child(child(settings, "theme-nord"), "caption").visible, "the rest carry their name alone")
    verify(child(settings, "dark-catppuccin-mocha").selected && child(settings, "light-catppuccin-latte").selected,
      "each mode's theme fills its pin on its tile")
    verify(!child(settings, "light-catppuccin-mocha").selected && !child(settings, "dark-nord").selected)
    compare([child(settings, "light-nord").text, child(settings, "dark-nord").text], ["󰖙", "󰖔"])
    tryVerify(function() { return child(child(settings, "theme-nord"), "wallpaper").shown }, 2000, "a tile shows its theme's linked image")
    verify(!child(child(settings, "theme-catppuccin-latte"), "wallpaper").shown, "and one linked to nothing its base")
    // The name sits on the preset's own mantle, so a light theme's tile reads
    // dark on light like the theme itself.
    var shade = child(child(settings, "theme-catppuccin-latte"), "shade").gradient.stops[1].color
    var mantle = Qt.color(latte[1])
    verify(Math.abs(shade.r - mantle.r) < 0.01 && Math.abs(shade.g - mantle.g) < 0.01 && Math.abs(shade.b - mantle.b) < 0.01,
      "the shade is the preset's mantle, not " + shade)
    compare(shade.a, theme.themeTileShadeOpacity, 0.01)
    // Giving a preset to the other mode changes that tile alone.
    fixture.slots = ({ dark: "catppuccin-mocha", light: "nord" })
    wait(20)
    verify(child(settings, "light-nord").selected && !child(settings, "light-catppuccin-latte").selected)
    fixture.current = "nord"
    wait(20)
    verify(child(child(settings, "theme-nord"), "caption").visible)
    verify(!child(mocha, "caption").visible)
  }

  function test_a_card_wears_its_preset_or_gives_it_to_a_mode() {
    click(child(settings, "light-nord"))
    click(child(settings, "dark-catppuccin-latte"))
    compare(fixture.calls, ["assign light nord", "assign dark catppuccin-latte"], "a preset of either kind may be either mode's theme")
    fixture.calls = []
    var wear = child(settings, "wear-nord")
    click(wear)
    compare(fixture.calls, ["wear nord"], "the tile wears it now, without the switcher's highlight")
    fixture.calls = []
    wear.forceActiveFocus(Qt.TabFocusReason)
    keyClick(Qt.Key_Return)
    compare(fixture.calls, ["wear nord"], "and so does Enter")
    keyClick(Qt.Key_Tab)
    verify(child(settings, "light-nord").activeFocus, "Tab moves on to the tile's pins")
    keyClick(Qt.Key_Return)
    compare(fixture.calls.slice(-1), ["assign light nord"], "and Enter on a pin gives it that mode")
    click(child(settings, "browse"))
    compare(browses.count, 1, "Open switcher floats the switcher")
  }

  function test_an_empty_catalog_offers_a_refresh() {
    fill({ items: [], order: [], count: 0 })
    fixture.focusedId = ""
    wait(20)
    verify(!child(panel, "carousel").visible)
    compare(child(panel, "name").text, "")
    var action = child(panel, "emptyAction")
    compare(action.text, "Refresh")
    click(action)
    compare(fixture.calls, ["refresh"])
  }

  function test_problems_are_stated_in_place() {
    fixture.reloadPending = "Ghostty still shows the previous palette."
    fixture.error = "The theme helper is unavailable. Nothing was changed."
    var retry = child(panel, "retry")
    tryVerify(function() { return retry.visible && retry.width > 0 && retry.mapToItem(panel, 0, 0).y >= 0 })
    waitForRendering(panel)
    click(retry)
    compare(fixture.calls, ["refresh"])
  }

  function test_the_picker_fits_the_room_it_is_given() {
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

  function test_the_control_center_panel_fits_its_width() {
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "schedule", lightAt: "07:00", darkAt: "19:00" } })
    fixture.scheduleText = "Dark at 19:00 · on a schedule"
    wait(20)
    for (var name of ["appearanceAuto", "autoSchedule", "darkAt", "browse", "light-nord", "dark-nord", "wear-nord"]) {
      var control = child(settings, name)
      verify(control.visible, name + " is shown")
      var left = control.mapToItem(settings, 0, 0).x
      verify(left >= -0.5 && left + control.width <= settings.width + 0.5, name + " stays inside the panel")
    }
    var lightAt = child(settings, "lightAt")
    verify(child(settings, "autoSchedule").mapToItem(settings, 0, 0).y + child(settings, "autoSchedule").height <= lightAt.mapToItem(settings, 0, 0).y,
      "the times sit below their choice")
  }

  function test_the_gallery_scrolls_inside_the_room_it_is_given() {
    fill(strip(8))
    settings.maximumHeight = 620
    wait(20)
    verify(settings.implicitHeight <= settings.maximumHeight + 0.5, "panel " + settings.implicitHeight + " fits " + settings.maximumHeight)
    var gallery = child(settings, "gallery")
    verify(gallery.contentHeight > gallery.height, "the gallery scrolls rather than growing")
    compare(gallery.contentY, 0)
    // A switch made elsewhere brings the preset now on screen into view.
    fixture.current = "extra-5"
    tryVerify(function() {
      var shown = child(settings, "theme-extra-5").mapToItem(gallery, 0, 0).y
      return shown >= -0.5 && shown + child(settings, "theme-extra-5").height <= gallery.height + 0.5
    }, 1000, "the card on screen is in view")
    fixture.current = "catppuccin-mocha"
    wait(20)
    compare(gallery.contentY, 0, "and the first card brings the gallery back to its top")
    var last = child(settings, "dark-extra-7")
    last.forceActiveFocus(Qt.TabFocusReason)
    wait(20)
    var card = child(settings, "theme-extra-7")
    var top = card.mapToItem(gallery, 0, 0).y
    verify(top >= -0.5 && top + card.height <= gallery.height + 0.5, "the keyboard brings its card into view")
    // A narrow panel holds one card to a row rather than squeezing two.
    settings.width = theme.themeGalleryCardMinimum + theme.spaceMedium
    wait(20)
    compare(settings.columns, 1)
    settings.width = theme.themeSettingsWidth - theme.panelMargin * 2
  }

  function test_a_short_output_scrolls_the_panel_rather_than_overflowing() {
    fill(strip(8))
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "schedule", lightAt: "07:00", darkAt: "19:00" } })
    fixture.scheduleText = "Dark at 19:00 · on a schedule"
    settings.maximumHeight = 300
    wait(20)
    verify(settings.implicitHeight <= settings.maximumHeight + 0.5, "panel " + settings.implicitHeight + " stays inside " + settings.maximumHeight)
    var view = child(settings, "view")
    var gallery = child(settings, "gallery")
    verify(view.contentHeight > view.height, "what does not fit scrolls")
    verify(gallery.height >= settings.tileHeight - 0.5, "the gallery keeps a whole row of tiles")
    // The keyboard reaches the last tile through both scrolls.
    child(settings, "dark-extra-7").forceActiveFocus(Qt.TabFocusReason)
    wait(20)
    var tile = child(settings, "theme-extra-7")
    var top = tile.mapToItem(view, 0, 0).y
    verify(top >= -0.5 && top < view.height, "the focused tile is in the panel's view")
    var inGallery = tile.mapToItem(gallery, 0, 0).y
    verify(inGallery >= -0.5 && inGallery + tile.height <= gallery.height + 0.5, "and whole in the gallery")
    child(settings, "view").contentY = 0
  }

  function test_a_picture_follows_the_corners_it_is_given() {
    lone.path = wallpaperPath
    tryVerify(function() { return lone.shown }, 2000)
    // A Canvas grabbed alone yields none of its paint; its holder holds it.
    var corner = function() { return grabImage(loneHolder).pixel(1, 1) }
    lone.topLeftRadius = 0
    waitForRendering(loneHolder)
    var square = corner()
    verify(!Qt.colorEqual(square, theme.crust), "a square corner is painted with the picture")
    lone.topLeftRadius = lone.height / 2
    tryVerify(function() { return Qt.colorEqual(corner(), theme.crust) }, 1000, "a new radius is clipped without anything else changing")
    lone.topLeftRadius = 0
    tryVerify(function() { return Qt.colorEqual(corner(), square) }, 1000, "and a smaller one paints the corner again")
    lone.path = ""
  }


  function test_both_views_render() {
    var centre = child(panel, "card-catppuccin-mocha")
    verify(Math.abs(centre.x + centre.width / 2 - child(panel, "carousel").width / 2) < 1, "the centre is centred")
    var capture = grabImage(panel)
    verify(capture.width > 0 && capture.height > 0)
    if (screenshotPath !== "") capture.save(screenshotPath)
    fixture.appearance = ({ mode: "dark", place: "Berlin", auto: { source: "sun", lightAt: "07:00", darkAt: "19:00" } })
    fixture.scheduleText = "Light at 07:12 · sunrise in Berlin"
    waitForRendering(settings)
    var panelCapture = grabImage(settings)
    verify(panelCapture.width > 0 && panelCapture.height > 0)
    if (settingsScreenshotPath !== "") panelCapture.save(settingsScreenshotPath)
  }
}
