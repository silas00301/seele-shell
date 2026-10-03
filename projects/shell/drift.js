// Which drifted checks a Restore names, and the exact argv that names them.
// The panel decides inclusion; seele-drift decides whether a check is still
// drifted and is the only thing that can change the machine.

function chosenIds(checks, skipped) {
  var skip = skipped || []
  var ids = []
  var list = checks || []
  for (var i = 0; i < list.length; i++) {
    var item = list[i]
    if (!item || !item.drifted || item.unavailable) continue
    if (skip.indexOf(item.id) >= 0) continue
    ids.push(item.id)
  }
  return ids
}

function argumentsFor(action, ids) {
  if (action === "diff") return ["seele-drift", "diff"]
  if (action !== "apply") return null
  var command = ["seele-drift", "apply"]
  var list = ids || []
  for (var i = 0; i < list.length; i++) {
    var id = String(list[i] || "")
    if (!/^[a-z0-9-]{1,32}$/.test(id)) return null
    command.push(id)
  }
  if (command.length < 3) return null
  return command
}
