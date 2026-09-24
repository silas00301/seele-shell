import QtQuick
import QtTest
import "shared" as Shared
Item {
  width: 480; height: 950
  Shared.TestTheme { id: tokens }
  ResourcesState { id: state; property var calls: []; onRequest: value => calls = calls.concat([value]) }
  ResourcesPanel { id: panel; width: 448; theme: tokens; store: state }
  TestCase {
    name: "Resources"; when: windowShown
    function process(id, name, cpu, rss) {
      return {id: id, pid: Number(id.split(":")[0]), name: name, cpu: cpu, rss: rss, state: "Sleeping", threads: 4, virtualBytes: 800000000}
    }
    function snapshot(rows, selected) {
      return {version:1,type:"snapshot",cadenceSeconds:1,historyCapacity:60,cpu:34.2,cores:8,memory:{total:34359738368,available:21474836480,used:12884901888,swapTotal:8589934592,swapUsed:1073741824},cpuHistory:[null,8,12,40,22,34.2],memoryHistory:[30,31,32,33,34,37.5],rows:rows,total:rows.length,matched:rows.length,selected:selected || null,selectionGone:false}
    }
    function init() {
      state.reset(); state.calls=[]; panel.width=448; panel.maximumHeight=720;
      state.accept(snapshot([process("41:1","compiler",148.2,1610612736),process("42:2","browser",16.4,4294967296),process("43:3","new process",null,1048576)]));
      panel.forceActiveFocus();wait(40);
    }
    function test_keyboard_selection_and_search() {
      keyClick(Qt.Key_Down);keyClick(Qt.Key_Return);
      compare(state.selected,"42:2");compare(state.calls[0].op,"select");
      keyClick(Qt.Key_F,Qt.ControlModifier);
      var search=findChild(panel,"resourcesSearch");verify(search.activeFocus);
      for (var key of [Qt.Key_B,Qt.Key_R,Qt.Key_O,Qt.Key_W,Qt.Key_S,Qt.Key_E,Qt.Key_R]) keyClick(key);compare(state.query,"browser");compare(state.calls[state.calls.length-1].op,"query");
    }
    function test_sort_buttons_and_unknown_readings() {
      var memory=findChild(panel,"resourcesMemorySort");mouseClick(memory);
      compare(state.sort,"memory");compare(state.calls[state.calls.length-1].value,"memory");
      mouseClick(findChild(panel,"resourcesCpuSort"));compare(state.sort,"cpu");
      compare(panel.percent(null),"—");compare(panel.percent(undefined),"—");compare(panel.percent(148.2),"148.2%");
    }
    function test_selection_identity_survives_reordering_and_disappearance() {
      state.select("42:2");
      var browser=process("42:2","browser",33,4294967296);
      state.accept(snapshot([browser,process("41:1","compiler",12,1610612736)],browser));
      compare(state.selected,"42:2");compare(panel.selected.id,"42:2");
      state.accept(snapshot([process("42:99","replacement",null,1000)]));
      compare(state.selected,"42:2");compare(panel.selected,null);
      verify(panel.implicitHeight<=720);
    }
    function test_render_and_height_bound() {
      var wide=grabImage(panel);compare(wide.width,448);verify(wide.height>300);
      panel.width=320;panel.maximumHeight=420;
      state.error="The resource inspector stopped. Reconnecting…";
      wait(40);verify(panel.implicitHeight<=420);
      var narrow=grabImage(panel);compare(narrow.width,320);verify(!wide.equals(narrow));
    }
    function test_chart_missing_and_finite_values() {
      var value=snapshot([]);value.cpuHistory=[null,0,NaN,Infinity,-5,120,50];state.accept(value);
      wait(40);verify(grabImage(panel).height>0);
      state.reset();compare(state.model.count,0);compare(state.snapshot.cpuHistory.length,0);compare(state.selected,"");
    }
  }
}
