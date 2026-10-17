import QtQuick
import QtTest
import "../projects/shared" as Shared
import "../projects/shared/Contract.js" as Contract

// A theme's own motion (SIL-240): a Cyberpunk panel sweeps its trim out from
// the cut as it appears, flickers its glitch after it and again when the
// pointer comes onto it, and comes to rest on the still frame. A preset's
// panel has no frame to move, and a user who holds the theme still sees the
// still frame at once.
//
//   bash tests/palette.sh projects/shared tests/tst_motion.qml <Qt QML imports>
TestCase {
  id: testCase
  name: "ThemeMotion"
  when: windowShown
  visible: true
  width: 260
  height: 100

  Shared.Theme { id: cyberpunk }
  Shared.Theme { id: preset }

  // A panel's own window, which a panel that closes hides and shows again.
  Window {
    id: host
    visible: false
    width: 240
    height: 80
    color: "#00ff00"
    // Inset, so the pointer can be off the panel and still in its window.
    Item {
      x: 20
      y: 10
      width: 200
      height: 60
      Shared.PanelSurface { id: surface; theme: cyberpunk }
    }
  }
  Item {
    id: presetHost
    width: 240
    height: 80
    Shared.PanelSurface { theme: preset }
  }
  // A window that stays mapped while the panel in it comes and goes, as the
  // AI cockpit's does.
  Window {
    id: cockpit
    visible: true
    width: 240
    height: 80
    color: "#00ff00"
    Item {
      id: cockpitPanel
      visible: false
      anchors.fill: parent
      Shared.PanelSurface { id: cockpitSurface; theme: cyberpunk }
    }
  }
  // A frame narrower than its trim and glitch would need: both shrink to fit,
  // and the glitch's jolt stays inside it.
  Window {
    id: narrowHost
    visible: false
    width: 110
    height: 40
    color: "#00ff00"
    Shared.PanelSurface { id: narrowSurface; theme: cyberpunk }
  }

  // What the moving parts did since `watch` was last cleared.
  property var seen: ({})
  function watch() { seen = ({ narrowest: 1e9, left: false, right: false, hidden: false }) }
  Connections {
    id: frameWatch
    target: null
    function onDrawnChanged() {
      if (testCase.frame().sweeping) testCase.seen.narrowest = Math.min(testCase.seen.narrowest, testCase.frame().drawn)
    }
    function onJoltChanged() {
      if (testCase.frame().jolt > 0) testCase.seen.left = true
      if (testCase.frame().jolt < 0) testCase.seen.right = true
    }
    function onGlitchShownChanged() { if (!testCase.frame().glitchShown) testCase.seen.hidden = true }
  }

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
  // The folder as `seele-theme register` resolves it.
  function registered(id, slots) {
    var request = new XMLHttpRequest(), document = null
    request.onreadystatechange = function() {
      if (request.readyState === XMLHttpRequest.DONE) document = JSON.parse(request.responseText)
    }
    request.open("GET", Qt.resolvedUrl("../themes/" + id + "/theme.json"))
    request.send()
    tryVerify(function() { return document !== null }, 5000, id + " loads")
    for (var slot of slots) {
      var name = document[slot.name]
      if (slot.type === "path" && typeof name === "string" && name !== "")
        document[slot.name] = Qt.resolvedUrl("../themes/" + id + "/" + name).toString().replace(/^file:\/\//, "")
    }
    return document
  }
  function trim(within) { return findChild(within || surface, "trim") }
  function glitch(within) { return findChild(within || surface, "glitch") }
  function frame(within) { return trim(within) ? trim(within).parent : null }
  // Where the still frame draws the glitch.
  function restingGlitchX(within) { return trim(within).x + frame(within).trimLength + cyberpunk.spaceMedium }
  function atRest(within) {
    var f = frame(within)
    return !f.sweeping && f.jolt === 0 && trim(within).width === f.trimLength
      && f.glitchShown && (!f.glitched || glitch(within).x === restingGlitchX(within))
  }

  function initTestCase() {
    failOnWarning(/.?/)
    cyberpunk.applyTheme(registered("cyberpunk", contractSlots()), contractSlots())
    tryVerify(function() { return frame() !== null }, 2000, "a cut panel draws its frame")
    frameWatch.target = frame()
  }
  function cleanup() {
    host.visible = false
    narrowHost.visible = false
    cockpitPanel.visible = false
    cyberpunk.motionEnabled = true
  }

  function test_the_theme_has_motion_and_sounds_of_its_own() {
    verify(cyberpunk.panelMotion, "Cyberpunk moves its panels")
    verify(cyberpunk.themeSounds, "and carries sounds")
    verify(!preset.panelMotion && !preset.themeSounds, "a preset has neither")
    verify(preset.motionEnabled && !preset.soundEnabled, "motion is let through and sound is not, until asked")
    compare(findChild(presetHost, "trim"), null, "a preset's panel has no frame to move")
  }

  function test_appearing_sweeps_the_trim_then_flickers_the_glitch() {
    watch()
    var started = Date.now()
    host.visible = true
    verify(trim().width < cyberpunk.panelTrim, "the trim starts short of its length: " + trim().width)
    verify(!glitch().visible, "and the glitch waits for it")
    tryVerify(atRest, 2000, "it comes to rest on the still frame")
    var took = Date.now() - started
    verify(took >= cyberpunk.durationPanelTrim, "the sweep takes its time: " + took)
    verify(took < cyberpunk.durationPanelTrim + cyberpunk.durationPanelGlitch + 400, "and is over quickly: " + took)
    verify(seen.narrowest < cyberpunk.panelTrim / 2, "the trim swept out from the cut: " + seen.narrowest)
    verify(seen.left && seen.right && seen.hidden, "the glitch jumped both ways and blinked: " + JSON.stringify(seen))
    compare(frame().jolt, 0)
  }

  function test_the_pointer_coming_on_flickers_the_glitch_again() {
    host.visible = true
    tryVerify(atRest, 2000)
    mouseMove(host.contentItem, 2, 2)
    tryVerify(function() { return !surface.hovered }, 1000, "the pointer starts off the panel")
    watch()
    mouseMove(surface, surface.width / 2, surface.height / 2)
    tryVerify(function() { return surface.hovered }, 1000, "the pointer is over the panel")
    tryVerify(function() { return seen.left && seen.right }, 1000, "the glitch flickers: " + JSON.stringify(seen))
    compare(seen.narrowest, 1e9, "the trim stays put: " + seen.narrowest)
    tryVerify(atRest, 2000, "and rests again")
  }

  // The cost of the motion is bounded by the events that start it: once the
  // panel is at rest nothing runs, so an open panel left alone draws no
  // frame at all.
  property int swaps: 0
  Connections { target: host; function onFrameSwapped() { testCase.swaps += 1 } }
  function test_a_panel_at_rest_costs_no_frame() {
    swaps = 0
    host.visible = true
    tryVerify(atRest, 2000)
    verify(swaps > 2, "the sweep itself draws frames: " + swaps)
    var arrival = findChild(frame(), "arrival"), flicker = findChild(frame(), "flicker")
    verify(arrival !== null && flicker !== null)
    verify(!arrival.running && !flicker.running, "no animation runs at rest")
    wait(100)
    swaps = 0
    wait(1000)
    compare(swaps, 0, "an open panel at rest draws no frame")
  }

  function test_hiding_the_panel_puts_everything_at_rest() {
    host.visible = true
    host.visible = false
    verify(atRest(), "a closed panel is at rest, ready for the next sweep")
  }

  // A panel shown and hidden inside a window that stays mapped sweeps each
  // time it is shown and rests each time it is hidden, as one in a window of
  // its own does.
  function test_a_panel_inside_a_mapped_window_moves_when_it_is_shown() {
    tryVerify(function() { return frame(cockpitSurface) !== null }, 2000)
    var f = frame(cockpitSurface), arrival = findChild(f, "arrival"), flicker = findChild(f, "flicker")
    verify(cockpit.visible && !f.shown, "the window is mapped and the panel hidden")
    verify(!arrival.running && !flicker.running, "nothing moves while it is hidden")
    cockpitPanel.visible = true
    verify(arrival.running && trim(cockpitSurface).width < f.trimLength, "showing it sweeps the trim")
    cockpitPanel.visible = false
    verify(atRest(cockpitSurface) && !arrival.running && !flicker.running, "hiding it mid-sweep rests it")
    cockpitPanel.visible = true
    verify(arrival.running, "and showing it again sweeps again")
    tryVerify(function() { return atRest(cockpitSurface) && !flicker.running }, 2000, "before it rests")
  }

  function test_a_narrow_frame_keeps_its_motion_inside_itself() {
    narrowHost.visible = true
    tryVerify(function() { return frame(narrowSurface) !== null }, 2000)
    var f = frame(narrowSurface), g = glitch(narrowSurface)
    tryVerify(function() { return f.width === narrowHost.width && atRest(narrowSurface) }, 2000, "the window lays the frame out")
    verify(f.glitched && f.flickers && f.glitchLength < cyberpunk.panelGlitch, "the glitch shrinks to fit and still flickers: " + f.glitchLength)
    compare(f.glitchX + f.glitchLength, f.width, "and ends on the frame's edge, where a jolt would push it out")
    var outside = []
    var check = function() { if (g.visible && (g.x < 0 || g.x + g.width > f.width)) outside.push(g.x) }
    g.xChanged.connect(check)
    g.visibleChanged.connect(check)
    narrowHost.visible = false
    narrowHost.visible = true
    tryVerify(function() { return atRest(narrowSurface) && !findChild(f, "flicker").running }, 2000, "it sweeps, flickers and rests")
    g.xChanged.disconnect(check)
    g.visibleChanged.disconnect(check)
    compare(outside, [], "the glitch never left the frame")
    verify(trim(narrowSurface).x + trim(narrowSurface).width <= f.width)
  }

  function test_held_still_the_panel_draws_the_still_frame_at_once() {
    host.visible = true
    verify(trim().width < cyberpunk.panelTrim)
    // Turning motion off mid-sweep lands on the still frame at once.
    cyberpunk.motionEnabled = false
    verify(atRest(), "the sweep stops on the still frame")
    host.visible = false
    watch()
    host.visible = true
    verify(atRest(), "a panel held still appears whole")
    mouseMove(surface, surface.width / 3, surface.height / 3)
    wait(cyberpunk.durationPanelGlitch)
    verify(!seen.left && !seen.right && !seen.hidden && seen.narrowest === 1e9, "and nothing moves: " + JSON.stringify(seen))
  }
}
