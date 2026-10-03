// Exercise the production Control Center grid, viewport and keyboard in Qt at
// desktop, short and compact output sizes.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {spawnSync} = require('node:child_process');
const source = path.resolve(process.argv[2]);
const shell = fs.readFileSync(path.join(source, 'shell.qml'), 'utf8');

// The text from `start` through the brace that closes the block it opens.
function braced(start) {
  const first = shell.indexOf(start);
  assert.ok(first >= 0, `production block ${start}`);
  let depth = 0;
  for (let index = shell.indexOf('{', first); index < shell.length; ++index) {
    if (shell[index] === '{') depth += 1;
    else if (shell[index] === '}' && --depth === 0) return shell.slice(first, index + 1);
  }
  throw Error(`unbalanced production block ${start}`);
}
function between(start, end) {
  const first = shell.indexOf(start), last = shell.indexOf(end, first);
  assert.ok(first >= 0 && last > first, `production block ${start}`);
  return shell.slice(first, last);
}
const components = ['ModuleDragArea', 'ConnectivityRow', 'ControlLevel', 'ControlTile', 'UtilityTile', 'ControlCenterGrid']
  .map(name => braced(`  component ${name}: `)).join('\n\n');
const geometry = between('      implicitWidth: Math.min(root.mediaPanelWidth', '      exclusionMode: ExclusionMode.Ignore');
const viewport = braced('          Shared.SeeleFlickable {\n            id: controlCenterViewport');

// Nothing in the grid may be placed at a counted offset or sized by a counted
// height: that is how the panel once grew a row of tiles nobody could reach.
const grid = braced('  component ControlCenterGrid: ');
assert.doesNotMatch(grid, /\by: controlGrid\.|\bheight: devicesY|smallTileHeight \*/, 'the grid lays itself out');

const work = fs.mkdtempSync(path.join(os.tmpdir(), 'seele-control-center-'));
try {
  const shared = fs.existsSync(path.join(source, 'shared')) ? path.join(source, 'shared') : path.resolve(source, '../shared');
  fs.cpSync(shared, path.join(work, 'shared'), {recursive: true});
  const themePath = path.join(work, 'shared/Theme.qml');
  const theme = fs.readFileSync(themePath, 'utf8').split('  FileView {')[0]
    .replace(/import Quickshell.*\n/g, '').replace('ShellRoot {', 'Item {')
    .replace(/Quickshell.env\("SEELE_SHELL_WALLPAPER"\) \|\| /, '')
    .replace('Qt.resolvedUrl("grain.png")', '""');
  fs.writeFileSync(themePath, theme + '}\n');
  fs.writeFileSync(path.join(work, 'tst_controlcenter.qml'), `
import QtQuick
import QtQuick.Controls
import QtTest
import "shared" as Shared
TestCase {
  id: testCase
  name: "ControlCenterLayout"
  width: 500; height: 1100; visible: true; when: windowShown
  Shared.Theme {
    id: root
    property var systemData: ({
      connection: "Seele", connectionType: "802-11-wireless", wifiAvailable: true, wifiEnabled: true,
      bluetoothAvailable: true, bluetoothPowered: true, bluetoothConnected: 1,
      volume: 60, muted: false, microphoneVolume: 80, microphoneMuted: false,
      cameraActive: false, headphones: {connected: false}
    })
    property string dragModule: ""
    property int volumeDrag: -1
    property int microphoneDrag: -1
    property int audioTrackMaximum: 100
    property int healthMaintenanceCount: 0
    readonly property int healthAttentionCount: integrationHealth.attentionCount + healthMaintenanceCount
    property color healthTint: red
    property string bluetoothAction: ""
    property string opened: ""
    property var calls: []
    function record(call) { calls = calls.concat([call]) }
    function nowPlayingPlayer() { return null }
    function toggleMedia() { opened = "media" }
    function toggleControl(name, screen) { opened = name + ":" + screen }
    function controlBusy() { return false }
    function runControl(action, value) { record(action + " " + value); return true }
    function patchSystemData(patch) {}
    function toggleBluetoothPower() { record("bluetooth") }
    function privateNetworkDetail() { return "Off" }
    function privateNetworkActive() { return false }
    function privateNetworkTarget() { return "tailscale" }
    function privateNetworkBusy() { return false }
    function togglePrivateNetwork() { record("vpn") }
    function cameraDetail() { return "Not in use" }
    function headphonesLabel() { return "Nothing Headphone (1)" }
    function headphonesDetail() { return "L 80% · R 80%" }
    function headphonesIconKind() { return "over-ear" }
    function audioFillRatio(value) { return Math.max(0, Math.min(1, value / 100)) }
    function adjustAudioFromWheel(wheel, microphone) {}
    function adjustAudio(steps, microphone) { record((microphone ? "microphone " : "volume ") + steps) }
    function toggleAudioMute(microphone) { record((microphone ? "microphone" : "volume") + " mute") }
    function beginModuleDrag() {}
    function updateModuleDrag() {}
    function endModuleDrag() {}
    function cancelModuleDrag() {}
    function closeOverlays() { opened = "closed" }
  }
  QtObject { id: integrationHealth; property int attentionCount: 0 }
  QtObject { id: transfersStore; property bool attention: false; property string barText: "" }
  QtObject { id: driftStore; property int drifted: 0 }
  QtObject {
    id: prFocusStore
    property bool configured: false
    property bool active: false
    property string label: "fixture PR"
    property var snapshot: ({title: "", checks: "", state: "", comment: "", commentAuthor: "", message: ""})
    function enter() { active = true }
    function exit() { active = false }
  }
  QtObject { id: githubFixture; function checksLabel(value) { return value } }
  QtObject { id: portsStore; property int total: 0 }
  QtObject { id: bluetoothProcess; property bool running: false }
  QtObject {
    id: themeStore
    property string currentName: "Catppuccin Mocha"
    property string appearanceGlyph: "D"
    function cycleAppearance() { root.record("appearance") }
  }
  Timer { id: volumeDragTimer }
  Timer { id: microphoneDragTimer }
  component CardEdge: Shared.CardEdge { theme: root }
  component HoverWash: Shared.HoverWash { theme: root }
  component FocusRing: Shared.FocusRing { theme: root }
  component PanelHeader: Shared.PanelHeader { theme: root }
  component PanelSurface: Shared.PanelSurface { theme: root }
  component SlimScrollBar: Shared.SlimScrollBar { theme: root }
  component RefreshGlyph: Shared.RefreshGlyph { theme: root }
  component CenteredGlyph: Shared.CenteredGlyph {}
  component HoverTip: QtObject { property var mouse: null; property string text: ""; property bool inOverlay: false }
  component HeadphonesIcon: Item { property string kind: ""; property color tint; width: 16; height: 16 }
  component MediaBody: Item { property var player: null }

  ${components}

  Item {
    id: controlCenterWindow
    property var modelData: ({width: 1920, height: 1080, name: "fixture"})
    property int barReach: root.barHeight + root.panelGap
    property bool dragging: false
    ${geometry}
    width: implicitWidth; height: implicitHeight
    Item {
      anchors.fill: parent
      anchors.topMargin: controlCenterWindow.barReach
      PanelSurface {
      ${viewport.replace(/GitHub\.checksLabel/g, "githubFixture.checksLabel")}
      }
    }
  }

  function find(item, test) {
    if (test(item)) return item
    var children = item.children || []
    for (var index = 0; index < children.length; ++index) {
      var found = find(children[index], test); if (found) return found
    }
    return null
  }
  function tile(label) { return find(controlCenterContent, item => item.label === label && item.visible) }
  function level(microphone) { return find(controlCenterContent, item => item.microphone === microphone && item.fillRatio !== undefined) }
  function box(item) { var point = item.mapToItem(controlCenterContent, 0, 0); return {x: point.x, y: point.y, w: item.width, h: item.height} }
  function overlaps(a, b) { return !(a.x + a.w <= b.x + 0.1 || b.x + b.w <= a.x + 0.1 || a.y + a.h <= b.y + 0.1 || b.y + b.h <= a.y + 0.1) }
  readonly property var utilities: ["Health", "Fix me", "Transfers", "Resources", "Traffic", "Ports", "Calculator", "Colour Lab", "Text"]
  readonly property var panels: ["system-health", "drift", "transfers", "resources", "network-activity", "ports", "calculator", "color-lab", "text-workbench"]

  function init() {
    prFocusStore.configured = false
    prFocusStore.active = false
    controlCenterWindow.modelData = {width: 1920, height: 1080, name: "fixture"}
    controlCenterWindow.dragging = false
    root.systemData = Object.assign({}, root.systemData, {headphones: {connected: false}})
    integrationHealth.attentionCount = 0
    root.healthMaintenanceCount = 0
    controlCenterViewport.contentY = 0
    root.opened = ""
    root.calls = []
    controlCenterContent.forceActiveFocus()
    wait(20)
  }

  function test_geometry_data() {
    return [{tag: "desktop", width: 1920, height: 1080},
            {tag: "short", width: 1024, height: 560},
            {tag: "compact", width: 320, height: 480}]
  }
  function test_geometry(data) {
    controlCenterWindow.modelData = {width: data.width, height: data.height, name: "fixture"}
    wait(20)
    verify(controlCenterWindow.height <= data.height - root.panelGap, "the panel stays on its output")
    verify(controlCenterWindow.width <= data.width)
    var labels = ["Camera", "Themes"].concat(utilities)
    var tiles = labels.map(label => tile(label))
    for (var i = 0; i < tiles.length; ++i) {
      verify(tiles[i] !== null, labels[i])
      var a = box(tiles[i])
      verify(a.w > 0 && a.h > 0)
      verify(a.x >= 0 && a.x + a.w <= controlCenterContent.width + 0.1, labels[i] + " stays inside the panel")
      for (var j = i + 1; j < tiles.length; ++j)
        verify(!overlaps(a, box(tiles[j])), labels[i] + " overlaps " + labels[j])
    }
    // Every utility sits in one grid of equal cells, four to a row.
    var first = box(tile("Health"))
    compare(box(tile("Traffic")).x, first.x)
    verify(box(tile("Ports")).y > first.y)
    for (var k = 1; k < 4; ++k) compare(box(tile(utilities[k])).y, first.y, utilities[k] + " shares the first row")
    // The last utility is reachable on every output, and clicking it opens its panel.
    var last = tile("Text")
    controlCenterViewport.contentY = Math.max(0, controlCenterViewport.contentHeight - controlCenterViewport.height)
    wait(20)
    var point = last.mapToItem(controlCenterViewport, last.width / 2, last.height / 2)
    verify(point.y > 0 && point.y < controlCenterViewport.height, "last utility stays reachable")
    mouseClick(last, last.width / 2, last.height / 2)
    compare(root.opened, "text-workbench:fixture")
  }

  function test_panel_fits_without_scrolling_on_a_desktop() {
    compare(controlCenterViewport.interactive, false, "a panel that fits does not scroll")
    compare(controlCenterViewport.height, controlCenterViewport.contentHeight)
  }

  function test_headphones_take_their_own_row() {
    var camera = box(tile("Camera")), themes = box(tile("Themes"))
    compare(tile("Nothing Headphone (1)"), null)
    root.systemData = Object.assign({}, root.systemData, {headphones: {connected: true}})
    wait(20)
    var headphones = tile("Nothing Headphone (1)")
    verify(headphones !== null)
    compare(box(tile("Camera")).y, camera.y, "Camera does not move when headphones connect")
    compare(box(tile("Themes")).x, themes.x, "neither does Themes")
    compare(headphones.width, controlCenterContent.width)
    verify(box(headphones).y > camera.y + camera.h - 0.1)
    verify(box(tile("Health")).y > box(headphones).y, "the utilities stay below")
  }

  function test_every_utility_opens_its_panel() {
    for (var index = 0; index < utilities.length; ++index) {
      var item = tile(utilities[index])
      mouseClick(item, item.width / 2, item.height / 2)
      compare(root.opened, panels[index] + ":fixture")
    }
  }

  function test_health_reports_only_when_something_is_wrong() {
    compare(tile("Health").value, "")
    compare(tile("Health").active, false)
    verify(tile("Health").tip.indexOf("all clear") > 0)
    integrationHealth.attentionCount = 1
    root.healthMaintenanceCount = 2
    wait(10)
    compare(tile("Health").value, "3")
    compare(tile("Health").active, true)
    compare(tile("Health").tip, "System Health · 1 integration needs attention · 2 maintenance items")
  }

  function test_transfers_show_the_bar_reading() {
    compare(tile("Transfers").value, "")
    transfersStore.attention = true
    transfersStore.barText = "\\u{f01da} 45%"
    wait(10)
    compare(tile("Transfers").value, "45%")
    transfersStore.barText = "\\u{f01da} !"
    wait(10)
    compare(tile("Transfers").valueColor, root.red)
    transfersStore.attention = false
  }

  function test_keyboard_reaches_every_module() {
    keyClick(Qt.Key_Down)
    var wifi = find(controlCenterContent, item => item.label === "Wi-Fi")
    verify(wifi.activeFocus, "the first arrow steps onto the first module")
    keyClick(Qt.Key_Down)
    verify(find(controlCenterContent, item => item.label === "Bluetooth").activeFocus, "Down moves within the card")
    keyClick(Qt.Key_Space)
    compare(root.calls, ["bluetooth"], "Space throws the radio")
    keyClick(Qt.Key_Return)
    compare(root.opened, "bluetooth:fixture", "Enter opens its panel")
    keyClick(Qt.Key_Right)
    var output = level(false), microphone = level(true)
    verify(output.activeFocus || microphone.activeFocus, "Right crosses to the Audio card")
    output.forceActiveFocus()
    root.calls = []
    keyClick(Qt.Key_Right)
    keyClick(Qt.Key_Left)
    keyClick(Qt.Key_Space)
    compare(root.calls, ["volume 1", "volume -1", "volume mute"], "a level takes its own arrows and Space mutes")
    keyClick(Qt.Key_Down)
    verify(microphone.activeFocus, "Down reaches the microphone")
    keyClick(Qt.Key_Down)
    verify(tile("Themes").activeFocus, "and then the tile under it")
    root.calls = []
    keyClick(Qt.Key_Space)
    compare(root.calls, ["appearance"], "Space is the Themes knob")
    keyClick(Qt.Key_Left)
    verify(tile("Camera").activeFocus, "Left stays on the row rather than dropping to a utility below")
    keyClick(Qt.Key_Down)
    verify(tile("Health").activeFocus, "Down reaches the utilities")
    keyClick(Qt.Key_Right)
    verify(tile("Fix me").activeFocus, "Right moves along the utilities")
    keyClick(Qt.Key_Down)
    verify(tile("Ports").activeFocus, "the utilities are a grid")
    keyClick(Qt.Key_Return)
    compare(root.opened, "ports:fixture")
  }

  function test_tab_walks_the_panel_in_reading_order() {
    keyClick(Qt.Key_Tab)
    verify(find(controlCenterContent, item => item.label === "Wi-Fi").activeFocus)
    for (var index = 0; index < 20 && !tile("Text").activeFocus; ++index) keyClick(Qt.Key_Tab)
    verify(tile("Text").activeFocus, "Tab reaches the last utility")
  }

  function test_pr_focus_keyboard_data() {
    return [{tag: "return", key: Qt.Key_Return},
            {tag: "enter", key: Qt.Key_Enter},
            {tag: "space", key: Qt.Key_Space}]
  }

  function test_pr_focus_keyboard(data) {
    prFocusStore.configured = true
    wait(20)
    keyClick(Qt.Key_Tab)
    verify(prFocusEnter.activeFocus, "Tab reaches the PR focus card first")
    keyClick(data.key)
    tryCompare(prFocusStore, "active", true)
    tryCompare(prFocusExit, "activeFocus", true)
    keyClick(data.key)
    tryCompare(prFocusStore, "active", false)
    tryCompare(prFocusEnter, "activeFocus", true)
    keyClick(Qt.Key_Tab)
    verify(find(controlCenterContent, item => item.label === "Wi-Fi").activeFocus)
    keyClick(Qt.Key_Backtab)
    verify(prFocusEnter.activeFocus, "Backtab returns from the grid to PR focus")
  }

  function test_pr_focus_arrow_navigation() {
    prFocusStore.configured = true
    wait(20)
    keyClick(Qt.Key_Down)
    verify(prFocusEnter.activeFocus, "The first arrow reaches the PR focus card")
    keyClick(Qt.Key_Down)
    verify(find(controlCenterContent, item => item.label === "Wi-Fi").activeFocus)
    keyClick(Qt.Key_Up)
    verify(prFocusEnter.activeFocus, "Up returns from the grid to PR focus")
  }

  function test_keyboard_scrolls_a_short_output() {
    controlCenterWindow.modelData = {width: 1024, height: 480, name: "fixture"}
    wait(20)
    verify(controlCenterViewport.interactive)
    tile("Text").forceActiveFocus(Qt.TabFocusReason)
    wait(20)
    var point = tile("Text").mapToItem(controlCenterViewport, 0, tile("Text").height)
    verify(point.y <= controlCenterViewport.height + 0.1, "the focused tile is scrolled into view")
    tile("Themes").forceActiveFocus(Qt.TabFocusReason)
    wait(20)
    verify(tile("Themes").mapToItem(controlCenterViewport, 0, 0).y >= -0.1, "and back up again")
  }

  function test_drag_and_escape() {
    controlCenterWindow.modelData = {width: 1024, height: 480, name: "fixture"}
    wait(20)
    verify(controlCenterViewport.interactive)
    controlCenterWindow.dragging = true
    verify(!controlCenterViewport.interactive, "scrolling must not steal a module drag")
    controlCenterWindow.dragging = false
    tile("Ports").forceActiveFocus()
    keyClick(Qt.Key_Escape)
    compare(root.opened, "closed", "Escape closes from any module")
  }
}
`);
  const result = spawnSync('qmltestrunner', ['-input', path.join(work, 'tst_controlcenter.qml'), '-import', process.argv[3]], {
    encoding: 'utf8', env: {...process.env, QT_QPA_PLATFORM: 'offscreen', QT_QUICK_BACKEND: 'software'},
  });
  process.stdout.write(result.stdout || ''); process.stderr.write(result.stderr || '');
  if (result.error) throw result.error;
  assert.equal(result.status, 0, 'production Control Center Qt checks');
  console.log('Control Center grid, output-bounded viewport, utilities and keyboard navigation passed');
} finally { fs.rmSync(work, {recursive: true, force: true}); }
