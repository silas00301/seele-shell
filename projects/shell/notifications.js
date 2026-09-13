.import "../shared/Native.js" as Bridge
// QObject properties are projected once; policy runs in the shared Rust core.
function actions(entry) { return Bridge.call("notifications.actions",[entry,entry && entry.action_order || Object.keys(entry && entry.actions || {})]) }
function verificationCode(entry) { return Bridge.call("notifications.verificationCode",[entry]) }
function groupKey(entry) { return Bridge.call("notifications.groupKey",[entry]) }
function stackedRows(entries, expanded) { return Bridge.call("notifications.stackedRows",[entries,expanded]) }
function localImage(source) { return Bridge.call("notifications.localImage",[source]) }
function imageRoles(entry) { return Bridge.call("notifications.imageRoles",[entry]) }
function bodyMarkup(body) { return Bridge.call("notifications.bodyMarkup",[body]) }
function fromNative(n, now) {
  var hints=n.hints || {}
  var entry=Bridge.call("notifications.fromNative",[{
    id:n.id,appName:n.appName,appIcon:n.appIcon,desktopEntry:n.desktopEntry,
    summary:n.summary,body:n.body,hasActionIcons:n.hasActionIcons,image:String(n.image || ""),
    urgency:Bridge.number(Number(n.urgency)),resident:n.resident,transient:n.transient,
    expireTimeout:Bridge.number(Number(n.expireTimeout)),hints:{value:Bridge.number(Number(hints.value)),
      "x-dunst-stack-tag":hints["x-dunst-stack-tag"],"x-canonical-private-synchronous":hints["x-canonical-private-synchronous"]}
  },now])
  // Preserve sender action insertion order and QObject identity outside JSON.
  // Qt serializes ordinary objects into QVariantMap. Null-prototype objects
  // remain opaque QJSValue wrappers, so define sender keys as own data instead.
  var advertised={}
  for (var i=0;i<n.actions.length;i++) Object.defineProperty(advertised,n.actions[i].identifier,
    {value:n.actions[i].text,enumerable:true,writable:true,configurable:true})
  entry.actions=advertised
  entry.action_order=Object.keys(advertised)
  return entry
}
function permanent(entry) { return Bridge.call("notifications.permanent",[entry]) }
function popupDuration(entry) { return Bridge.call("notifications.popupDuration",[entry]) }

// Qt owns live QObject handles and callback invocation. Rust owns all serializable
// state, timing, quiet periods, replacement, pinning, history and grouping policy.
function createStore(publish, arrived, now) {
  var policy=Bridge.notificationState(now), snapshot={dnd:false,dndUntil:0,dndMinutes:0}
  var objects=Object.create(null), state={}
  Object.defineProperty(state,"dnd",{get:function(){return snapshot.dnd}})
  Object.defineProperty(state,"dndUntil",{get:function(){return snapshot.dndUntil}})
  Object.defineProperty(state,"dndMinutes",{get:function(){return snapshot.dndMinutes}})
  state.find=function(id) { return objects[String(id)] || null }
  function apply(operation,args,notification) {
    var response=policy.call(operation,args)
    snapshot={dnd:response.dnd,dndUntil:response.dndUntil,dndMinutes:response.dndMinutes}
    if (notification && !objects[String(notification.id)]) objects[String(notification.id)]=notification
    // Install state before invoking Qt: dismiss/expire may synchronously close.
    for (var i=0;i<response.effects.length;i++) {
      var effect=response.effects[i], object=objects[String(effect.id)]
      if (effect.operation==="publish") state.publish()
      else if (effect.operation==="arrived") arrived(effect.entry,effect.fresh)
      else if (object && effect.operation==="dismiss") object.dismiss()
      else if (object && effect.operation==="expire") object.expire()
    }
    return response.result
  }
  state.save=function() { return policy.call("save",[]) }
  state.view=function() { return policy.call("view",[]) }
  state.publish=function() { publish(state.view(),snapshot.dnd) }
  state.restore=function(saved) { return apply("restore",[saved]) }
  state.receive=function(n,timestamp) {
    return apply("receive",[fromNative(n,timestamp),Bridge.number(timestamp),n.lastGeneration],n)
  }
  state.closed=function(id,reason) { delete objects[String(id)]; return apply("closed",[id,reason]) }
  state.advance=function(timestamp) { return apply("advance",[Bridge.number(timestamp)]) }
  state.pause=function(paused,timestamp) { return apply("pause",[paused,Bridge.number(timestamp)]) }
  state.retire=function(id) { return apply("retire",[id]) }
  state.dismiss=function(id) { return apply("dismiss",[id]) }
  state.pin=function(id) { return apply("pin",[id]) }
  state.setDnd=function(enabled) { return apply("setDnd",[enabled]) }
  state.snooze=function(minutes,timestamp) { return apply("snooze",[Bridge.number(minutes),Bridge.number(timestamp)]) }
  state.clear=function(history) { return apply("clear",[history]) }
  state.group=function(key,popup) { return apply("group",[key,popup]) }
  state.invoke=function(id,key) {
    var object=objects[String(id)]
    if (!object) return false
    for (var i=0;i<object.actions.length;i++) {
      if (object.actions[i].identifier!==key) continue
      object.actions[i].invoke()
      return true
    }
    return false
  }
  return state
}
