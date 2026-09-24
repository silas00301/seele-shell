import QtQuick
import QtTest
import "shared" as Shared
Rectangle {
  width: 480; height: 1000
  color: theme.mantle
  Shared.TestTheme { id: theme }
  Item {
    id: store
    property alias model: rows
    property var groups: [{id:"live"}]
    property var targets: []
    property var selection: []
    property string expanded: ""
    property string query: ""
    property string direction: "all"
    property string status: "all"
    property var history: ({active:1,matched:0,total:2})
    property bool filtering: query.trim() !== "" || direction !== "all" || status !== "all"
    property string error: ""
    property string actionError: ""
    property bool busy: false
    property var calls: []
    signal revealRequested()
    function failure(code) { return code }
    function resetFilters() { query="";direction="all";status="all" }
    function enqueue(value) { calls=calls.concat([value]) }
    ListModel { id: rows }
  }
  TransfersPanel { id: panel; width: 440; theme: theme; store: store }
  TestCase {
    name: "TransfersHistory"; when: windowShown
    property string artifactDirectory: ""
    function init() {
      store.resetFilters();store.calls=[];rows.clear();panel.width=440;
      rows.append({entry:{id:"live",state:"sending",direction:"outgoing",device:"Phone",size:100,bytes:25,error:"",files:[{name:"Photo.jpg",state:"sending",bytes:25,size:100,error:""}]}});
      rows.append({entry:{id:"done",state:"completed",direction:"incoming",device:"Tablet",size:100,bytes:100,error:"",files:[{name:"Budget.pdf",state:"completed",bytes:100,size:100,error:""}]}});
      store.history={active:1,matched:1,total:2};
      panel.forceActiveFocus();wait(20);
    }
    function test_keyboard_search_and_reset() {
      keyClick(Qt.Key_F,Qt.ControlModifier);
      var search=findChild(panel,"transferSearch");verify(search.activeFocus);
      keyClick(Qt.Key_A);keyClick(Qt.Key_B);compare(store.query,"ab");
      keyClick(Qt.Key_Escape);compare(store.query,"");verify(search.activeFocus);
    }
    function test_keyboard_filter_choices() {
      var incoming=findChild(panel,"direction_incoming");incoming.forceActiveFocus();
      keyClick(Qt.Key_Space);compare(store.direction,"incoming");
      var failed=findChild(panel,"status_failed");failed.forceActiveFocus();
      keyClick(Qt.Key_Return);compare(store.status,"failed");
      var reset=findChild(panel,"resetFilters");reset.forceActiveFocus();keyClick(Qt.Key_Return);
      compare(store.status,"all");compare(store.direction,"all");
      verify(findChild(panel,"transferSearch").activeFocus);
    }
    function test_filters_and_progress_render_at_narrow_width() {
      if (artifactDirectory) grabImage(panel).save(artifactDirectory + "/transfers-history.png");
      store.query="missing";rows.remove(1);store.history={active:1,matched:0,total:2};wait(20);
      if (artifactDirectory) grabImage(panel).save(artifactDirectory + "/transfers-no-results.png");
      verify(panel.implicitHeight>300);verify(panel.implicitHeight<1000);
      var full=grabImage(panel);verify(full.width>400);
      panel.width=320;wait(20);var narrow=grabImage(panel);compare(narrow.width,320);
      verify(!full.equals(narrow));compare(rows.count,1);
    }
  }
}
