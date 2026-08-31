function monthDate(now, offset) {
  return new Date(now.getFullYear(), now.getMonth() + Number(offset || 0), 1, 12)
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

// Six Monday-first rows keep every month the same height and make scrolling stable.
function calendarCells(now, offset) {
  var month = monthDate(now, offset)
  var mondayOffset = (month.getDay() + 6) % 7
  var cursor = new Date(month.getFullYear(), month.getMonth(), 1 - mondayOffset, 12)
  var cells = []

  for (var week = 0; week < 6; week++) {
    cells.push({ week: true, label: isoWeek(cursor) })
    for (var day = 0; day < 7; day++) {
      var value = new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() + day, 12)
      cells.push({
        week: false,
        day: value.getDate(),
        inMonth: value.getMonth() === month.getMonth(),
        today: sameDay(value, now)
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
