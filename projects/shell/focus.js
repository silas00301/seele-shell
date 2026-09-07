// Deadlines include suspended time; no timer state is written to disk.
function initial() {
  return { status: "idle", duration: 25 * 60, remaining: 25 * 60, deadline: 0 }
}

function valid(state) {
  return state && ["idle", "running", "paused", "done"].indexOf(state.status) >= 0
    && Number.isFinite(state.duration) && state.duration >= 1 && state.duration <= 14400
    && Number.isFinite(state.remaining) && state.remaining >= 0 && state.remaining <= state.duration
    && Number.isFinite(state.deadline) && state.deadline >= 0
}

function update(saved, action, now, minutes) {
  var state = valid(saved) ? saved : initial()
  now = Number(now)
  if (!Number.isFinite(now) || now < 0) return state
  var remaining = state.status === "running"
    ? Math.min(state.duration, Math.max(0, Math.ceil((state.deadline - now) / 1000)))
    : state.remaining
  if (action === "start") {
    var duration = Number(minutes) * 60
    if (!Number.isFinite(duration) || duration < 1 || duration > 14400) return state
    duration = Math.round(duration)
    return { status: "running", duration: duration, remaining: duration, deadline: now + duration * 1000 }
  }
  if (action === "cancel") return initial()
  if (state.status === "running" && remaining === 0)
    return { status: "done", duration: state.duration, remaining: 0, deadline: 0 }
  if (action === "pause" && state.status === "running")
    return { status: "paused", duration: state.duration, remaining: remaining, deadline: 0 }
  if (action === "resume" && state.status === "paused")
    return { status: "running", duration: state.duration, remaining: remaining, deadline: now + remaining * 1000 }
  if (state.status === "running" && state.remaining !== remaining)
    return { status: "running", duration: state.duration, remaining: remaining, deadline: state.deadline }
  return state
}

function label(seconds) {
  seconds = Math.max(0, Math.ceil(Number(seconds) || 0))
  return String(Math.floor(seconds / 60)).padStart(2, "0") + ":" + String(seconds % 60).padStart(2, "0")
}
