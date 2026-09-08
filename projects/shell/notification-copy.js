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

function payload(entry, part) {
  if (!entry || (part !== "title" && part !== "body")) return {ok:false, message:"Nothing to copy."}
  var value = part === "title" ? String(entry.summary || "") : bodyText(entry.body)
  if (!value) return {ok:false, message:"Nothing to copy."}
  if (value.length > 262144) return {ok:false, message:"Notification text is too large to copy."}
  return {ok:true, value:value}
}
