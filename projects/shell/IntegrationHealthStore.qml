import QtQuick
import Quickshell
import Quickshell.Io
import "health.js" as Health

Scope {
  id: store
  property alias model: rowModel
  ListModel { id: rowModel; dynamicRoles: true }
  property var registrations: ({})
  property var values: ({})
  property var errors: ({})
  property var pending: ({})
  property var handlers: ({})
  property double now: Date.now()
  property int serial: 0
  readonly property var rows: Health.rows(registrations, values, now)
  readonly property int attentionCount: rows.filter(function(row){return row.state !== "healthy"}).length
  signal openSettings(string destination)
  signal configured()
  onRowsChanged: reconcile()
  function reconcile() {
    for(var i=0;i<rows.length;i++) {
      var found=-1
      for(var j=i;j<rowModel.count;j++) if(rowModel.get(j).modelData.id===rows[i].id){found=j;break}
      if(found<0) rowModel.insert(i,{modelData:rows[i]})
      else {
        if(found!==i) rowModel.move(found,i,1)
        if(JSON.stringify(rowModel.get(i).modelData)!==JSON.stringify(rows[i])) rowModel.setProperty(i,"modelData",rows[i])
      }
    }
    while(rowModel.count>rows.length)rowModel.remove(rowModel.count-1)
  }

  function configure(entries) {
    var next = {}, retained = {}
    try {
      if (!Array.isArray(entries) || entries.length>64) return false
      entries.forEach(function(value){var reg=Health.registration(value); next[reg.id]=reg; if(values[reg.id]) retained[reg.id]=values[reg.id]})
    } catch (_) { return false }
    registrations=next; values=retained; pending=({}); errors=({}); configured(); return true
  }
  function publish(id, value) {
    if (!registrations[id]) return false
    try {
      var next=Object.assign({},values)
      next[id]=Health.publication(registrations[id],value,Date.now())
      values=next; now=Date.now(); return true
    } catch (_) { return false }
  }
  function complete(id, token, ok) {
    if (!pending[id] || pending[id].token!==token) return
    var p=Object.assign({},pending), e=Object.assign({},errors)
    delete p[id]; if(ok) delete e[id]; else e[id]="Action failed. Try again."
    pending=p; errors=e
  }
  function act(id, action, confirmed) {
    var row=rows.find(function(r){return r.id===id}), reg=registrations[id]
    if (!row || !reg || row.actions.indexOf(action)<0 || pending[id]) return false
    if (reg.disruptive.indexOf(action)>=0 && !confirmed) return false
    if (action==="settings") { if(!reg.setup)return false; openSettings(reg.setup); return true }
    if (action==="diagnostics") return true
    var token=++serial, p=Object.assign({},pending), e=Object.assign({},errors)
    delete e[id]; errors=e; p[id]={token:token,started:Date.now()}; pending=p
    if (action==="restart" && reg.service) {
      var task=restartFactory.createObject(store,{providerId:id,token:token,command:["systemctl","--user","restart",reg.service]})
      if(task) task.running=true; else complete(id,token,false)
    } else if (handlers[id]) handlers[id](action,token)
    else complete(id,token,false)
    return true
  }
  FileView {
    path: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME")+"/.config")+"/seele-shell/health.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: { try { store.configure(JSON.parse(text())) } catch (_) { store.configure([]) } }
    onLoadFailed: store.configure([])
  }
  Timer {
    interval:1000; running:true; repeat:true
    onTriggered: {
      store.now=Date.now()
      Object.keys(store.pending).forEach(function(id){if(store.now-store.pending[id].started>45000)store.complete(id,store.pending[id].token,false)})
    }
  }
  Component {
    id: restartFactory
    Process {
      property string providerId
      property int token
      onExited: function(code) { store.complete(providerId,token,code===0); destroy() }
    }
  }
}
