.import "../shared/Native.js" as Bridge

function describe(sample, palette) { return Bridge.call("color_picker.describe", [sample, palette]) }
function cycle(entry, format) { return Bridge.call("color_picker.cycle", [entry, format]) }
function payload(entry, format) { return Bridge.call("color_picker.payload", [entry, format]) }
// The bound on how many picks are kept belongs to the policy, not here.
function history(entries, entry) { return Bridge.call("color_picker.history", [entries, entry]) }
function loupe(x, y, width, height, cells) { return Bridge.call("color_picker.loupe", [x, y, width, height, cells]) }
