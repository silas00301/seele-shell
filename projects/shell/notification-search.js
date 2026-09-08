// Search the visible body text, without matching hidden markup or link targets.
function bodyText(value) {
  return String(value || "").replace(/<br\s*\/?\s*>/gi, "\n").replace(/<[^>]*>/g, "")
    .replace(/&(#x[0-9a-f]+|#[0-9]+|amp|lt|gt|quot|apos|nbsp);/gi, function(match, entity) {
      var name = entity.toLowerCase()
      var named = {amp:"&", lt:"<", gt:">", quot:'"', apos:"'", nbsp:" "}
      if (name[0] !== "#") return named[name]
      var number = name[1] === "x" ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10)
      return number > 0 && number <= 0x10ffff && !(number >= 0xd800 && number <= 0xdfff)
        ? String.fromCodePoint(number) : match
    })
}

function filter(entries, query) {
  var terms = String(query || "").slice(0, 256).trim().toLowerCase().split(/\s+/).filter(Boolean)
  if (!terms.length) return entries
  return entries.filter(function(entry) {
    var text = (String(entry.app_name || "") + "\n" + String(entry.summary || "") + "\n" + bodyText(entry.body)).toLowerCase()
    return terms.every(function(term) { return text.indexOf(term) !== -1 })
  })
}
