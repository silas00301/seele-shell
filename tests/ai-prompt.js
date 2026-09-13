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

const qml = fs.readFileSync(process.argv[3], "utf8")
const open = qml.match(/function open\(screen, window\) \{[\s\S]*?\n  \}/)[0]
assert.ok(open.indexOf("active = true") < open.indexOf('send({ command: "open"'),
  "the panel must map before its resident helper is contacted")

for (const [feature, pattern] of Object.entries({
  "model-free resident helper": /Process\s*\{\s*id: worker\s*command: \["seele-ai-prompt-worker"\]\s*running: true/,
  "active-output pin": /prompt\.screenName === modelData\.name/,
  "centered layer surface": /implicitWidth: Math\.min\(640,[\s\S]*WlrLayershell\.namespace: "seele-shell-prompt"/,
  "focus-loss privacy": /else if \(sawFocus && panelActive && !prompt\.needsAction\) prompt\.close\(\)/,
  "one-time clipboard permission": /Ai.has\(permissionContexts, "clip"\).*"Allow once"/,
  "exact screen preview": /this exact image will be sent/,
  "explicit copy shortcut": /text: "Copy · Enter"/,
  "explicit insert shortcut": /text: "Insert · Ctrl\+Enter"/,
  "private screen disclosure": /captured once when you press Send/,
  "stale context rejection": /!acceptsContext\(String\(message\.kind \|\| ""\), Number\(message\.token \|\| 0\)\)/,
  "answer-only response card": /label: "ANSWER"/,
})) assert.ok(pattern.test(qml), `${feature} is missing from AiPrompt.qml`)

console.log("AI prompt mentions, permission gates, shortcuts, focus lifetime, and UI disclosure passed")

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
