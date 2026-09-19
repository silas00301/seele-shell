const {nativeBridge, source: nativeSource} = require("./native-functions.cjs");
const assert = require("node:assert/strict")
const fs = require("node:fs")
const vm = require("node:vm")
const context = vm.createContext({Bridge: nativeBridge()})
vm.runInContext(nativeSource(fs.readFileSync(process.argv[2], "utf8")), context)

// The shell's own block, named by role. `accent` is Lavender on this desktop,
// which is why the picker answers with a token rather than a Catppuccin name:
// these eleven are the only colour names the shell actually has.
const palette = {
  base: "#1e1e2e", mantle: "#181825", crust: "#11111b", surface: "#313244",
  overlay: "#6c7086", text: "#cdd6f4", subtext: "#a6adc8", accent: "#b4befe",
  red: "#f38ba8", green: "#a6e3a1", yellow: "#f9e2af",
}

const exact = context.describe({r: 0xb4, g: 0xbe, b: 0xfe}, palette)
assert.equal(exact.hex, "#b4befe")
assert.equal(exact.rgb, "rgb(180, 190, 254)")
assert.equal(exact.token, "accent")
assert.equal(exact.exact, true)
assert.equal(exact.note, "accent")

// One step off the token is still worth naming, but only with the distance
// attached and never as something the user can copy as a name.
const near = context.describe({r: 0xb5, g: 0xbf, b: 0xfd}, palette)
assert.equal(near.exact, false)
assert.equal(near.token, "accent")
assert.match(near.note, /^nearest accent · ΔE \d+\.\d$/)
assert.equal(context.payload(near, "token").format, "hex")
assert.equal(context.payload(near, "token").text, near.hex)
assert.equal(context.payload(exact, "token").text, "accent")
assert.equal(context.payload(exact, "rgb").text, "rgb(180, 190, 254)")

// A colour nothing in the palette is close to says nothing at all rather than
// naming whichever token happens to be least far away.
const far = context.describe({r: 0, g: 255, b: 0}, palette)
assert.equal(far.token, "")
assert.equal(far.note, "")
assert.equal(far.exact, false)

// Only a colour that is exactly a token offers the token format.
assert.equal(context.cycle(exact, "hex"), "rgb")
assert.equal(context.cycle(exact, "rgb"), "token")
assert.equal(context.cycle(exact, "token"), "hex")
assert.equal(context.cycle(far, "rgb"), "hex")
assert.equal(context.cycle(far, "token"), "hex")
assert.equal(context.cycle(null, "hex"), "rgb")

// The bridge has to return a real JavaScript array here, not a QVariant list:
// the history is rendered by a Repeater and indexed by the recall keys.
let history = []
for (const colour of [exact, near, far]) history = context.history(history, colour)
assert.ok(Array.isArray(history))
assert.deepEqual(history.map(entry => entry.hex), [far.hex, near.hex, exact.hex])
history = context.history(history, exact)
assert.deepEqual(history.map(entry => entry.hex), [exact.hex, far.hex, near.hex])
let filled = []
for (let index = 0; index < 20; index++) filled = context.history(filled, context.describe({r: index, g: 0, b: 0}, palette))
assert.equal(filled.length, 9, "the history must stay within single-digit recall")

const lens = context.loupe(0, 0, 1920, 1080, 9)
assert.deepEqual([lens.x, lens.y, lens.w, lens.h, lens.cx, lens.cy], [0, 0, 9, 9, 0, 0])
const edge = context.loupe(1, 1, 1920, 1080, 9)
assert.deepEqual([edge.w, edge.h, edge.cx, edge.cy], [9, 9, 8, 8])
assert.equal(edge.x + edge.w, 1920)
console.log("colour naming, honest near matches, format offers, history bounds and lens geometry passed")

// Exercise the actual QML controller functions with fake process IO. This
// catches stale samples, coalescing and commit races without a compositor.
const controller = fs.readFileSync(process.argv[3], "utf8")
const methods = [...controller.matchAll(/^  function \w+\([^\n]*\) \{\n[\s\S]*?^  \}/gm)].map(m => m[0]).join("\n")
const sent = []
const state = vm.createContext({
  Colors: context,
  palette, active: false, presented: false, generation: 0, frames: [], loaded: {},
  output: "", pointX: 0.5, pointY: 0.5, asked: 0, answered: 0, waiting: false, trailing: false,
  entry: null, payloadText: "", format: "hex", history: [], copying: "", copyFormat: "",
  copyingNow: false, error: "", notice: "",
  clipboard: {payload: "", running: false, stdinEnabled: false},
  worker: {running: true, write(message) { sent.push(JSON.parse(message)) }},
  watchdog: {running: false, restart() { this.running = true }, stop() { this.running = false }},
  noticeTimer: {running: false, restart() { this.running = true }, stop() { this.running = false }},
  Quickshell: {screens: [{name: "DP-1"}, {name: "DP-2"}]},
  Qt: {
    Key_Escape: 0x01000000, Key_Tab: 0x01000001, Key_Return: 0x01000004, Key_Enter: 0x01000005,
    Key_Left: 0x01000012, Key_Up: 0x01000013, Key_Right: 0x01000014, Key_Down: 0x01000015,
    Key_1: 0x31, Key_9: 0x39, Key_H: 0x48, Key_J: 0x4a, Key_K: 0x4b, Key_L: 0x4c,
    ShiftModifier: 0x02000000, ControlModifier: 0x04000000,
    AltModifier: 0x08000000, MetaModifier: 0x10000000,
  },
})
state.picker = state
vm.runInContext(methods, state)
const press = (key, modifiers = 0, isAutoRepeat = false) => state.key({key, modifiers, isAutoRepeat, accepted: false})
const last = command => [...sent].reverse().find(message => message.command === command)
const freeze = () => {
  state.open()
  state.accept({id: state.generation, event: "frames",
    frames: [{output: "DP-1", path: "/run/a.ppm", width: 100, height: 50},
      {output: "DP-2", path: "/run/b.ppm", width: 100, height: 50}]})
  state.imageReady("DP-1")
  state.imageReady("DP-2")
}

state.open()
assert.equal(last("capture").outputs.length, 2)
assert.equal(state.watchdog.running, true)
state.accept({id: state.generation, event: "frames", frames: [{output: "DP-1", path: "/run/a.ppm", width: 100, height: 50}]})
state.imageReady("DP-1")
assert.equal(state.presented, true)
assert.equal(state.watchdog.running, false, "the watchdog guards freezing, not aiming")

// One sample in flight at a time, with the newest point coalesced behind it.
state.aim("DP-1", 0.25, 0.25)
assert.equal(state.waiting, true)
const first = last("sample").token
state.aim("DP-1", 0.75, 0.75)
assert.equal(last("sample").token, first, "a second point must wait for the first answer")
assert.equal(state.trailing, true)
state.accept({id: state.generation, event: "sample", token: first, commit: false, output: "DP-1", r: 0xb4, g: 0xbe, b: 0xfe})
assert.equal(state.entry.token, "accent")
assert.equal(state.payloadText, "#b4befe")
assert.equal(last("sample").token, first + 1, "the trailing point is asked for once the answer lands")
assert.equal(last("sample").x, 0.75)

// An answer for a point the pointer has already left is dropped, not drawn.
state.accept({id: state.generation, event: "sample", token: first, commit: false, output: "DP-1", r: 0, g: 255, b: 0})
assert.equal(state.entry.hex, "#b4befe")
state.accept({id: 9999, event: "sample", token: 99, commit: false, output: "DP-1", r: 0, g: 255, b: 0})
assert.equal(state.entry.hex, "#b4befe")

// Tab moves onto the token only where the colour is exactly one.
press(state.Qt.Key_Tab)
assert.equal(state.format, "rgb")
assert.equal(state.payloadText, "rgb(180, 190, 254)")
press(state.Qt.Key_Tab)
assert.equal(state.format, "token")
assert.equal(state.payloadText, "accent")
state.accept({id: state.generation, event: "sample", token: state.asked, commit: false, output: "DP-1", r: 0x12, g: 0x34, b: 0x56})
assert.equal(state.format, "hex", "a colour that is not a token cannot keep the token format")
assert.equal(state.payloadText, "#123456")

// Keys walk the point by whole captured pixels without moving the pointer.
state.aim("DP-1", 0.5, 0.5)
const walked = state.pointX
press(state.Qt.Key_L)
assert.ok(Math.abs(state.pointX - (walked + 1 / 100)) < 1e-9)
press(state.Qt.Key_H, state.Qt.ShiftModifier)
assert.ok(Math.abs(state.pointX - (walked - 9 / 100)) < 1e-9)
press(state.Qt.Key_J)
assert.ok(Math.abs(state.pointY - (0.5 + 1 / 50)) < 1e-9)
state.aim("DP-1", 0, 0)
press(state.Qt.Key_Up)
assert.equal(state.pointY, 0, "the point cannot walk off its own output")

// Enter re-samples the exact point rather than copying the throttled readout.
const before = sent.length
press(state.Qt.Key_Return)
assert.equal(last("sample").commit, true)
assert.equal(sent.length, before + 1)
state.accept({id: state.generation, event: "sample", token: state.asked, commit: true, output: "DP-1", r: 0xa6, g: 0xe3, b: 0xa1})
assert.equal(state.active, false, "a pick releases the frozen screens")
assert.equal(state.clipboard.payload, "#a6e3a1")
assert.equal(state.clipboard.stdinEnabled, true)
assert.equal(state.copyingNow, true, "the card stays up while wl-copy runs")
assert.equal(state.copyFormat, "hex")
assert.equal(state.history[0].token, "green")
assert.equal(state.entry.hex, "#a6e3a1", "the result card shows what was picked")
assert.equal(last("cancel").id, state.generation)
console.log("colour coalescing, stale answers, format resolution, pixel walking and commit passed")

// Recall copies an older pick without re-picking it or reordering the history.
freeze()
assert.equal(state.history.length, 1)
const kept = state.history[0].hex
state.clipboard.payload = ""
press(state.Qt.Key_1, state.Qt.ControlModifier)
assert.equal(state.clipboard.payload, kept)
assert.equal(state.active, true, "recalling must not end the pick in progress")
assert.equal(state.history.length, 1)
press(state.Qt.Key_9, state.Qt.ControlModifier)
assert.equal(state.clipboard.payload, kept, "an empty slot copies nothing")
press(state.Qt.Key_Escape)
assert.equal(state.active, false)
assert.equal(state.presented, false)
assert.equal(state.frames.length, 0)

// A failure releases the screen immediately and says so for five seconds.
freeze()
state.fail("Screen colour unavailable")
assert.equal(state.active, false)
assert.equal(state.notice, "Screen colour unavailable")
assert.equal(state.noticeTimer.running, true)
state.accept({id: state.generation, event: "sample", token: 1, commit: true, output: "DP-1", r: 1, g: 2, b: 3})
assert.notEqual(state.clipboard.payload, "#010203", "a failed picker must not still copy")
state.open()
assert.equal(state.notice, "")
assert.equal(state.error, "")
press(state.Qt.Key_Escape)

// Session history and the chosen format survive the picker closing entirely.
assert.ok(state.history.length > 0)
assert.match(controller, /command: \["seele-color-worker"\]/)
assert.match(controller, /id: noticeTimer\s+interval: 5000/)
assert.match(controller, /id: watchdog\s+interval: 15000/)
assert.match(controller, /picker\.copyingNow = false/)
assert.ok(!/FileView|atomic|writeFile/.test(controller), "picked colours are session memory and never touch disk")
console.log("colour recall, dismissal, failure notices and session memory passed")
