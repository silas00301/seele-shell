const assert = require("node:assert/strict")
const fs = require("node:fs")
const vm = require("node:vm")
const context = vm.createContext({})
vm.runInContext(fs.readFileSync(process.argv[2], "utf8"), context)

const links = Array.from({ length: 125 }, (_, index) => ({ number: index + 1, uri: "https://example.org/" + (index + 1) }))
assert.equal(context.selection(links, "1", true, false), null)
assert.equal(context.selection(links, "1", true, true).number, 1)
assert.equal(context.selection(links, "10", true, false), null)
assert.equal(context.selection(links, "10", true, true).number, 10)
assert.equal(context.selection(links, "125", true, false).number, 125)
assert.equal(context.selection(links, "9", true, false), null)
assert.equal(context.selection(links.slice(0, 9), "9", true, false).number, 9)
assert.equal(context.selection(links.slice(0, 9), "9", false, true).number, 9)
assert.equal(context.selection(links.slice(0, 9), "9", false, false), null)
assert.equal(context.selection(links, "0", true, true), null)
assert.equal(context.selection(links, "126", true, true), null)
assert.equal(context.selection(links, "", true, true), null)

const positions = [
  { number: 1, output: "DP-1", x0: 0, y0: 0, w: 0.15, h: 0.02 },
  { number: 2, output: "DP-1", x0: 0.2, y0: 0, w: 0.15, h: 0.02 },
  { number: 3, output: "DP-1", x0: 0.85, y0: 0.98, w: 0.15, h: 0.02 },
  { number: 4, output: "DP-2", x0: 0.85, y0: 0.98, w: 0.15, h: 0.02 }
]
for (const [width, height] of [[1920, 1080], [1280, 720], [1080, 1920]]) {
  const layout = context.layout(positions, "DP-1", width, height, 32, 24, 4)
  assert.equal(Object.keys(layout).length, 3)
  for (const box of Object.values(layout)) {
    assert.ok(box.x >= 0 && box.x + box.w <= width)
    assert.ok(box.y >= 0 && box.y + box.h <= height)
  }
  assert.equal(context.overlap(layout[1], layout[2]), 0)
}
console.log("URI number selection and mixed-output badge layout passed")

// Exercise the actual QML controller functions with fake process/display IO.
// This catches stale OCR responses and launch races without opening a browser.
const controller = fs.readFileSync(process.argv[3], "utf8")
const methods = [...controller.matchAll(/^  function \w+\([^\n]*\) \{\n[\s\S]*?^  \}/gm)].map(m => m[0]).join("\n")
const sent = []
const launched = []
const model = { items: [], append(link) { this.items.push(link) }, clear() { this.items = [] } }
const state = vm.createContext({
  Uris: context,
  active: false, presented: false, complete: false, generation: 0,
  digits: "", confirmPending: false, error: "", hoveredUri: "", failedAreas: 0,
  frames: [], allLinks: [], loaded: {}, linkModel: model,
  worker: { running: true, write(message) { sent.push(JSON.parse(message)) } },
  watchdog: { restart() {}, stop() {} },
  Quickshell: { screens: [{ name: "DP-1" }, { name: "DP-2" }], execDetached(argv) { launched.push(Array.from(argv)) } },
  Qt: { Key_Escape: 27, Key_Backspace: 8, Key_Return: 13, Key_Enter: 14,
    Key_0: 48, Key_9: 57, ControlModifier: 0x04000000, AltModifier: 0x08000000, MetaModifier: 0x10000000 }
})
state.picker = state
vm.runInContext(methods, state)
const press = key => state.key({ key, modifiers: 0, isAutoRepeat: false, accepted: false })
state.open()
assert.equal(sent.at(-1).command, "capture")
assert.equal(sent.at(-1).outputs.length, 2)
const firstId = state.generation
state.accept({ id: firstId - 1, event: "links", links: [{ number: 1, uri: "https://stale.example" }] })
assert.equal(model.items.length, 0)
state.accept({ id: firstId, event: "frames", frames: [{ output: "DP-1" }, { output: "DP-2" }] })
state.imageReady("DP-1")
assert.equal(state.presented, false)
state.imageReady("DP-2")
assert.equal(state.presented, true)
const literal = "https://example.org/a?x=$(id)&q='test';suffix"
state.accept({ id: firstId, event: "links", links: [{ number: 1, uri: literal }, { number: 11, uri: "https://example.org/11" }] })
assert.equal(state.linkModel, model)
press(49)
assert.equal(state.active, true)
press(13) // Explicit Enter can open an existing number while scanning.
assert.deepEqual(launched.at(-1), ["xdg-open", literal])
assert.equal(state.active, false)
assert.equal(state.frames.length, 0)
assert.equal(model.items.length, 0)
assert.equal(sent.at(-1).command, "cancel")
state.accept({ id: firstId, event: "done", failedAreas: 0 })
assert.equal(state.active, false)

state.open()
state.accept({ id: firstId, event: "frames", frames: [{ output: "DP-1" }] })
assert.equal(state.frames.length, 0)
press(49)
press(49)
state.accept({ id: state.generation, event: "links", links: [{ number: 1, uri: literal }, { number: 11, uri: "https://example.org/11" }] })
assert.equal(state.active, true)
state.accept({ id: state.generation, event: "done", failedAreas: 0 })
assert.deepEqual(launched.at(-1), ["xdg-open", "https://example.org/11"])
assert.equal(state.active, false)

state.open()
press(49)
press(50)
press(8)
assert.equal(state.digits, "1")
state.fail("capture failed")
const launchCount = launched.length
state.accept({ id: state.generation, event: "links", links: [{ number: 1, uri: literal }] })
state.accept({ id: state.generation, event: "done", failedAreas: 0 })
assert.equal(launched.length, launchCount)
press(27)
assert.equal(state.active, false)
console.log("URI controller capture barriers, stale generations, buffered input and argv launches passed")
