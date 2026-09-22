.import "../shared/Native.js" as Bridge

// The header of one preview: glyph, name, the line of facts and whether the
// panel can draw the file at all.
function summary(item) { return Bridge.call("quicklook.summary", [item]) }
// Moving between the highlighted files wraps; moving between pages does not.
function step(total, index, delta) { return Bridge.call("quicklook.step", [total, index, delta]) }
function page(pages, current, delta) { return Bridge.call("quicklook.page", [pages, current, delta]) }
// The footer names only the keys this preview actually answers.
function hint(item, total) { return Bridge.call("quicklook.hint", [item, total]) }
// A file:// URL Qt parses back to exactly this path, whatever the name holds.
function url(path) { return Bridge.call("quicklook.url", [path]) }
// Elapsed and total time for a sound or a moving picture.
function duration(milliseconds) { return Bridge.call("quicklook.duration", [milliseconds]) }
