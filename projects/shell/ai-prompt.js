var contextPattern = /(^|[^\w@])@(clip|select|window|dir|screen)\b/g

function mentions(value) {
  var result = []
  var match
  contextPattern.lastIndex = 0
  while ((match = contextPattern.exec(String(value || ""))) !== null) {
    if (result.indexOf(match[2]) < 0) result.push(match[2])
  }
  return result
}

function has(values, kind) {
  return values.indexOf(kind) >= 0
}

function permissions(values, clipAllowed, selectionAllowed) {
  var result = []
  if (has(values, "clip") && clipAllowed) result.push("clip")
  if (has(values, "select") && selectionAllowed) result.push("select")
  return result
}

function canSubmit(value, values, clipReady, selectionReady, directoryReady, screenReady, busy) {
  if (busy || String(value || "").trim() === "") return false
  if (has(values, "clip") && !clipReady) return false
  if (has(values, "select") && !selectionReady) return false
  if (has(values, "dir") && !directoryReady) return false
  if (has(values, "screen") && !screenReady) return false
  return true
}

function preview(value, characters, truncated) {
  var text = String(value || "")
  if (text === "") text = "Empty text"
  text = text.replace(/\r\n?/g, "\n").replace(/\n/g, "  ↵  ")
  var count = Number(characters || 0)
  var suffix = count > 0 ? " · " + count + " character" + (count === 1 ? "" : "s") : ""
  if (truncated) suffix += " · first 65,536 characters"
  return text + suffix
}
