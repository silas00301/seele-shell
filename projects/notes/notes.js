function filter(notes, query, trash) {
  var words = String(query || "").trim().toLowerCase().split(/\s+/).filter(Boolean)
  return (notes || []).filter(function(note) {
    var text = (note.title + " " + note.body).toLowerCase()
    return !!note.trashed === !!trash && words.every(function(word) { return text.indexOf(word) >= 0 })
  })
}

function duration(milliseconds) {
  var seconds = Math.max(0, Math.floor(Number(milliseconds || 0) / 1000))
  return Math.floor(seconds / 60) + ":" + String(seconds % 60).padStart(2, "0")
}

// Keep newer local text when an older save finishes. Also used after reconnect
// so an interrupted write never replaces the draft still in the editor.
function merge(incoming, existing, acknowledgedVersion) {
  if (existing && existing._dirty && existing._version !== acknowledgedVersion) {
    incoming.title = existing.title
    incoming.body = existing.body
    incoming._version = existing._version
    incoming._dirty = true
  } else {
    incoming._version = existing ? existing._version : 0
    incoming._dirty = false
  }
  return incoming
}
