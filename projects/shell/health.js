// Version 1: current sanitized metadata only. Provider owners must never pass raw responses.
var states = ["healthy", "degraded", "disconnected", "setup-required"]
var actions = ["retry", "restart", "reconnect", "settings", "diagnostics"]
function identifier(value) { return typeof value === "string" && /^[a-z][a-z0-9-]{0,63}$/.test(value) }
function clean(value, limit) {
  if (typeof value !== "string" || value.length > limit || /[\x00-\x1f]/.test(value)) throw Error("Invalid metadata")
  // Defense in depth; publishers remain responsible for summarizing private data.
  if (/(bearer\s|token[=:]|password[=:]|secret[=:]|gh[pousr]_|sk-[A-Za-z0-9])/i.test(value)) throw Error("Private metadata")
  return value
}
function registration(value) {
  if (!value || !identifier(value.id)) throw Error("Invalid provider")
  var allowed = value.actions || ["settings", "diagnostics"]
  if (!Array.isArray(allowed) || allowed.some(function(a) { return actions.indexOf(a) < 0 })) throw Error("Invalid action")
  if (value.service && !/^[a-zA-Z0-9][a-zA-Z0-9@_.-]{0,100}\.service$/.test(value.service)) throw Error("Invalid managed service")
  if (value.setup && !identifier(value.setup)) throw Error("Invalid setup destination")
  return {id:value.id, name:clean(value.name,80), deadline:Math.max(5000,Math.min(3600000,Number(value.deadline)||90000)), actions:allowed.slice(), service:value.service||"", setup:value.setup||"", disruptive:(value.disruptive||[]).filter(function(a){return allowed.indexOf(a)>=0})}
}
function publication(reg, value, now) {
  if (!value || states.indexOf(value.state)<0) throw Error("Invalid state")
  var offered = value.actions || []
  if (!Array.isArray(offered) || offered.some(function(a){return reg.actions.indexOf(a)<0})) throw Error("Unsupported action")
  return {id:reg.id, name:reg.name, state:value.state, summary:clean(value.summary,240), detail:clean(value.detail||"",1200), lastSuccess:Math.max(0,Math.min(now,Number(value.lastSuccess)||0)), actions:offered.slice(), updated:now}
}
function rows(registrations, values, now) {
  return Object.keys(registrations).map(function(id) {
    var reg=registrations[id], value=values[id] || {id:id,name:reg.name,state:"stale",summary:"Waiting for provider status",lastSuccess:0,detail:"",actions:reg.actions.filter(function(a){return a==="settings" || a==="restart"}),updated:0}
    var row=Object.assign({},value)
    if (now-value.updated>reg.deadline) { row.state="stale"; row.summary="Provider stopped updating"; row.actions=reg.actions.filter(function(a){return a==="settings" || a==="restart" || a==="diagnostics"}) }
    return row
  }).sort(function(a,b){return Number(a.state==="healthy")-Number(b.state==="healthy") || a.name.localeCompare(b.name)})
}
