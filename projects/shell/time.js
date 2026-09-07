function monthDate(now, offset) {
  return new Date(now.getFullYear(), now.getMonth() + Number(offset || 0), 1, 12)
}

// Use local calendar fields: UTC conversion can move a date across midnight.
function calendarDate(date) {
  if (!date || !Number.isFinite(date.getTime())) return ""
  return String(date.getFullYear()).padStart(4, "0") + "-"
    + String(date.getMonth() + 1).padStart(2, "0") + "-"
    + String(date.getDate()).padStart(2, "0")
}

function calendarCopyDate(cell) {
  return cell && !cell.week && cell.inMonth ? String(cell.date || "") : ""
}

function sameDay(left, right) {
  return left.getFullYear() === right.getFullYear()
    && left.getMonth() === right.getMonth()
    && left.getDate() === right.getDate()
}

function isoWeek(date) {
  var value = new Date(date.getFullYear(), date.getMonth(), date.getDate(), 12)
  var day = value.getDay() || 7
  value.setDate(value.getDate() + 4 - day)
  var yearStart = new Date(value.getFullYear(), 0, 1, 12)
  return Math.ceil((((value - yearStart) / 86400000) + 1) / 7)
}

// The Monday-first rows a month actually occupies: four when it is February
// starting on a Monday, six when a long month starts on a Sunday.
function calendarWeeks(now, offset) {
  var month = monthDate(now, offset)
  var mondayOffset = (month.getDay() + 6) % 7
  var days = new Date(month.getFullYear(), month.getMonth() + 1, 0, 12).getDate()
  return Math.ceil((mondayOffset + days) / 7)
}

// A month's section carries that month's days and nothing else. The days on
// either side of it belong to the sections above and below, so their cells are
// left empty rather than filled in to square the block off, and a month that
// needs four rows is drawn four rows tall.
function calendarCells(now, offset) {
  var month = monthDate(now, offset)
  var mondayOffset = (month.getDay() + 6) % 7
  var weeks = calendarWeeks(now, offset)
  var cursor = new Date(month.getFullYear(), month.getMonth(), 1 - mondayOffset, 12)
  var cells = []

  for (var week = 0; week < weeks; week++) {
    cells.push({ week: true, label: isoWeek(cursor) })
    for (var day = 0; day < 7; day++) {
      var value = new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() + day, 12)
      var inMonth = value.getMonth() === month.getMonth()
        && value.getFullYear() === month.getFullYear()
      cells.push({
        week: false,
        inMonth: inMonth,
        day: inMonth ? value.getDate() : 0,
        date: inMonth ? calendarDate(value) : "",
        today: inMonth && sameDay(value, now)
      })
    }
    cursor.setDate(cursor.getDate() + 7)
  }

  return cells
}

function filterZones(zones, query) {
  var needle = String(query || "").trim().toLowerCase()
  if (needle === "") return zones || []
  var result = []
  zones = zones || []
  for (var i = 0; i < zones.length; i++) {
    var zone = zones[i]
    var searchable = [zone.id, zone.zone, zone.label, zone.aliases, zone.flag].join(" ").toLowerCase()
    if (searchable.indexOf(needle) >= 0) result.push(zone)
  }
  return result
}

function offsetTime(now, offset, includeSeconds) {
  var match = String(offset || "").match(/^([+-])(\d{2})(\d{2})$/)
  if (!match) return ""
  var minutes = Number(match[2]) * 60 + Number(match[3])
  if (match[1] === "-") minutes = -minutes
  var shifted = new Date(now.getTime() + minutes * 60000)
  var hours = String(shifted.getUTCHours()).padStart(2, "0")
  var clockMinutes = String(shifted.getUTCMinutes()).padStart(2, "0")
  var seconds = String(shifted.getUTCSeconds()).padStart(2, "0")
  return hours + ":" + clockMinutes + (includeSeconds ? ":" + seconds : "")
}

function orderZones(zones, pinned, query) {
  var filtered = filterZones(zones, query)
  var ordered = []
  var seen = {}
  pinned = pinned || []
  if (pinned.length === 0) return filtered.slice()

  for (var pin = 0; pin < pinned.length; pin++) {
    for (var i = 0; i < filtered.length; i++) {
      if (filtered[i].id === pinned[pin] && !seen[filtered[i].id]) {
        ordered.push(filtered[i])
        seen[filtered[i].id] = true
        break
      }
    }
  }
  for (var zone = 0; zone < filtered.length; zone++) {
    if (!seen[filtered[zone].id]) ordered.push(filtered[zone])
  }
  return ordered
}

// Calendar keyboard navigation stays at local noon across DST boundaries.
function moveCalendarDate(now, selected, days) {
  var match = String(selected || calendarDate(now)).match(/^(\d{4})-(\d{2})-(\d{2})$/)
  if (!match || !Number.isInteger(days)) return null
  var date = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]), 12)
  date.setFullYear(Number(match[1]))
  if (calendarDate(date) !== match[0]) return null
  date.setDate(date.getDate() + days)
  var offset = (date.getFullYear() - now.getFullYear()) * 12 + date.getMonth() - now.getMonth()
  if (offset < -60 || offset > 60) return null
  return { date: calendarDate(date), monthOffset: offset }
}
