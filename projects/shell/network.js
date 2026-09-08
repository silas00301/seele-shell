// iproute2's address snapshot belongs to the interface selected by the kernel route.
function addresses(entries, device) {
  if (!Array.isArray(entries)) return []
  var rows = [], seen = {}
  for (var i = 0; i < entries.length && rows.length < 8; i++) {
    var entry = entries[i]
    if (!entry || (entry.family !== "inet" && entry.family !== "inet6")) continue
    if (entry.tentative || entry.dadfailed || entry.valid_life_time === 0) continue
    var value = String(entry.local || "")
    if (!value || !/^[a-fA-F0-9:.]+$/.test(value)) continue
    var ipv6 = entry.family === "inet6"
    if (entry.scope === "host") continue
    if (ipv6 && entry.scope === "link") {
      if (!/^[a-zA-Z0-9_.:-]{1,15}$/.test(device)) continue
      value += "%" + device
    }
    if (seen[value]) continue
    seen[value] = true
    rows.push({label: ipv6 ? "IPv6" : "IPv4", value: value,
      detail: value + (Number.isInteger(entry.prefixlen) ? "/" + entry.prefixlen : "")})
  }
  return rows
}
