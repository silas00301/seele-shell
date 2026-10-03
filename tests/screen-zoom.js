// Runs the shell's actual showZoom callback against the messages
// `seele-shellctl zoom` sends, without starting a desktop shell.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

const source = fs.readFileSync(process.argv[2], "utf8");
const start = source.indexOf("  function showZoom(state) {");
assert(start >= 0, "showZoom is missing from shell.qml");
let end = source.indexOf("{", start) + 1;
for (let depth = 1; depth; end++) {
  if (source[end] === "{") depth++;
  if (source[end] === "}") depth--;
}

const shown = [];
let timerStops = 0;
const root = {
  osdOpen: false,
  osdKind: "volume",
  zoomOsd: { label: "1×", ratio: 0 },
  showTimedOsd(kind) {
    shown.push(kind);
    root.osdKind = kind;
    root.osdOpen = true;
  },
};
const context = vm.createContext({ root, osdTimer: { stop: () => timerStops++ }, JSON, Math });
vm.runInContext(source.slice(start, end), context);
const message = value => context.showZoom(JSON.stringify(value));

message({ factor: 2.0, label: "2×", ratio: 1 / 3, zoomed: true });
assert.deepEqual(shown, ["zoom"]);
assert.equal(root.zoomOsd.label, "2×");
assert.equal(root.zoomOsd.ratio, 1 / 3);

// Back at 1x the zoom strip goes at once rather than counting down.
message({ factor: 1.0, label: "1×", ratio: 0, zoomed: false });
assert.equal(root.osdOpen, false);
assert.equal(timerStops, 1);
assert.deepEqual(shown, ["zoom"], "1x never opens the OSD");

// A reset while another kind is on screen leaves that kind alone.
root.osdKind = "volume";
root.osdOpen = true;
message({ factor: 1.0, label: "1×", ratio: 0, zoomed: false });
assert.equal(root.osdOpen, true);
assert.equal(timerStops, 1);

// Malformed messages change nothing, and the meter cannot overfill.
for (const bad of ["", "{", "null", JSON.stringify({ zoomed: true, label: 2, ratio: 0.5 })]) {
  context.showZoom(bad);
}
assert.deepEqual(shown, ["zoom"]);
message({ factor: 9, label: "9×", ratio: 1.5, zoomed: true });
assert.equal(root.zoomOsd.ratio, 1);

console.log("screen zoom OSD callback ok");
