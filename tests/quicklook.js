const {nativeBridge, source: nativeSource} = require("./native-functions.cjs");
const assert = require("node:assert/strict")
const fs = require("node:fs")
const vm = require("node:vm")
const context = vm.createContext({Bridge: nativeBridge()})
vm.runInContext(nativeSource(fs.readFileSync(process.argv[2], "utf8")), context)

// The adapter is thin on purpose; what is proven here is that each production
// call reaches the native policy and comes back as the value QML binds to.
assert.equal(context.duration(83000), "1:23")
assert.equal(context.url("/home/user/a b.png"), "file:///home/user/a%20b.png")
assert.equal(context.step(3, 2, 1), 0)
assert.equal(context.page(7, 7, 1), 7)
const header = context.summary({kind: "pdf", name: "a.pdf", size: 2048, pages: 7, error: ""})
assert.equal(header.drawable, true)
assert.equal(header.detail, "PDF document · PDF · 7 pages · 2.0 KiB")
assert.match(context.hint({kind: "pdf", name: "a.pdf", pages: 7, error: ""}, 3), /between pages/)
console.log("Quick Look presentation reaches the native policy through the production adapter")

// Exercise the actual QML controller with fake process, clipboard and
// compositor IO, so stale replies and key routing are caught without a display.
const controller = fs.readFileSync(process.argv[3], "utf8")
const media = fs.readFileSync(process.argv[4], "utf8")

// Its own layer surface, named so the compositor's blur rule can find it, and
// exclusive only while it is open: a mapped-but-closed surface that kept the
// keyboard would take every keystroke on the desktop.
assert.match(controller, /WlrLayershell\.namespace: "seele-shell-quicklook"/)
assert.match(controller,
  /WlrLayershell\.keyboardFocus: panelActive \? WlrKeyboardFocus\.Exclusive : WlrKeyboardFocus\.None/)
// There is no editable control in this panel. That is what makes Space its
// dismissal rather than a shortcut competing with typing.
assert.ok(!/TextField|TextInput|TextEdit|TextArea/.test(controller),
  "Quick Look must contain no editable control while Space dismisses it")
assert.match(media, /signal finished\(\)/)
assert.match(media, /EndOfMedia\) media\.finished\(\)/)
assert.match(media, /Keys\.onPressed: event => \{[\s\S]*event\.accepted = true[\s\S]*player\.setPosition/)
assert.match(controller, /function onFinished\(\) \{ preview\.mediaPlaying = false \}/)
const methods = [...controller.matchAll(/^  function \w+\([^\n]*\) \{\n[\s\S]*?^  \}/gm)].map(m => m[0]).join("\n")
const sent = []
const launched = []
const state = vm.createContext({
  QuickLook: context,
  active: false, screenName: "", generation: 0, paths: [], items: [], index: 0,
  pageNumber: 1, pagePath: "", pageError: "", pageLoading: false,
  loading: false, error: "", mediaPlaying: false,
  theme: {rowHeight: 40},
  clipboard: {payload: "", running: false, stdinEnabled: false},
  worker: {running: true, write(message) { sent.push(JSON.parse(message)) }},
  watchdog: {running: false, restart() { this.running = true }, stop() { this.running = false }},
  Quickshell: {execDetached(argv) { launched.push(Array.from(argv)) }},
  Qt: {
    Key_Escape: 0x01000000, Key_Space: 0x20, Key_Return: 0x01000004, Key_Enter: 0x01000005,
    Key_Left: 0x01000012, Key_Up: 0x01000013, Key_Right: 0x01000014, Key_Down: 0x01000015,
    Key_PageUp: 0x01000016, Key_PageDown: 0x01000017, Key_Home: 0x01000010, Key_End: 0x01000011,
    Key_C: 0x43, Key_H: 0x48, Key_J: 0x4a, Key_K: 0x4b, Key_L: 0x4c, Key_P: 0x50,
    ControlModifier: 0x04000000, AltModifier: 0x08000000, MetaModifier: 0x10000000,
  },
})
state.preview = state
Object.defineProperty(state, "item", {
  get() { return this.index >= 0 && this.index < this.items.length ? this.items[this.index] : null },
})
vm.runInContext(methods, state)

const ctrl = state.Qt.ControlModifier
const press = (key, modifiers = 0, body = null) => {
  const event = {key, modifiers, isAutoRepeat: false, accepted: false}
  state.key(event, body)
  // Every key the panel sees belongs to the panel: it holds the keyboard, so
  // nothing may fall through to whatever is underneath it.
  assert.equal(event.accepted, true)
  return event
}
const last = () => sent[sent.length - 1]

// An empty request is not a preview, and neither is a list of empty strings.
state.open("DP-1", [])
assert.equal(state.active, false)
state.open("DP-1", ["", null])
assert.equal(state.active, false)
assert.equal(sent.length, 0)

state.open("DP-1", ["/files/a.txt", "", "/files/b.pdf"])
assert.equal(state.active, true)
assert.equal(state.screenName, "DP-1")
assert.equal(state.loading, true)
assert.equal(state.watchdog.running, true)
assert.deepEqual(last(), {command: "open", id: state.generation, paths: ["/files/a.txt", "/files/b.pdf"]})

// A reply from a superseded request is never drawn over the current one.
state.accept({id: state.generation - 1, event: "items", items: [{kind: "text", name: "old"}]})
assert.equal(state.items.length, 0)

const text = {kind: "text", name: "a.txt", path: "/files/a.txt", size: 12, text: "one\ntwo\n", error: ""}
const document = {kind: "pdf", name: "b.pdf", path: "/files/b.pdf", size: 4096, pages: 7, error: ""}
state.accept({id: state.generation, event: "items", items: [text, document]})
assert.equal(state.loading, false)
assert.equal(state.watchdog.running, false)
assert.equal(state.index, 0)
// A text preview asks for nothing further; the reply already carried it.
assert.equal(last().command, "open")

// Moving to the document asks for its first page, and only its first page.
press(state.Qt.Key_Right)
assert.equal(state.index, 1)
assert.deepEqual(last(), {command: "page", id: state.generation, index: 1, page: 1})
assert.equal(state.pageLoading, true)

// A page for another file, or for a page already left behind, is not the
// picture in front of the reader.
state.accept({id: state.generation, event: "page", index: 0, page: 1, path: "/run/wrong.png", error: ""})
assert.equal(state.pagePath, "")
state.accept({id: state.generation, event: "page", index: 1, page: 4, path: "/run/wrong.png", error: ""})
assert.equal(state.pagePath, "")
state.accept({id: state.generation, event: "page", index: 1, page: 1, path: "/run/p1.png", error: ""})
assert.equal(state.pagePath, "/run/p1.png")
assert.equal(state.pageLoading, false)

// Pages stop at the document's own ends; files wrap around the chosen set.
press(state.Qt.Key_Down)
assert.equal(state.pageNumber, 2)
press(state.Qt.Key_End)
assert.equal(state.pageNumber, 7)
press(state.Qt.Key_Down)
assert.equal(state.pageNumber, 7)
press(state.Qt.Key_Right)
assert.equal(state.index, 0)
press(state.Qt.Key_Left)
assert.equal(state.index, 1)
// Leaving a page releases the picture rather than showing the last one drawn.
assert.equal(state.pagePath, "")

// A text preview scrolls under the same keys a document pages with, and a
// body that cannot scroll is simply left alone.
press(state.Qt.Key_Left)
assert.equal(state.index, 0)
const body = {contentHeight: 1000, height: 200, contentY: 0}
press(state.Qt.Key_Down, 0, body)
assert.equal(body.contentY, 40)
press(state.Qt.Key_PageDown, 0, body)
assert.equal(body.contentY, 240)
press(state.Qt.Key_End, 0, body)
assert.equal(body.contentY, 800)
press(state.Qt.Key_Home, 0, body)
assert.equal(body.contentY, 0)
press(state.Qt.Key_Down, 0, {})
press(state.Qt.Key_Down, 0, null)

// Playback is explicit, belongs only to a file that has any, and never
// survives a move to the next one.
press(state.Qt.Key_P)
assert.equal(state.mediaPlaying, false)
state.items = [{kind: "video", name: "c.mp4", path: "/files/c.mp4", error: ""}, text]
state.index = 0
press(state.Qt.Key_P)
assert.equal(state.mediaPlaying, true)
press(state.Qt.Key_Right)
assert.equal(state.mediaPlaying, false)

// The path is copied through stdin, never as a command argument.
state.index = 0
press(state.Qt.Key_C, ctrl)
assert.equal(state.clipboard.payload, "/files/c.mp4")
assert.equal(state.clipboard.stdinEnabled, true)
assert.equal(state.active, true)

// Handing a file to its own application closes the preview with it.
press(state.Qt.Key_Return)
assert.deepEqual(launched.at(-1), ["xdg-open", "/files/c.mp4"])
assert.equal(state.active, false)
assert.equal(last().command, "cancel")

// Space dismisses exactly as it opened: there is no editable control in this
// panel for it to belong to instead.
state.open("DP-1", ["/files/a.txt"])
state.accept({id: state.generation, event: "items", items: [text]})
press(state.Qt.Key_Space)
assert.equal(state.active, false)
assert.deepEqual(last(), {command: "cancel", id: state.generation})

state.open("DP-1", ["/files/a.txt"])
press(state.Qt.Key_Escape)
assert.equal(state.active, false)

// An item that cannot be described is still shown, and still refuses to be
// handed to an application.
state.open("DP-1", ["/files/gone.txt"])
state.accept({id: state.generation, event: "items", items: [
  {kind: "unavailable", name: "gone.txt", path: "/files/gone.txt", error: "This file is no longer there"},
]})
const before = launched.length
press(state.Qt.Key_Return)
assert.equal(launched.length, before)
assert.equal(state.active, true)

// A worker that answers nothing at all becomes a stated failure, not a
// permanently empty panel holding the keyboard.
state.fail("Reading this file timed out")
assert.equal(state.error, "Reading this file timed out")
assert.equal(state.loading, false)
console.log("Quick Look request generations, key routing, playback and dismissal passed")
