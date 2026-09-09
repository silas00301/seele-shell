const assert = require("node:assert/strict")
const fs = require("node:fs")
const vm = require("node:vm")

const helpers = vm.createContext({})
vm.runInContext(fs.readFileSync(process.argv[2], "utf8"), helpers)

assert.deepEqual(Array.from(helpers.mentions("Ask @clip, @window, @clip and @screen.")), ["clip", "window", "screen"])
assert.deepEqual(Array.from(helpers.mentions("mail me@example.org and quote @@clip")), [])
assert.deepEqual(Array.from(helpers.permissions(["clip", "select"], true, false)), ["clip"])
assert.equal(helpers.canSubmit("question", [], false, false, false, false, false), true)
assert.equal(helpers.canSubmit("question @clip", ["clip"], false, false, false, false, false), false)
assert.equal(helpers.canSubmit("question @clip", ["clip"], true, false, false, false, false), true)
assert.equal(helpers.canSubmit("question @dir", ["dir"], false, false, false, false, false), false)
assert.equal(helpers.canSubmit("question @dir", ["dir"], false, false, true, false, false), true)
assert.equal(helpers.canSubmit("question @screen", ["screen"], false, false, false, false, false), false)
assert.equal(helpers.canSubmit("question @screen", ["screen"], false, false, false, true, false), true)
assert.equal(helpers.canSubmit("question", [], false, false, false, false, true), false)
assert.equal(helpers.preview("one\ntwo", 7, false), "one  ↵  two · 7 characters")
assert.match(helpers.preview("text", 65536, true), /first 65,536 characters/)

const qml = fs.readFileSync(process.argv[3], "utf8")
const open = qml.match(/function open\(screen, window\) \{[\s\S]*?\n  \}/)[0]
assert.ok(open.indexOf("active = true") < open.indexOf('send({ command: "open"'),
  "the panel must map before its resident helper is contacted")

for (const [feature, pattern] of Object.entries({
  "model-free resident helper": /Process\s*\{\s*id: worker\s*command: \["seele-ai-prompt-worker"\]\s*running: true/,
  "active-output pin": /prompt\.screenName === modelData\.name/,
  "centered layer surface": /implicitWidth: Math\.min\(640,[\s\S]*WlrLayershell\.namespace: "seele-shell-prompt"/,
  "focus-loss privacy": /else if \(sawFocus && panelActive && !prompt\.needsAction\) prompt\.close\(\)/,
  "one-time clipboard permission": /action: clipReady \? \(clipExpanded \? "Collapse" : "Review"\) : "Allow once"/,
  "exact screen preview": /this exact image will be sent/,
  "explicit copy shortcut": /text: "Copy · Enter"/,
  "explicit insert shortcut": /text: "Insert · Ctrl\+Enter"/,
  "private screen disclosure": /capture a private preview before sending/,
  "stale context rejection": /!acceptsContext\(String\(message\.kind \|\| ""\), Number\(message\.token \|\| 0\)\)/,
  "answer-only response card": /label: "ANSWER"/,
})) assert.ok(pattern.test(qml), `${feature} is missing from AiPrompt.qml`)

console.log("AI prompt mentions, permission gates, shortcuts, focus lifetime, and UI disclosure passed")
