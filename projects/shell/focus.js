.import "../shared/Native.js" as Bridge

function initial() { return Bridge.call("focus.initial", []) }
function valid(state) { return Bridge.call("focus.valid", [state]) }
function label(seconds) { return Bridge.call("focus.label", [seconds]) }
function update(saved, action, now, minutes) {
  var next = Bridge.call("focus.update", [saved, action, Bridge.number(now), Bridge.number(minutes)])
  return next === null ? saved : next
}
