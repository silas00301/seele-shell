// Pure helpers for Seele Notes. Every editing command is expressed as one
// replacement — replace [start, end) with `text`, then put the caret at
// `caret` — so the editor can apply it through TextArea's own insert and
// remove and the undo stack keeps working. A command that rewrote the whole
// document would take the caret, the selection and the undo history with it.

function filter(notes, query) {
  var words = String(query || "").trim().toLowerCase().split(/\s+/).filter(Boolean)
  return (notes || []).filter(function(note) {
    var haystack = (note.title + " " + note.name + " " + note.excerpt).toLowerCase()
    return words.every(function(word) { return haystack.indexOf(word) >= 0 })
  })
}

function duration(milliseconds) {
  var seconds = Math.max(0, Math.floor(Number(milliseconds || 0) / 1000))
  return Math.floor(seconds / 60) + ":" + String(seconds % 60).padStart(2, "0")
}

function label(note) {
  if (!note) return "Untitled note"
  return String(note.title || note.name || "").trim() || "Untitled note"
}

// A capture is usually recent, so the row says how recent rather than making
// the reader parse a date.
function when(seconds, now) {
  var stamp = new Date(Number(seconds || 0) * 1000)
  var today = new Date(Number(now || Date.now()))
  var midnight = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime()
  var pad = function(value) { return String(value).padStart(2, "0") }
  if (stamp.getTime() >= midnight) return pad(stamp.getHours()) + ":" + pad(stamp.getMinutes())
  if (stamp.getTime() >= midnight - 86400000) return "Yesterday"
  if (stamp.getFullYear() === today.getFullYear())
    return pad(stamp.getDate()) + "." + pad(stamp.getMonth() + 1)
  return stamp.getFullYear() + "-" + pad(stamp.getMonth() + 1) + "-" + pad(stamp.getDate())
}

function lineStart(text, position) {
  var start = text.lastIndexOf("\n", Math.max(0, position - 1))
  return start < 0 ? 0 : start + 1
}

function lineEnd(text, position) {
  var end = text.indexOf("\n", position)
  return end < 0 ? text.length : end
}

// Wrap or unwrap a selection. Toggling off is what makes the shortcut a
// toggle rather than a way to accumulate asterisks.
function wrap(text, start, end, marker) {
  var before = text.slice(Math.max(0, start - marker.length), start)
  var after = text.slice(end, end + marker.length)
  if (before === marker && after === marker) {
    return {
      start: start - marker.length,
      end: end + marker.length,
      text: text.slice(start, end),
      caret: start - marker.length,
      selectionEnd: end - marker.length,
    }
  }
  var selected = text.slice(start, end)
  return {
    start: start,
    end: end,
    text: marker + selected + marker,
    caret: start + marker.length,
    selectionEnd: end + marker.length,
  }
}

// Setting a level replaces whatever level the line already carried, and
// asking for the level a line already has clears it.
function heading(text, position, level) {
  var start = lineStart(text, position)
  var line = text.slice(start, lineEnd(text, position))
  var match = /^(#{1,6})\s+/.exec(line)
  var current = match ? match[1].length : 0
  var body = match ? line.slice(match[0].length) : line
  var prefix = level > 0 && level !== current ? "#".repeat(level) + " " : ""
  return {
    start: start,
    end: start + line.length,
    text: prefix + body,
    caret: Math.max(start + prefix.length, position + prefix.length - (match ? match[0].length : 0)),
  }
}

function task(text, position) {
  var start = lineStart(text, position)
  var line = text.slice(start, lineEnd(text, position))
  var match = /^(\s*)(?:([-*+])\s+)?(?:\[([ xX])\]\s+)?/.exec(line)
  var indent = match[1]
  var body = line.slice(match[0].length)
  var box = match[3] === undefined ? "[ ] " : (match[3] === " " ? "[x] " : "")
  var bullet = box ? "- " : (match[2] ? match[2] + " " : "")
  var replacement = indent + bullet + box + body
  return {
    start: start,
    end: start + line.length,
    text: replacement,
    caret: start + replacement.length - body.length + Math.max(0, position - (start + match[0].length)),
  }
}

function link(text, start, end) {
  var selected = text.slice(start, end)
  var looksLikeUrl = /^[a-z][a-z0-9+.-]*:\/\//i.test(selected)
  var replacement = looksLikeUrl ? "[](" + selected + ")" : "[" + selected + "]()"
  return {
    start: start,
    end: end,
    text: replacement,
    caret: start + (looksLikeUrl ? 1 : selected.length + 3),
  }
}

// Pressing Return inside a list continues it, and pressing it on an empty
// item ends the list instead of leaving a dangling marker behind.
function newline(text, position) {
  var start = lineStart(text, position)
  var line = text.slice(start, position)
  var match = /^(\s*)([-*+]|\d+[.)])\s+(\[[ xX]\]\s+)?/.exec(line)
  if (!match) return null
  if (line.length === match[0].length) {
    return { start: start, end: position, text: "", caret: start }
  }
  var marker = match[2]
  var ordered = /^\d+/.exec(marker)
  var next = ordered
    ? String(parseInt(ordered[0], 10) + 1) + marker.slice(ordered[0].length) + " "
    : marker + " "
  var box = match[3] ? "[ ] " : ""
  var inserted = "\n" + match[1] + next + box
  return { start: position, end: position, text: inserted, caret: position + inserted.length }
}

// An embed is its own block. A line placed directly under a paragraph is a
// lazy continuation of it, which Obsidian renders as part of the sentence
// rather than as a player, so the embed takes a blank line above it unless it
// already has one.
function embed(text, position, name) {
  var reference = "![[" + name + "]]"
  var before = text.slice(0, position)
  var lead = before.length === 0 || before.endsWith("\n\n")
    ? ""
    : before.endsWith("\n") ? "\n" : "\n\n"
  var after = text.slice(position)
  var trail = after.startsWith("\n") || after.length === 0 ? "" : "\n"
  var inserted = lead + reference + "\n" + trail
  return { start: position, end: position, text: inserted, caret: position + inserted.length }
}

// Removing an embed only removes the reference. The recording stays in the
// vault, because another note may be pointing at the same file.
function unembed(text, name) {
  var pattern = new RegExp("^[ \\t]*!\\[\\[" + name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "(\\|[^\\]]*)?\\]\\][ \\t]*\\n?", "m")
  var match = pattern.exec(text)
  if (!match) return null
  return { start: match.index, end: match.index + match[0].length, text: "", caret: match.index }
}

// The one line that says what the file on disk currently is. Every branch is
// a state the user can act on, so none of them may be spelled "Saved".
function status(state, seconds, now) {
  switch (state) {
    case "idle": return "Up to date"
    case "dirty": return "Unsaved changes"
    case "saving": return "Saving…"
    case "saved": return "Saved " + when(seconds, now)
    case "conflict": return "Changed on disk while you were editing"
    case "gone": return "This file was moved or deleted elsewhere"
    case "failed": return "Could not be saved"
    default: return ""
  }
}

// Obsidian opens a note by vault name and vault-relative path.
function obsidianUri(vault, path) {
  var name = String(vault || "").replace(/\/+$/, "").split("/").pop()
  if (!name || !path) return ""
  return "obsidian://open?vault=" + encodeURIComponent(name) + "&file=" + encodeURIComponent(path)
}
