import "../shared/ListModels.js" as Models
import QtQuick
import Quickshell
import Quickshell.Io

Scope {
  id: store
  property alias activeModel: activeRows
  property alias snoozedModel: snoozedRows
  property alias historyModel: historyRows
  ListModel { id: activeRows; dynamicRoles: true }
  ListModel { id: snoozedRows; dynamicRoles: true }
  ListModel { id: historyRows; dynamicRoles: true }
  property var snapshot: ({active:[],snoozed:[],history:[],count:0,urgency:"",checkErrors:{}})
  property var expandedIds: ({})
  property string error: "Waiting for maintenance service"
  property string pendingId: ""
  property string actionErrorId: ""
  property string actionError: ""
  property var confirmation: null
  readonly property int count: snapshot.count || 0
  readonly property string urgency: snapshot.urgency || ""
  function reconcile(model, values) {
    Models.reconcile(model, values, "modelData", function(item) { return item.id })
  }
  function row(id) {
    return snapshot.active.concat(snapshot.snoozed, snapshot.history).find(function(value){return value.id===id})
  }
  function unavailable() {
    error="Maintenance service unavailable · findings cannot be checked"
    snapshot={active:[],snoozed:[],history:[],count:0,urgency:"",checkErrors:{}}
    reconcile(activeRows,[]); reconcile(snoozedRows,[]); reconcile(historyRows,[])
    confirmation=null
  }
  function accept(value) {
    if(!value || value.ok !== true || !Array.isArray(value.active) || !Array.isArray(value.snoozed) || !Array.isArray(value.history)) {
      unavailable()
      return false
    }
    snapshot=value
    reconcile(activeRows,value.active)
    reconcile(snoozedRows,value.snoozed)
    reconcile(historyRows,value.history)
    if(confirmation) {
      var current=row(confirmation.id)
      if(!current || current.resolved || current.revision!==confirmation.revision
          || !current.actions.some(function(a){return a.id===confirmation.action})
          || (confirmation.proposed && (current.analysisStale || !current.analysis
              || current.analysis.actions.indexOf(confirmation.action)<0))) confirmation=null
    }
    if(actionErrorId) {
      var target=row(actionErrorId)
      if(!target || target.revision!==command.targetRevision) { actionErrorId=""; actionError="" }
    }
    error=""
    return true
  }
  function refresh() {
    if(!poll.running) { poll.received=false; poll.stdinEnabled=true; poll.running=true }
  }
  function request(item, operation, extra) {
    if(!item || pendingId || command.running || item.busy || item.resolved || error) return false
    var current=row(item.id)
    if(!current || current.busy || current.resolved || current.revision!==item.revision) return false
    var message={op:operation,id:current.id,revision:current.revision}
    extra=extra || {}
    if(operation==="action") {
      var action=current.actions.find(function(a){return a.id===extra.action})
      if(!action || (action.disruptive && extra.confirmed!==true)) return false
      message.action=action.id
      message.confirmed=extra.confirmed===true
    } else if(operation==="snooze") {
      if(!Number.isInteger(extra.seconds) || extra.seconds<60 || extra.seconds>2592000) return false
      message.seconds=extra.seconds
    } else if(operation==="done") {
      if(current.lifecycle!=="notice") return false
    } else if(operation==="analyze") {
      if(!current.canAnalyze) return false
    } else if(operation!=="unsnooze") return false
    pendingId=current.id
    actionErrorId=""; actionError=""
    command.targetId=current.id; command.targetRevision=current.revision
    command.payload=JSON.stringify(message)+"\n"
    command.received=false; command.stdinEnabled=true; command.running=true
    return true
  }
  function repair(item, action, proposed) {
    if(!item || !action || error || pendingId) return false
    var current=row(item.id)
    if(!current || current.busy || current.resolved || current.revision!==item.revision) return false
    var registered=current.actions.find(function(a){return a.id===action.id})
    if(!registered) return false
    if(proposed && (current.analysisStale || !current.analysis || current.analysis.actions.indexOf(registered.id)<0)) return false
    if(proposed || registered.disruptive) {
      confirmation={id:current.id,revision:current.revision,action:registered.id,label:registered.label,proposed:!!proposed}
      return true
    }
    return request(current,"action",{action:registered.id})
  }
  function confirm() {
    if(!confirmation || error) return false
    var reviewed=confirmation
    confirmation=null
    var current=row(reviewed.id)
    if(!current || current.revision!==reviewed.revision) return false
    if(reviewed.proposed && (current.analysisStale || !current.analysis || current.analysis.actions.indexOf(reviewed.action)<0)) return false
    return request(current,"action",{action:reviewed.action,confirmed:true})
  }
  function acceptAction(reply) {
    var current=row(command.targetId)
    if((!reply || reply.ok!==true) && current && current.revision===command.targetRevision) {
      actionErrorId=command.targetId
      actionError="Action failed. Refresh and retry."
    }
  }
  Timer { interval:1000; running:true; repeat:true; triggeredOnStart:true; onTriggered:store.refresh() }
  Timer { interval:10000; running:poll.running; onTriggered:{poll.running=false;store.unavailable()} }
  Timer { interval:45000; running:command.running; onTriggered:{command.running=false;store.acceptAction({ok:false})} }
  Process {
    id:poll
    property bool received:false
    command:["seele-maintenance","request"]
    stdinEnabled:true
    onStarted:{write('{"op":"list"}\n'); stdinEnabled=false}
    onRunningChanged:if(!running)stdinEnabled=true
    stdout:SplitParser { onRead:data=>{poll.received=true;try{store.accept(JSON.parse(data))}catch(_){store.unavailable()}} }
    onExited:function(code){if(code!==0 || !received)store.unavailable()}
  }
  Process {
    id:command
    property bool received:false
    property string targetId:""
    property int targetRevision:0
    property string payload:""
    command:["seele-maintenance","request"]
    stdinEnabled:true
    onStarted:{write(payload);stdinEnabled=false}
    stdout:SplitParser { onRead:data=>{command.received=true;try{store.acceptAction(JSON.parse(data))}catch(_){store.acceptAction({ok:false})}} }
    onExited:function(code){if(code!==0 || !received)store.acceptAction({ok:false});store.pendingId="";stdinEnabled=true;store.refresh()}
  }
}
