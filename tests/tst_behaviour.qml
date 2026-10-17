import QtQuick
import QtTest
import "../projects/shared" as Shared
import "../projects/shared/Contract.js" as Contract

// Component behaviour (SIL-238): the shared controls report hover and press
// through state layers and ripples and move on the theme's springs, each from
// the contract's slots. A preset gives none of these, so its controls behave
// exactly as before; Material 3 Expressive gives all of them.
//
//   bash tests/palette.sh projects/shared tests/tst_behaviour.qml <Qt QML imports>
TestCase {
  id: testCase
  name: "ComponentBehaviour"
  when: windowShown
  visible: true
  width: 480
  height: 240

  Shared.Theme { id: preset }
  Shared.Theme { id: material }

  Shared.ActionButton { id: presetButton; theme: preset; text: "Preset"; x: 10; y: 10; width: 120 }
  Shared.ActionButton { id: materialButton; theme: material; text: "Material"; x: 150; y: 10; width: 120 }
  Shared.ControlSwitch { id: presetSwitch; theme: preset; x: 10; y: 80 }
  Shared.ControlSwitch { id: materialSwitch; theme: material; x: 150; y: 80 }
  Shared.GlyphButton { id: materialGlyph; theme: material; glyph: "x"; x: 300; y: 10 }
  Rectangle {
    id: presetHost; x: 10; y: 150; width: 80; height: 40; radius: preset.radius
    Shared.FocusRing { id: presetRing; theme: preset }
  }
  Rectangle {
    id: materialHost; x: 150; y: 150; width: 80; height: 40; radius: material.radius
    Shared.FocusRing { id: materialRing; theme: material }
  }

  // A control that fills a surface which clips, as a card's full-width row
  // does, leaves an outer ring no room.
  Rectangle {
    x: 300; y: 150; width: 80; height: 40; clip: true
    Rectangle {
      id: crampedHost; anchors.fill: parent; radius: material.radius
      Shared.FocusRing { id: crampedRing; theme: material }
    }
  }

  // The highest point the material switch's handle reached since `reset`.
  property real handlePeak: 0
  Connections { target: materialSwitch; function onHandleCentreChanged() { testCase.handlePeak = Math.max(testCase.handlePeak, materialSwitch.handleCentre) } }

  function contractSlots() {
    var request = new XMLHttpRequest(), slots = null
    request.onreadystatechange = function() {
      if (request.readyState === XMLHttpRequest.DONE) slots = Contract.slots(JSON.parse(request.responseText))
    }
    request.open("GET", Qt.resolvedUrl("../projects/shared/theme-contract.json"))
    request.send()
    tryVerify(function() { return slots !== null }, 5000, "the contract loads")
    return slots
  }
  function themeFolder(id) {
    var request = new XMLHttpRequest(), document = null
    request.onreadystatechange = function() {
      if (request.readyState === XMLHttpRequest.DONE) document = JSON.parse(request.responseText)
    }
    request.open("GET", Qt.resolvedUrl("../themes/" + id + "/theme.json"))
    request.send()
    tryVerify(function() { return document !== null }, 5000, id + " loads")
    return document
  }
  // The press ripple a control's state layer holds, if any.
  function ripple(item) {
    if (!item) return null
    if (item.reach !== undefined && item.strength !== undefined) return item
    for (var i = 0; i < item.children.length; i++) {
      var found = ripple(item.children[i])
      if (found) return found
    }
    return null
  }
  function handle(control) {
    for (var i = 0; i < control.children.length; i++) {
      var child = control.children[i]
      if (child.radius !== undefined && child.visible && child.width === child.height && child.width < control.height) return child
    }
    return null
  }

  function initTestCase() {
    material.applyTheme(themeFolder("material-expressive-dark"), contractSlots())
    // The controls ease into the theme's colours on its effects spring.
    tryCompare(materialButton.background, "color", material.cardColor, 2000)
  }
  function init() { failOnWarning(/.?/) }

  function test_a_preset_keeps_its_motion_and_its_shapes() {
    compare(preset.radiusPressed, preset.radius)
    compare(preset.focusRingWidth, preset.hairline)
    compare(preset.focusRingGap, -preset.hairline)
    compare(preset.focusRingFlare, preset.hairline)
    compare(preset.focusRingColor, preset.accent)
    compare(preset.durationState, preset.durationFast)
    compare(preset.durationEffects, preset.durationFast)
    compare(preset.easingEffects, Easing.Linear)
    compare(preset.durationSpatial, preset.durationFast)
    compare(preset.easingSpatial, preset.easingStandard)
    compare(preset.easingSpatialCurve.length, 0)
    compare(preset.durationRipple, 0)
    compare(preset.durationFocusRing, 0)
    compare(preset.switchHandleSize, preset.switchHeight - preset.switchKnobInset * 2)
    compare(preset.switchHandleOnSize, preset.switchHandleSize)
    compare(preset.switchHandlePressedSize, preset.switchHandleSize)
    compare(preset.switchStateLayerSize, 0)
    compare(preset.switchHandleOn, preset.crust)
  }

  function test_material_gives_its_springs_and_ripples() {
    compare(material.durationState, 15)
    compare(material.durationRipple, 450)
    compare(material.radiusPressed, 8)
    compare(material.focusRingGap, 2)
    compare(material.focusRingWidth, 3)
    compare(material.focusRingFlare, 8)
    compare(material.durationSpatial, 421)
    compare(material.easingSpatial, Easing.BezierSpline)
    compare(material.easingSpatialCurve.length, 48)
    compare(material.durationEffects, 150)
    compare(material.easingEffects, Easing.BezierSpline)
    compare([material.switchHandleSize, material.switchHandleOnSize, material.switchHandlePressedSize, material.switchStateLayerSize], [16, 24, 28, 40])
  }

  function test_a_preset_press_lays_its_colour_flat() {
    verify(ripple(presetButton.background) === null, "a preset grows no ripple")
    mousePress(presetButton, 20, 10)
    tryCompare(presetButton.background, "color", preset.pressColor, 1000)
    compare(presetButton.background.radius, preset.radius, "and keeps its corners")
    mouseRelease(presetButton, 20, 10)
  }

  function test_a_material_press_ripples_from_where_it_landed() {
    var wave = ripple(materialButton.background)
    verify(wave !== null, "Material grows ripples")
    compare(wave.strength, 0)
    mousePress(materialButton, 12, 8)
    // The ripple starts at the press, as a disc a fifth of the longer side.
    compare([wave.centreX, wave.centreY, wave.reach], [12, 8, Math.floor(materialButton.width * material.rippleOriginScale) / 2])
    tryVerify(function() { return wave.strength > 0 }, 500, "it fades in")
    // The container keeps its colour; the ripple is the press.
    compare(materialButton.background.color, materialButton.background.resting)
    // The corners close to Material's pressed shape on the spatial spring.
    tryCompare(materialButton.background, "radius", material.radiusPressed, 1500)
    // Let go at once: the ripple holds, then fades while it spreads on.
    mouseRelease(materialButton, 12, 8)
    var spread = (Math.sqrt(materialButton.width * materialButton.width + materialButton.height * materialButton.height) + material.ripplePadding + material.rippleSoftEdge) / 2
    tryVerify(function() { return Math.abs(wave.reach - spread) < 0.5 && Math.abs(wave.centreX - materialButton.width / 2) < 0.5 }, 1500, "it spreads to cover the button from its centre")
    tryCompare(wave, "strength", 0, 1500)
    tryCompare(materialButton.background, "radius", material.radius, 1500)
  }

  function test_a_tap_is_seen_for_the_ripple_hold() {
    var wave = ripple(materialGlyph.background)
    verify(wave !== null)
    mouseClick(materialGlyph, 4, 4)
    // Released at once, it is still shown until the hold has run out.
    wait(material.durationRippleHold / 2)
    verify(wave.strength > 0, "a tap is seen")
    tryCompare(wave, "strength", 0, 2000)
  }

  function test_the_switch_handle_grows_and_springs() {
    var presetKnob = handle(presetSwitch), knob = handle(materialSwitch)
    verify(presetKnob && knob)
    compare(presetKnob.x, preset.switchKnobInset, "a preset's handle rests at its inset")
    compare(presetKnob.width, preset.switchHandleSize)
    compare(knob.width, material.switchHandleSize)
    compare(knob.x + knob.width / 2, materialSwitch.height / 2, "off, the handle is centred in the track's round end")
    testCase.handlePeak = 0
    materialSwitch.checked = true
    var target = materialSwitch.width - materialSwitch.height / 2
    tryCompare(knob, "width", material.switchHandleOnSize, 1500)
    tryVerify(function() { return Math.abs(materialSwitch.handleCentre - target) < 0.01 }, 1500)
    // The fast spatial spring overshoots by about a tenth of its travel.
    verify(testCase.handlePeak > target + 1, "the handle overshoots on the spring: " + testCase.handlePeak + " past " + target)
    mousePress(materialSwitch, materialSwitch.width - materialSwitch.height / 2, materialSwitch.height / 2)
    tryCompare(knob, "width", material.switchHandlePressedSize, 1500)
    compare(knob.color, material.switchHandleOnActive)
    mouseRelease(materialSwitch, materialSwitch.width - materialSwitch.height / 2, materialSwitch.height / 2)
    materialSwitch.checked = false
  }

  // The focus ring a control lays over its background, if any.
  function focusRing(item) {
    if (!item) return null
    if (item.outset !== undefined && item.shown !== undefined) return item
    for (var i = 0; i < item.children.length; i++) {
      var found = focusRing(item.children[i])
      if (found) return found
    }
    return null
  }

  function test_only_the_keyboard_shows_an_outer_ring() {
    var ring = focusRing(materialButton.background)
    verify(ring !== null)
    mouseClick(materialButton, 20, 10)
    verify(materialButton.activeFocus, "a click focuses the button")
    verify(!ring.visible, "but Material's ring is for the keyboard")
    materialButton.focus = false
    materialButton.forceActiveFocus(Qt.TabFocusReason)
    verify(ring.visible, "Tab shows it")
    compare(materialButton.background.border.color, material.cardBorder, "and the button keeps its own edge")
    materialButton.focus = false
    // A preset's ring is the button's own edge, as it always was.
    verify(focusRing(presetButton.background) === null || !focusRing(presetButton.background).visible)
    presetButton.forceActiveFocus(Qt.TabFocusReason)
    compare(presetButton.background.border.color, preset.accent)
    presetButton.focus = false
  }

  function test_the_focus_ring_lies_where_the_theme_puts_it() {
    presetRing.shown = true
    compare(presetRing.anchors.margins, 0, "a preset's ring is the control's own edge")
    compare(presetRing.radius, presetHost.radius)
    compare(presetRing.border.width, preset.hairline)
    var widest = 0
    materialRing.shown = true
    tryVerify(function() { widest = Math.max(widest, materialRing.border.width); return widest >= material.focusRingFlare - 0.5 }, 1500, "Material's ring flares as it appears")
    tryCompare(materialRing.border, "width", material.focusRingWidth, 1500)
    compare(materialRing.anchors.margins, -(material.focusRingGap + material.focusRingWidth), "outside the edge, by its gap")
    compare(materialRing.radius, materialHost.radius + material.focusRingGap + material.focusRingWidth)
    presetRing.shown = false
    materialRing.shown = false
  }

  function test_a_ring_that_would_be_cut_lies_inside_the_edge() {
    crampedRing.shown = true
    verify(crampedRing.cramped, "an ancestor that clips would cut the outer ring")
    compare(crampedRing.anchors.margins, 0, "so it lies on the control's own edge")
    compare(crampedRing.radius, crampedHost.radius)
    tryCompare(crampedRing.border, "width", material.focusRingWidth, 1500)
    crampedRing.shown = false
    // With room around the control, the same theme's ring lies outside.
    materialRing.shown = true
    verify(!materialRing.cramped)
    materialRing.shown = false
  }
}
