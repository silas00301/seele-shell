.import "../shared/Native.js" as Bridge

function mentions(value) { return Bridge.call("ai_prompt.mentions", [value]) }
function has(values, kind) { return Bridge.call("ai_prompt.has", [values, kind]) }
function permissions(values, clipAllowed, selectionAllowed) { return Bridge.call("ai_prompt.permissions", [values, clipAllowed, selectionAllowed]) }
function canSubmit(value, values, clipReady, selectionReady, directoryReady, screenReady, busy) { return Bridge.call("ai_prompt.canSubmit", [value, values, clipReady, selectionReady, directoryReady, screenReady, busy]) }
function preview(value, characters, truncated) { return Bridge.call("ai_prompt.preview", [value, characters, truncated]) }
function completion(value, cursor) { return Bridge.call("ai_prompt.completion", [value, cursor]) }
function applyCompletion(value, cursor, kind) { return Bridge.call("ai_prompt.applyCompletion", [value, cursor, kind]) }
function completionKey(active, key, shift) { return Bridge.call("ai_prompt.completionKey", [active, key, shift]) }
