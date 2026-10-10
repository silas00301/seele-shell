const {nativeBridge, source: nativeSource} = require("./native-functions.cjs");
const assert = require("node:assert/strict")
const fs = require("node:fs")
const vm = require("node:vm")

const helpers = vm.createContext({Bridge: nativeBridge()})
vm.runInContext(nativeSource(fs.readFileSync(process.argv[2], "utf8")), helpers)

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

const kinds = ["clip", "select", "window", "dir", "screen"]
const opened = helpers.completion("Ask @", "Ask @".length)
assert.equal(opened.active, true)
assert.equal(opened.query, "")
assert.deepEqual(opened.options.map(option => option.kind), kinds)
assert.deepEqual(opened.options.map(option => option.label), kinds.map(kind => "@" + kind))
assert.ok(opened.options.every(option => option.glyph && option.caption))
assert.equal(opened.options[0].caption, "Included once when you press Send")
assert.equal(opened.options[3].caption, "Resolved when you press Send")
const filtered = helpers.completion("Ask @sc", "Ask @sc".length)
assert.deepEqual(filtered.options.map(option => option.kind), ["screen"])
assert.equal(helpers.completion("Ask @clip", "Ask @clip".length).active, false)
assert.equal(helpers.completion("mail me@example.org", "mail me@example.org".length).active, false)
assert.equal(helpers.completion("quote @@clip", "quote @@clip".length).active, false)
assert.equal(helpers.completion("@cli", 2).active, false)
assert.equal(helpers.completion("Ask @clipboard", "Ask @clipboard".length).active, false)
assert.equal(helpers.completion("🙂 @", "🙂 @".length).active, true)
const spaced = helpers.applyCompletion("Explain @cl please", "Explain @cl".length, "clip")
assert.equal(spaced.applied, true)
assert.equal(spaced.text, "Explain @clip please")
assert.equal(spaced.cursor, "Explain @clip ".length)
const inserted = helpers.applyCompletion("Explain @", "Explain @".length, "screen")
assert.equal(inserted.text, "Explain @screen ")
assert.equal(inserted.cursor, inserted.text.length)
assert.equal(helpers.applyCompletion("@cl-next", 3, "clip").text, "@clip -next")
assert.deepEqual(helpers.applyCompletion("@cl\nmore", 3, "clip"), {text: "@clip\nmore", cursor: "@clip\n".length, applied: true})
const emoji = helpers.applyCompletion("🙂 @", "🙂 @".length, "dir")
assert.equal(emoji.text, "🙂 @dir ")
assert.equal(emoji.cursor, "🙂 @dir ".length)
const refused = helpers.applyCompletion("Explain @cl", "Explain @cl".length, "screen")
assert.equal(refused.applied, false)
assert.equal(refused.text, "Explain @cl")
assert.equal(helpers.completionKey(true, 16777237, false), "down")
assert.equal(helpers.completionKey(true, 16777235, false), "up")
assert.equal(helpers.completionKey(true, 16777220, false), "accept")
assert.equal(helpers.completionKey(true, 16777221, true), "passthrough")
assert.equal(helpers.completionKey(true, 16777217, false), "accept")
assert.equal(helpers.completionKey(true, 16777216, false), "dismiss")
assert.equal(helpers.completionKey(false, 16777216, false), "passthrough")
assert.equal(helpers.completionKey(false, 16777220, false), "passthrough")

const qml = fs.readFileSync(process.argv[3], "utf8")
const open = qml.match(/function open\(screen, window\) \{[\s\S]*?\n  \}/)[0]
assert.ok(open.indexOf("active = true") < open.indexOf('send({ command: "open"'),
  "the panel must map before its resident helper is contacted")

for (const [feature, pattern] of Object.entries({
  "model-free resident helper": /Process\s*\{\s*id: worker\s*command: \["seele-ai-prompt-worker"\]\s*running: true/,
  "active-output pin": /prompt\.screenName === modelData\.name/,
  "centered layer surface": /implicitWidth: Math\.min\(640,[\s\S]*WlrLayershell\.namespace: "seele-shell-prompt"/,
  "immediate keyboard focus": /WlrLayershell\.keyboardFocus: panelActive \? WlrKeyboardFocus\.Exclusive : WlrKeyboardFocus\.None/,
  "caret reclaimed when the compositor grants focus": /onActiveFocusChanged: if \(activeFocus && panelActive && !promptField\.activeFocus\) promptField\.forceActiveFocus\(\)/,
  "one-time clipboard permission": /Ai.has\(permissionContexts, "clip"\).*"Allow once"/,
  "exact screen preview": /this exact image will be sent/,
  "explicit copy shortcut": /text: "Copy · Enter"/,
  "explicit insert shortcut": /text: "Insert · Ctrl\+Enter"/,
  "private screen disclosure": /captured once when you press Send/,
  "stale context rejection": /!acceptsContext\(String\(message\.kind \|\| ""\), Number\(message\.token \|\| 0\)\)/,
  "answer-only response card": /label: "Answer"/,
  "mention completion list": /id: completionMenu/,
  "completion stays in the scroll": /height: 112 \+ \(prompt\.completionOpen \? completionMenu\.height \+ prompt\.theme\.spaceTight : 0\)/,
  "completion keeps the field focused": /onClicked: \{\n\s*prompt\.acceptMention\(completionRow\.modelData\.kind\)\n\s*promptField\.forceActiveFocus\(\)/,
})) assert.ok(pattern.test(qml), `${feature} is missing from AiPrompt.qml`)

const key = qml.match(/function key\(event\) \{[\s\S]*?\n  \}/)[0]
assert.ok(key.indexOf("completionKey") < key.indexOf("close()"), "Escape dismisses completion before it can close the panel")
assert.ok(key.includes('action === "accept"'), "Enter accepts the highlighted mention")
assert.ok(key.includes('action === "dismiss"'), "Escape dismisses the mention list")
assert.ok(/onCompletionIdentityChanged: completionIndex = 0/.test(qml), "a new query returns to the first match")

// The panel used to close itself as soon as it lost keyboard focus. It now
// holds the keyboard until it is dismissed by hand, so nothing may bring that
// focus-loss shortcut back.
assert.ok(!/sawFocus/.test(qml), "the prompt must not close itself on focus loss")
assert.ok(/function onActiveChanged\(\) \{\s*if \(promptWindow\.panelActive\) Qt\.callLater/.test(qml),
  "opening the panel must focus its field")

console.log("AI prompt mentions, permission gates, shortcuts, keyboard focus, and UI disclosure passed")

// Drive the production coordinator with deferred context replies.
const methods = [...qml.matchAll(/^  function \w+\([^\n]*\) \{\n[\s\S]*?^  \}/gm)].map(m => m[0]).join('\n')
const messages = []
const state = vm.createContext({
  Ai: helpers, generation: 0, request: 0, contextSerial: 0, alive: false,
  busy: false, collecting: false, permissionContexts: [],
  worker: { running: true, write(value) { messages.push(JSON.parse(value)) } },
  screenCaptureDelay: { stop() {}, restart() {} }, usageRefreshRequested() {}
})
state.prompt = state
vm.runInContext(methods, state)
let text = ''
Object.defineProperty(state, 'promptText', { get() { return text }, set(value) { text = value; state.syncContexts() } })
Object.defineProperty(state, 'mentionedContexts', { get() { return helpers.mentions(text) } })
Object.defineProperty(state, 'canSend', { get() { return !state.busy && !state.collecting && text.trim() !== '' } })
state.open('DP-1', {app:'terminal', title:'work'})
messages.length = 0
state.promptText = 'Explain @clip @select @dir @screen @window'
assert.equal(messages.filter(m => m.command === 'preview').length, 0, 'typing never reads context')
state.submit()
state.submit()
assert.equal(messages.filter(m => m.command === 'preview').length, 1)
assert.equal(messages.filter(m => m.command === 'submit').length, 0)
const reply = (kind, token) => state.accept({id:state.generation,event:'preview',kind,token,available:true,text:'private',preview:'preview',path:'/private/screen.png'})
reply('clip', state.clipToken)
reply('select', state.selectionToken)
reply('dir', state.directoryToken)
assert.equal(state.active, false, 'screen capture hides the panel')
assert.equal(messages.filter(m => m.command === 'submit').length, 0)
reply('screen', state.screenToken)
assert.equal(state.active, true)
assert.equal(messages.filter(m => m.command === 'submit').length, 1)
assert.deepEqual(messages.find(m => m.command === 'submit').permissions, ['clip','select'])
state.close()
state.open('DP-1', {})
state.promptText = 'Read @clip'
state.submit()
const stale = state.clipToken
state.promptText = 'Changed @clip'
reply('clip', stale)
assert.equal(state.collecting, false)
assert.equal(state.clipReady, false)
state.submit()
state.accept({id:state.generation,event:'context-error',kind:'clip',token:state.clipToken,message:'Clipboard unavailable'})
assert.equal(state.promptText, 'Changed @clip')
assert.equal(state.canSend, true)
assert.match(state.error, /@clip: Clipboard unavailable/)
state.submit()
const oldGeneration = state.generation, oldToken = state.clipToken
state.close()
state.open('DP-1', {})
state.accept({id:oldGeneration,event:'preview',kind:'clip',token:oldToken,available:true,text:'old'})
assert.equal(state.clipReady, false)
state.promptText = 'Find @dir'
state.submit()
state.accept({id:state.generation,event:'preview',kind:'dir',token:state.directoryToken,available:false})
assert.equal(state.canSend, true)
assert.match(state.error, /@dir/)
console.log('One-send collection, no pre-send reads, duplicate prevention, errors, edits and stale generations passed')

state.Qt = {ShiftModifier: 0x02000000, ControlModifier: 0x04000000}
state.completion = helpers.completion("Use @w", "Use @w".length)
state.completionOpen = true
state.completionIndex = 0
state.completionDismissed = ""
state.completionIdentity = "query"
state.promptCursor = "Use @w".length
state.promptText = "Use @w"
messages.length = 0
state.acceptMention("screen")
assert.equal(state.promptText, "Use @w")
assert.equal(messages.filter(message => message.command === "preview").length, 0)
state.acceptMention("window")
assert.equal(state.promptText, "Use @window ")
assert.equal(state.pendingCursor, "Use @window ".length)
assert.equal(messages.filter(message => message.command === "preview").length, 0)
assert.equal(messages.filter(message => message.command === "submit").length, 0)
state.completionOpen = true
state.completion = helpers.completion("Use @", "Use @".length)
state.completionIndex = 0
state.promptText = "Use @"
state.promptCursor = "Use @".length
state.key({key: 16777237, modifiers: 0, accepted: false})
assert.equal(state.completionIndex, 1)
state.key({key: 16777216, modifiers: 0, accepted: false})
assert.equal(state.completionDismissed, "query")
assert.equal(state.active, true)
state.completionDismissed = ""
state.completionOpen = true
messages.length = 0
state.key({key: 16777220, modifiers: 0, accepted: false})
assert.equal(state.promptText, "Use @select ")
assert.equal(messages.filter(message => message.command === "preview").length, 0)
assert.equal(messages.filter(message => message.command === "submit").length, 0)
console.log("Mention completion inserts the highlighted source without reading it")
