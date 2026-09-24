import QtQuick
import QtTest
import "production" as Shell
import "production/shared" as Shared

TestCase {
  id: testCase
  name: "PortsBindings"
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
    property string bindingScope: "all"
    readonly property bool filtered: query !== "" || bindingScope !== "all"
    property string expanded: ""
    property string error: ""
    property string actionError: ""
    property string copied: ""
    property bool busy: false
    property bool limited: false
    property int total: 3
    property var plan: null
    property var outcome: null
    function scheme(id) { return "https" }
    function url(entry) { return "" }
    function summary(entry) { return "Unknown owner" }
    function expand(id) { expanded = expanded === id ? "" : id }
    function refresh() {}
    function resetFilters() { query = ""; bindingScope = "all" }
  }
  Rectangle { anchors.fill: parent; color: theme.base }
  Shell.PortsPanel {
    id: panel
    theme: theme
    store: fixture
    x: theme.panelMargin
    y: theme.panelMargin
    width: parent.width - theme.panelMargin * 2
  }
  function child(name) {
    var item = findChild(panel, name)
    verify(item !== null, "Missing " + name)
    return item
  }
  function entry(id) {
    var network = id === "row1"
    var ipv6 = id === "row2"
    return { id: id, binding: network ? "0.0.0.0:8080" : ipv6 ? "[::1]:9000" : "127.0.0.1:3000",
      scope: network ? "wildcard" : "loopback", scopeLabel: network ? "All interfaces" : "This machine only",
      owners: [], target: {kind: "none", reason: "Owner is not readable."}, token: id }
  }
  function init() {
    failOnWarning(/.?/)
    fixture.query = ""
    fixture.bindingScope = "all"
    fixture.expanded = ""
    rows.clear()
    for (var i = 0; i < 3; i++) rows.append({entry: entry("row" + i)})
    wait(20)
  }
  function test_scope_pointer_keyboard_and_reset() {
    fixture.query = "https://localhost:3000"
    var network = child("scopeNetwork")
    mouseClick(network, network.width / 2, network.height / 2)
    compare(fixture.bindingScope, "network")
    compare(fixture.query, "https://localhost:3000")
    verify(network.selected)
    var loopback = child("scopeLoopback")
    loopback.forceActiveFocus()
    keyClick(Qt.Key_Return)
    compare(fixture.bindingScope, "loopback")
    verify(loopback.selected)
    child("scopeAll").forceActiveFocus()
    keyClick(Qt.Key_Space)
    compare(fixture.bindingScope, "all")
    rows.clear()
    wait(20)
    var reset = child("resetFilters")
    verify(reset.visible)
    mouseClick(reset, reset.width / 2, reset.height / 2)
    compare(fixture.query, "")
    compare(fixture.bindingScope, "all")
  }
  function test_list_selection_after_filter_and_keyboard() {
    var list = child("listeners")
    list.currentIndex = 2
    rows.clear()
    tryCompare(list, "currentIndex", -1)
    rows.append({entry: entry("remaining")})
    tryCompare(list, "currentIndex", 0)
    panel.handleKey({key: Qt.Key_Return, modifiers: Qt.NoModifier, accepted: false})
    compare(fixture.expanded, "remaining")
    rows.append({entry: entry("next")})
    panel.handleKey({key: Qt.Key_J, modifiers: Qt.NoModifier, accepted: false})
    compare(list.currentIndex, 1)
    panel.handleKey({key: Qt.Key_K, modifiers: Qt.NoModifier, accepted: false})
    compare(list.currentIndex, 0)
  }
  function test_controls_fit_and_render() {
    var all = child("scopeAll")
    var network = child("scopeNetwork")
    verify(all.width > 0 && all.height > 0)
    verify(network.mapToItem(panel, network.width, 0).x <= panel.width)
    verify(panel.implicitHeight > child("listeners").height)
    var capture = grabImage(panel)
    verify(capture.width > 0 && capture.height > 0)
    if (screenshotPath !== "") capture.save(screenshotPath)
  }
}
