function initial(host) {
  return { state: "idle", message: "Open this panel to load pull requests.", host: host || "github.com", viewer: "", updatedAt: "", reviews: [], authored: [], reviewTotal: 0, authoredTotal: 0, stale: false }
}

function receive(current, next) {
  if (!next || ["ready", "error", "auth-required", "rate-limited"].indexOf(next.state) < 0)
    next = { state: "error", message: "GitHub returned an unreadable response." }
  if (next.state === "ready" && Array.isArray(next.reviews) && Array.isArray(next.authored)) {
    next.stale = false
    return next
  }
  var value = next.state === "auth-required" ? initial(current.host) : Object.assign({}, current)
  value.state = next.state === "ready" ? "error" : next.state
  value.message = next.message || "GitHub returned incomplete data. Try refreshing again."
  value.stale = value.updatedAt !== ""
  return value
}

function due(lastAttempt, now, state, manual) {
  return now - lastAttempt >= (manual ? 5000 : state === "rate-limited" ? 300000 : 60000)
}

function safeUrl(url, host) {
  if (typeof url !== "string" || typeof host !== "string") return false
  var prefix = "https://" + host + "/"
  return url.indexOf(prefix) === 0 && /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+\/pull\/[1-9][0-9]*$/.test(url.slice(prefix.length))
}

function checksLabel(state) {
  return ({ SUCCESS: "Checks passing", FAILURE: "Checks failing", ERROR: "Checks errored", PENDING: "Checks running", EXPECTED: "Checks expected" })[state] || "No check status"
}

function reviewLabel(item) {
  if (item.draft) return "Draft"
  return ({ APPROVED: "Approved", CHANGES_REQUESTED: "Changes requested", REVIEW_REQUIRED: "Review required" })[item.review] || "Open"
}
