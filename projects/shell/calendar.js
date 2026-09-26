function day(value) { return String(value || "").slice(0, 10) }
function start(event) { return event && event.start && (event.start.dateTime || event.start.date) || "" }
function end(event) { return event && event.end && (event.end.dateTime || event.end.date) || "" }
function allDay(event) { return !!(event && event.start && event.start.date) }
function millis(value) { return Date.parse(value || "") }
function visible(event) {
  if (!event || event.status === "cancelled") return false
  var self = (event.attendees || []).find(function(person) { return person.self })
  return !self || self.responseStatus !== "declined"
}
function onDay(event, date) {
  if (!visible(event)) return false
  if (allDay(event)) return start(event) <= date && end(event) > date
  var from = millis(start(event)), to = millis(end(event))
  var a = millis(date + "T00:00:00"), next = new Date(a)
  next.setDate(next.getDate() + 1)
  var b = next.getTime()
  return from < b && to > a
}
function agenda(events, selected, date) {
  return (events || []).filter(function(event) { return selected.indexOf(event.calendar_id) >= 0 && onDay(event, date) })
    .sort(function(a,b) {
      if (allDay(a) !== allDay(b)) return allDay(a) ? -1 : 1
      var x = allDay(a) ? start(a).localeCompare(start(b)) : millis(start(a)) - millis(start(b))
      return x || String(a.id).localeCompare(String(b.id))
    })
}
function dots(events, calendars, selected, date) {
  var ids = {}
  agenda(events, selected, date).forEach(function(event) { ids[event.calendar_id] = true })
  return (calendars || []).filter(function(calendar) { return ids[calendar.id] })
    .map(function(calendar) { return { id: calendar.id, color: calendar.backgroundColor || "" } })
}
function indicator(events, selected, now) {
  var eligible = (events || []).filter(function(event) {
    if (allDay(event) || !visible(event) || selected.indexOf(event.calendar_id) < 0) return false
    var from = millis(start(event)), to = millis(end(event))
    return from <= now + 900000 && to > now
  })
  eligible.sort(function(a,b) {
    var ax = millis(start(a)), bx = millis(start(b))
    var af = ax > now, bf = bx > now
    if (af !== bf) return af ? -1 : 1
    var delta = af ? ax - bx : millis(end(a)) - millis(end(b))
    return delta || String(a.calendar_id + ":" + a.id).localeCompare(String(b.calendar_id + ":" + b.id))
  })
  if (!eligible.length) return null
  var chosen = eligible[0], from = millis(start(chosen))
  return { event: chosen, extra: eligible.length - 1, text: from > now ? Math.max(1, Math.ceil((from-now)/60000)) + "m" : "Ongoing" }
}
// The time selected calendars hold, as blocks in epoch seconds overlapping
// [from, to). An event marked free, an all-day entry and a working location
// leave the day open; declined and cancelled events are not shown at all.
function busy(events, selected, calendars, colors, from, to) {
  return (events || []).filter(function(event) {
    return selected.indexOf(event.calendar_id) >= 0 && visible(event) && !allDay(event)
      && event.transparency !== "transparent" && event.eventType !== "workingLocation"
  }).map(function(event) {
    return { start: Math.floor(millis(start(event)) / 1000), end: Math.ceil(millis(end(event)) / 1000),
      title: event.summary || "Busy", color: color(event, calendars, colors) }
  }).filter(function(block) { return block.end > block.start && block.start < to && block.end > from })
    .sort(function(a, b) { return a.start - b.start || a.end - b.end || a.title.localeCompare(b.title) })
}
function safeLink(value) {
  if (typeof value !== "string" || value.length > 4096 || !/^https:\/\/[^\s/@]+(?:[:/]|$)/i.test(value) || /[\u0000-\u0020\u007f]/.test(value)) return ""
  return value
}
function meeting(event) {
  if (!event) return ""
  var entry = (event.conferenceData && event.conferenceData.entryPoints || []).find(function(point) { return point.entryPointType === "video" })
  return safeLink(event.hangoutLink || entry && entry.uri || "")
}
function color(event, calendars, colors) {
  var calendar = (calendars || []).find(function(item) { return item.id === event.calendar_id }) || {}
  return event.colorId && colors && colors.event && colors.event[event.colorId]
    ? colors.event[event.colorId].background : calendar.backgroundColor || ""
}
function allDayLabel(event) {
  var first = start(event), exclusive = end(event)
  var last = new Date(exclusive + "T12:00:00")
  last.setDate(last.getDate() - 1)
  var finalDay = [last.getFullYear(), String(last.getMonth()+1).padStart(2,"0"), String(last.getDate()).padStart(2,"0")].join("-")
  return first === finalDay ? "All day" : "All day · " + first + "–" + finalDay
}
function rsvp(event) {
  var self = (event.attendees || []).find(function(person) { return person.self })
  var status = self && self.responseStatus
  return status === "accepted" ? "Accepted" : status === "tentative" ? "Tentative" : status === "needsAction" ? "Unanswered" : ""
}
function description(event) {
  return String(event && event.description || "").replace(/<[^>]*>/g, " ")
    .replace(/&nbsp;/g, " ").replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">")
    .replace(/[\u0000-\u001f\u007f]/g, " ").trim()
}
