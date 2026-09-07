function monthDate(now, offset) {
  return new Date(now.getFullYear(), now.getMonth() + Number(offset || 0), 1, 12)
}

function sameDay(left, right) {
  return left.getFullYear() === right.getFullYear()
    && left.getMonth() === right.getMonth()
    && left.getDate() === right.getDate()
}

function isoWeek(date) {
  // Calendar arithmetic must not include the local zone's DST transitions.
  var value = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()))
  var day = value.getUTCDay() || 7
  value.setUTCDate(value.getUTCDate() + 4 - day)
  var yearStart = new Date(Date.UTC(value.getUTCFullYear(), 0, 1))
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
        today: inMonth && sameDay(value, now)
      })
    }
    cursor.setDate(cursor.getDate() + 7)
  }

  return cells
}

function filterZones(zones, query) {
  var words = String(query || "").trim().toLowerCase().replace(/[_/]/g, " ").split(/\s+/).filter(Boolean)
  if (words.length === 0) return zones || []
  var result = []
  zones = zones || []
  for (var i = 0; i < zones.length; i++) {
    var zone = zones[i]
    var searchable = [zone.id, zone.zone, zone.label, zone.aliases, zone.flag, zone.abbreviation, formatOffset(zone.offset)].join(" ").toLowerCase().replace(/[_/]/g, " ")
    if (words.every(function(word) { return searchable.indexOf(word) >= 0 })) result.push(zone)
  }
  return result
}

function offsetTime(now, offset, includeSeconds) {
  var match = String(offset || "").match(/^([+-])(\d{2})(\d{2})$/)
  if (!match || Number(match[2]) > 23 || Number(match[3]) > 59) return ""
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

function formatOffset(offset) {
  var match = String(offset || "").match(/^([+-])(\d{2})(\d{2})$/)
  return match ? "UTC" + match[1] + match[2] + ":" + match[3] : ""
}
