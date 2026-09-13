.import "../shared/Native.js" as Bridge

function calendarDate(date) { return Bridge.call("time.calendarDate", [Bridge.date(date)]) }
function calendarCopyDate(cell) { return Bridge.call("time.calendarCopyDate", [cell]) }
function sameDay(left, right) { return Bridge.call("time.sameDay", [Bridge.date(left), Bridge.date(right)]) }
function isoWeek(date) { return Bridge.call("time.isoWeek", [Bridge.date(date)]) }
function calendarWeeks(now, offset) { return Bridge.call("time.calendarWeeks", [Bridge.date(now), offset]) }
function calendarCells(now, offset) { return Bridge.call("time.calendarCells", [Bridge.date(now), offset]) }
function moveCalendarDate(now, selected, days) { return Bridge.call("time.moveCalendarDate", [Bridge.date(now), selected, Bridge.number(days)]) }
function offsetTime(now, offset, includeSeconds) { return Bridge.call("time.offsetTime", [Bridge.date(now), offset, includeSeconds]) }
function formatOffset(offset) { return Bridge.call("time.formatOffset", [offset]) }
function monthDate(now, offset) {
  var month = Bridge.call("time.monthDate", [Bridge.date(now), offset])
  if (month === null) return new Date(NaN)
  var value = new Date(0)
  value.setFullYear(month[0], month[1] - 1, 1)
  value.setHours(12, 0, 0, 0)
  return value
}
function clockTimestamp(now, offset) {
  return Bridge.call("time.clockTimestamp", [Bridge.date(now), offset === undefined ? { local: true } : offset])
}
function filterZones(zones, query) {
  return Bridge.call("time.filterZones", [zones, query]).map(function(index) { return zones[index] })
}
function orderZones(zones, pinned, query) {
  return Bridge.call("time.orderZones", [zones, pinned, query]).map(function(index) { return zones[index] })
}
