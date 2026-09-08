// Only actions advertised by the sender are offered; dismissed history is inert.
function actions(entry) {
  var result = []
  var advertised = entry && entry.actions
  if (!advertised || typeof advertised !== "object" || Array.isArray(advertised)) return result
  for (var key in advertised) {
    if (key !== "default" && typeof advertised[key] === "string")
      result.push({ key: key, label: advertised[key] || key })
  }
  return result
}

// Require verification context and an unambiguous code. Never mine URLs or
// ordinary message numbers (dates, prices, phone numbers) for clipboard actions.
function verificationCode(entry) {
  var text = String(entry.summary || "") + " " + String(entry.body || "")
  text = text.replace(/<[^>]*>/g, " ").replace(/&(?:nbsp|#160);/gi, " ")
    .replace(/https?:\/\/\S+/gi, " ")
    .replace(/\b[0-9]{4}[-/][0-9]{1,2}[-/][0-9]{1,2}\b/g, " ")
  var context = /\b(?:(?:verification|security|authentication|confirmation|login|access|reset|sign[ -]?in|one[ -]?time|two[ -]?factor|your)[ -]+(?:code|pin)|otp|2fa|verifizierungscode|bestätigungscode|sicherheitscode|anmeldecode)\b|\b(?:use|enter)\b.{0,80}\b(?:sign[ -]?in|verify|authenticate)\b/i
  if (!context.test(text)) return ""
  var candidates = text.match(/\b(?:[0-9]{4,8}|[A-Z0-9]{6,8}|[0-9]{3}[ -][0-9]{3})\b/g) || []
  var codes = []
  for (var i = 0; i < candidates.length; i++) {
    var code = candidates[i].replace(/[ -]/g, "")
    if (!/[0-9]/.test(code) || codes.indexOf(code) !== -1) continue
    codes.push(code)
  }
  return codes.length === 1 ? codes[0] : ""
}

function groupKey(entry) {
  var desktop = String(entry.desktop_entry || "").trim().replace(/\.desktop$/, "").toLowerCase()
  if (desktop) return "desktop:" + desktop
  var app = String(entry.app_name || "").trim().toLowerCase()
  return app ? "app:" + app : "id:" + entry.id
}

// Input is newest first. An app moves to the front only when it has new content.
// One row per group, whether it is open or not: expanding a stack then grows
// that group's own card instead of inserting rows into the list, so nothing
// below it is displaced and the group cannot move out from under the pointer
// that just opened it.
function stackedRows(entries, expanded) {
  var groups = [], byKey = Object.create(null)
  for (var i = 0; i < entries.length; i++) {
    var key = groupKey(entries[i])
    if (!byKey[key]) {
      byKey[key] = { key: key, group: key, items: [] }
      groups.push(byKey[key])
    }
    byKey[key].items.push(entries[i])
  }
  for (var g = 0; g < groups.length; g++) {
    var group = groups[g]
    group.count = group.items.length
    // A single notification is never a stack, however its group was left.
    group.expanded = group.count > 1 && !!expanded[group.key]
    group.depth = group.expanded ? 0 : Math.min(2, group.count - 1)
  }
  return groups
}

function localImage(source) {
  source = String(source || "")
  return /^(?:image:\/\/|file:\/\/\/|\/)/.test(source) ? source : ""
}

// Notification markup supports formatting and links, never remote inline images.
function bodyMarkup(body) {
  return String(body || "").replace(/<[^>]*>/g, function(tag) {
    if (/^<\/?(?:b|i|u)\s*>$/i.test(tag) || /^<br\s*\/?\s*>$/i.test(tag)) return tag
    if (/^<\/a\s*>$/i.test(tag)) return tag
    var link = /^<a\s+href=["'](https?:\/\/[^"'<>]+|mailto:[^"'<>]+)["']\s*>$/i.exec(tag)
    return link ? '<a href="' + link[1].replace(/"/g, "&quot;") + '">' : ""
  })
}

function fromNative(n, now) {
  var advertised = Object.create(null)
  for (var i = 0; i < n.actions.length; i++) advertised[n.actions[i].identifier] = n.actions[i].text
  var hints = n.hints || {}, progress = Number(hints.value)
  return { id: n.id, app_name: n.appName, app_icon: n.appIcon, desktop_entry: n.desktopEntry,
    summary: n.summary, body: n.body, actions: advertised, action_icons: !!n.hasActionIcons,
    image: localImage(n.image), urgency: Number(n.urgency), resident: !!n.resident,
    transient: !!n.transient, timeout: Number(n.expireTimeout), time: now,
    progress: hints.value === undefined || !isFinite(progress) ? -1 : Math.max(0, Math.min(100, progress)),
    tag: String(hints["x-dunst-stack-tag"] || hints["x-canonical-private-synchronous"] || ""),
    pinned: false }
}

function permanent(entry) { return entry.pinned || entry.timeout === 0 || entry.urgency === 2 }

// The pinned Quickshell revision exposes the raw D-Bus milliseconds despite
// its property's seconds documentation. Only -1 selects our 30-second default.
function popupDuration(entry) { return permanent(entry) ? -1 : entry.timeout > 0 ? entry.timeout / 1000 : 30 }

// Session state stays in memory; notification text and verification codes are
// never written to disk. QObjects remain separate from the serializable view.
function createStore(publish, arrived, now) {
  var state = { current: [], history: [], dnd: false, paused: false, lastTick: now, restored: {} }
  state.save = function() {
    var metadata = {}
    state.current.forEach(function(record) {
      metadata[String(record.entry.id)] = { time: record.entry.time, pinned: record.entry.pinned,
        popup: record.popup && permanent(record.entry) }
    })
    return { history: state.history, dnd: state.dnd, metadata: metadata }
  }
  state.restore = function(saved) {
    if (!saved) return
    state.history = saved.history || []
    state.dnd = !!saved.dnd
    state.restored = saved.metadata || {}
    state.publish()
  }
  state.view = function() {
    var items = [], popups = []
    for (var i = 0; i < state.current.length; i++) {
      var record = state.current[i]
      if (!record.entry.transient) items.push(record.entry)
      if (record.popup) popups.push(record.entry)
    }
    return { count: items.length, items: items, popups: popups, history: state.history }
  }
  state.publish = function() { publish(state.view(), state.dnd) }
  state.find = function(id) {
    for (var i = 0; i < state.current.length; i++) if (String(state.current[i].entry.id) === String(id)) return state.current[i]
    return null
  }
  state.receive = function(n, timestamp) {
    var entry = fromNative(n, timestamp), record = state.find(n.id)
    if (record) {
      entry.pinned = record.entry.pinned
      var fresh = entry.summary !== record.entry.summary || entry.body !== record.entry.body
      var durationChanged = entry.timeout !== record.entry.timeout || entry.urgency !== record.entry.urgency
      entry.time = fresh ? timestamp : record.entry.time
      record.entry = entry
      if (fresh) {
        record.popup = !state.dnd
        record.remaining = popupDuration(entry)
        record.clock = timestamp
        state.current = state.current.filter(function(other) { return other !== record })
        state.current.unshift(record)
        if (record.popup) arrived(entry, false)
      } else if (durationChanged) {
        record.remaining = popupDuration(entry)
        record.clock = timestamp
      }
      state.publish()
      return
    }
    // Stack tags replace an app's earlier progress/status notification, while
    // app grouping below is only presentation and never changes notification IDs.
    if (entry.tag) {
      state.current.slice().forEach(function(other) {
        if (other.entry.tag === entry.tag && groupKey(other.entry) === groupKey(entry)) {
          other.skipHistory = true
          other.notification.dismiss()
        }
      })
    }
    var restored = n.lastGeneration ? state.restored[String(n.id)] : null
    if (restored) {
      entry.time = restored.time
      entry.pinned = !!restored.pinned
      delete state.restored[String(n.id)]
    }
    record = { notification: n, entry: entry, remaining: popupDuration(entry), clock: timestamp,
      popup: !state.dnd && (!n.lastGeneration || !!(restored && restored.popup)), skipHistory: false }
    state.current.unshift(record)
    if (record.popup) arrived(entry, true)
    state.publish()
  }
  state.closed = function(id, reason) {
    var record = state.find(id)
    if (!record) return
    state.current = state.current.filter(function(other) { return other !== record })
    if (!record.skipHistory && !record.entry.transient && reason === 2) {
      var item = Object.assign({}, record.entry, { actions: {}, image: "", pinned: false })
      state.history = [item].concat(state.history).slice(0, 100)
    }
    state.publish()
  }
  state.advance = function(timestamp) {
    var changed = false
    state.lastTick = timestamp
    state.current.slice().forEach(function(record) {
      var elapsed = Math.max(0, timestamp - record.clock)
      record.clock = timestamp
      if (state.paused || record.remaining < 0) return
      // Transient entries suppressed by DND still expire without entering history.
      if (!record.popup && !record.entry.transient) return
      record.remaining -= elapsed
      if (record.remaining > 0) return
      record.popup = false
      changed = true
      if (record.entry.transient) record.notification.expire()
    })
    var history = state.history.filter(function(item) { return item.time > timestamp - 86400 })
    if (history.length !== state.history.length) { state.history = history; changed = true }
    if (changed) state.publish()
  }
  state.pause = function(paused, timestamp) { state.advance(timestamp); state.paused = paused }
  state.retire = function(id) {
    var record = state.find(id)
    if (!record) return false
    record.popup = false
    if (record.entry.transient) record.notification.dismiss()
    state.publish()
    return true
  }
  state.dismiss = function(id) {
    var record = state.find(id)
    if (!record) return false
    record.notification.dismiss()
    return true
  }
  state.invoke = function(id, key) {
    var record = state.find(id)
    if (!record) return false
    var available = record.notification.actions
    for (var i = 0; i < available.length; i++) {
      if (available[i].identifier !== key) continue
      available[i].invoke()
      return true
    }
    return false
  }
  state.pin = function(id) {
    var record = state.find(id)
    if (!record) return false
    record.entry = Object.assign({}, record.entry, { pinned: !record.entry.pinned })
    record.remaining = popupDuration(record.entry)
    if (record.entry.pinned && !state.dnd) {
      record.popup = true
      arrived(record.entry, false)
    }
    state.publish()
    return true
  }
  state.setDnd = function(enabled) {
    state.dnd = enabled
    if (enabled) state.current.forEach(function(record) { record.popup = false })
    state.publish()
  }
  state.clear = function(history) {
    if (history) { state.history = []; state.publish(); return }
    state.current.slice().forEach(function(record) { record.notification.dismiss() })
  }
  state.group = function(key, popup) {
    state.current.slice().forEach(function(record) {
      if (groupKey(record.entry) === key) {
        if (popup) state.retire(record.entry.id)
        else state.dismiss(record.entry.id)
      }
    })
  }
  return state
}
