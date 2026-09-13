.import "../shared/Native.js" as Bridge
// QObject maps and Qt locale collation stay at the host binding boundary.
function own(object, key) { return Object.prototype.hasOwnProperty.call(object, key) }
function identifier(value) { return Bridge.call("health.identifier",[value]) }
function clean(value, limit) { return Bridge.call("health.clean",[value,limit]) }
function registration(value) { return Bridge.call("health.registration",[value]) }
function publication(reg, value, now) { return Bridge.call("health.publication",[reg,value,Bridge.number(now)]) }
function rows(registrations, values, now) {
  var groups=Bridge.call("health.rowGroups",[registrations,values,Bridge.number(now),Object.keys(registrations)])
  // Qt owns locale-aware display ordering; Rust already separated priorities.
  return groups[0].sort(function(a,b){return a.name.localeCompare(b.name)}).concat(groups[1].sort(function(a,b){return a.name.localeCompare(b.name)}))
}
