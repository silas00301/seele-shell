.import "../shared/Native.js" as Bridge

function duration(milliseconds) { return Bridge.call("notes.duration", [milliseconds]) }
function label(note) { return Bridge.call("notes.label", [note]) }
function lineStart(text, position) { return Bridge.call("notes.lineStart", [text, position]) }
function lineEnd(text, position) { return Bridge.call("notes.lineEnd", [text, position]) }
function wrap(text, start, end, marker) { return Bridge.call("notes.wrap", [text, start, end, marker]) }
function heading(text, position, level) { return Bridge.call("notes.heading", [text, position, level]) }
function task(text, position) { return Bridge.call("notes.task", [text, position]) }
function link(text, start, end) { return Bridge.call("notes.link", [text, start, end]) }
function newline(text, position) { return Bridge.call("notes.newline", [text, position]) }
function embed(text, position, name) { return Bridge.call("notes.embed", [text, position, name]) }
function unembed(text, name) { return Bridge.call("notes.unembed", [text, name]) }
function obsidianUri(vault, path) { return Bridge.call("notes.obsidianUri", [vault, path]) }
function filter(notes, query) {
  return Bridge.call("notes.filter", [notes, query]).map(function(index) { return notes[index] })
}
function when(seconds, now) { return Bridge.call("notes.when", Bridge.localDates(seconds, now)) }
function status(state, seconds, now) { return Bridge.call("notes.status", [state].concat(Bridge.localDates(seconds, now))) }
