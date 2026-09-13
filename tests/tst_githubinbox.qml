import QtQuick
import QtTest
import "shared" as Shared
Item {
  width: 480; height: 900
  Shared.TestTheme { id: theme }
  Item {
    id: store
    property alias model: rows
    property alias detailModel: blocks
    property string connectionError: ""
    property var calls: []
    property var snapshot: ({ count: 2, complete: true, refreshing: false, selected: "", detail: null, error: "", notice: "" })
    function send(op, id) { calls = calls.concat([{op:op,id:id || ""}]) }
    ListModel { id: rows; dynamicRoles: true }
    ListModel { id: blocks; dynamicRoles: true }
  }
  GitHubInboxPanel { id: panel; width: 440; theme: theme; store: store }
  TestCase {
    name: "GitHubInbox"; when: windowShown
    function init() {
      store.calls=[];store.connectionError="";
      store.snapshot={count:2,complete:true,refreshing:false,selected:"",detail:null,error:"",notice:""};
      rows.clear();blocks.clear();panel.width=440;
      rows.append({entry:{id:"1",title:"An urgent thread",repository:"team/project",kind:"Issue",reason:"mention",state:"ready",triage:{priority:"Immediate Action required",summary:"A useful summary"}}});
      rows.append({entry:{id:"2",title:"A pending notification",repository:"team/project",kind:"FutureKind",reason:"subscribed",state:"pending",triage:null}});
      panel.forceActiveFocus();wait(20);
    }
    function test_keyboard_select_opens_internal_detail() {
      keyClick(Qt.Key_Down);keyClick(Qt.Key_Return);
      compare(store.calls.length,1);compare(store.calls[0].op,"select");compare(store.calls[0].id,"2");
    }
    function test_detail_actions_and_escape() {
      store.snapshot={count:2,complete:true,selected:"1",detail:{thread:{url:"https://github.com/team/project/issues/1"},state:"ready",pendingDone:false,detail:{url:"https://github.com/team/project/issues/1"}},error:"",notice:""};
      blocks.append({block:{key:"summary",label:"Immediate Action required",body:"Full triage summary",meta:""}});
      blocks.append({block:{key:"body",label:"Thread",body:"# Untouched Markdown\n\n<script> is plain text.",meta:"author · open"}});
      panel.forceActiveFocus();keyClick(Qt.Key_O);keyClick(Qt.Key_D);keyClick(Qt.Key_Escape);
      compare(store.calls[0].op,"open");compare(store.calls[1].op,"done");compare(store.calls[2].op,"back");
      compare(store.calls.length,3);
    }
    function test_render_loading_failure_and_narrow_width() {
      verify(panel.implicitHeight>100);verify(panel.implicitHeight<850);
      var normal=grabImage(panel);verify(normal.width>400);
      panel.width=320;store.connectionError="GitHub rate limit reached. Retry in five minutes.";wait(20);
      verify(panel.implicitHeight>100);verify(panel.implicitHeight<850);
      var narrow=grabImage(panel);compare(narrow.width,320);
      verify(!normal.equals(narrow));
    }
    function test_enter_folds_the_open_thread() {
      store.snapshot={count:2,complete:true,refreshing:false,selected:"1",detail:{thread:{url:"https://github.com/team/project/issues/1"},state:"ready",pendingDone:false,triage:{priority:"Immediate Action required",summary:"A useful summary",reason:"Mentioned",attention:"Reply",nextAction:"Answer the question",changes:"New comment"},previous:null,detail:{url:""}},error:"",notice:""};
      blocks.append({block:{key:"Thread:0",label:"Thread",body:"Body",meta:"author · open"}});
      panel.forceActiveFocus();wait(theme.durationNormal+80);
      verify(panel.implicitHeight>200,"the open thread grows its row in place");
      keyClick(Qt.Key_Return);
      compare(store.calls.length,1);compare(store.calls[0].op,"back");
    }
  }
}
