.pragma library
.import Seele.Core 1.0 as Native

// Qt owns Date conversion and live object identity. Policy and data algorithms
// execute synchronously in the shared Rust library, never in subprocesses.
function call(operation, args) { return Native.Functions.call(operation, args) }
function number(value) {
  var numeric = Number(value)
  return Number.isFinite(numeric) ? numeric : String(numeric)
}
function date(value) {
  if (!value || !Number.isFinite(value.getTime())) return null
  return { epoch: value.getTime(), year: value.getFullYear(), month: value.getMonth() + 1,
    day: value.getDate(), hour: value.getHours(), minute: value.getMinutes(), offset: -value.getTimezoneOffset() }
}
function localDates(seconds, now) {
  var stamp = new Date(Number(seconds || 0) * 1000)
  var today = new Date(Number(now || Date.now()))
  return [date(stamp), date(today), new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime()]
}

function notificationState(now) { return Native.Functions.notificationState(now) }
