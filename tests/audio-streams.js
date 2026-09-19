const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

// Exercise the Audio panel's actual application-mixer callbacks without
// starting a second desktop shell, the way the other production-QML checks do.
const source = fs.readFileSync(process.argv[2], "utf8");
const start = source.indexOf("  function applicationStreams() {");
const end = source.indexOf("  function activeAgents() {", start);
assert(start >= 0 && end > start, "the application mixer helpers are in production shell.qml");

let timerRunning = false;
const controls = [];
const patches = [];
const root = {
  audioTrackMaximum: 100,
  audioWheelStep: 5,
  audioWheelSteps: wheel => wheel.steps,
  streamDragId: "",
  streamDragValue: -1,
  systemData: { audioStreams: [] },
  runControl: (action, value, extra) => {
    controls.push([action, value, extra]);
    return !root.controlBusy;
  },
  controlBusy: false,
  patchSystemData: patch => {
    patches.push(patch);
    Object.assign(root.systemData, patch);
  },
};
const streamDragTimer = {
  get running() { return timerRunning; },
  restart: () => { timerRunning = true; },
  stop: () => { timerRunning = false; },
};
const Quickshell = { iconPath: (icon, check) => (check && icon === "missing" ? "" : "/icons/" + icon + ".svg") };
const context = vm.createContext({ root, streamDragTimer, Quickshell, Math, Number, String });
vm.runInContext(source.slice(start, end), context);
for (const name of ["applicationStreams", "streamVolume", "streamLevel", "dragStreamVolume",
  "commitStreamVolume", "toggleStreamMute", "adjustStreamFromWheel", "releaseStreamDrag",
  "releaseFailedStreamDrag", "streamIcon"]) {
  assert.equal(typeof context[name], "function", `${name} is a production function`);
  root[name] = context[name];
}

const zen = { id: 7, name: "Zen Browser", detail: "A song", icon: "zen", volume: 40, muted: false, playing: true };
const mpv = { id: 9, name: "mpv", detail: "", icon: "", volume: 80, muted: false, playing: false };
root.systemData.audioStreams = [zen, mpv];

// An absent field is an empty group rather than an error: the panel is drawn
// before the first status frame arrives.
root.systemData.audioStreams = undefined;
assert.equal(root.applicationStreams().length, 0);
root.systemData.audioStreams = [zen, mpv];
assert.equal(root.applicationStreams().length, 2);

// A level under the pointer is shown on the row being dragged and on no other.
assert.equal(root.streamVolume(zen), 40);
root.dragStreamVolume(7, 65);
assert.equal(root.streamVolume(zen), 65);
assert.equal(root.streamVolume(mpv), 80, "a drag on one row never moves another row's level");
assert.equal(timerRunning, true, "a drag schedules its coalesced write");
assert.deepEqual(controls, [], "dragging does not run a control per pointer event");

// The drag maps across the track's own 100, never past it.
root.dragStreamVolume(7, 400);
assert.equal(root.streamVolume(zen), 100);
root.dragStreamVolume(7, -30);
assert.equal(root.streamVolume(zen), 0);

root.commitStreamVolume(7, 55);
assert.deepEqual(controls.pop(), ["stream-volume", "7", "55"]);
assert.equal(timerRunning, false, "a committed level stops the coalescing timer");
assert.equal(root.streamVolume(zen), 55, "the committed level is held until the graph agrees");

// The wheel takes the same notch the master levels take, and stops at the top
// of the track: the boost above full belongs to the output, not to one
// application stacked on top of it.
root.releaseStreamDrag([{ id: 7, volume: 55 }]);
zen.volume = 55;
root.adjustStreamFromWheel({ steps: 1 }, zen);
assert.deepEqual(controls.pop(), ["stream-volume", "7", "60"]);
root.adjustStreamFromWheel({ steps: 40 }, zen);
assert.deepEqual(controls.pop(), ["stream-volume", "7", "100"], "an application is never amplified past full");
root.adjustStreamFromWheel({ steps: 0 }, zen);
root.adjustStreamFromWheel({ steps: 1 }, null);
assert.deepEqual(controls, [], "an empty notch and an absent stream write nothing");
root.releaseStreamDrag([{ id: 7, volume: 100 }]);

// A refused control (another action is already running) has to come back.
root.controlBusy = true;
root.commitStreamVolume(7, 30);
assert.equal(timerRunning, true, "a refused write is retried rather than dropped");
root.controlBusy = false;
controls.length = 0;

// The graph agreeing with the latched value is what releases it. A value that
// is not yet the one asked for leaves the row where the pointer left it.
root.dragStreamVolume(7, 55);
root.releaseStreamDrag([{ id: 7, volume: 40 }, { id: 9, volume: 80 }]);
assert.equal(root.streamDragValue, 55, "an older graph value does not release the latch");
root.releaseStreamDrag([{ id: 7, volume: 55 }]);
assert.equal(root.streamDragId, "");
assert.equal(root.streamDragValue, -1);
// A stream that leaves mid-drag releases it too, or the panel keeps showing a
// level that belongs to nothing.
root.dragStreamVolume(9, 20);
root.releaseStreamDrag([{ id: 7, volume: 55 }]);
assert.equal(root.streamDragValue, -1, "a stream that disappears mid-drag releases the latch");

// A failed older write must not discard a newer pointer value. Only the exact
// request that failed may release the optimistic latch.
root.dragStreamVolume(7, 45);
root.releaseFailedStreamDrag("7", "40");
assert.equal(root.streamDragValue, 45, "an older failed write preserves the newer drag value");
root.releaseFailedStreamDrag("7", "45");
assert.equal(root.streamDragValue, -1, "the failed value itself releases the latch");

// Mute is optimistic on the stream it was asked for, and only on that one.
root.toggleStreamMute(zen);
assert.deepEqual(controls.pop(), ["stream-volume", "7", "mute"]);
const muted = root.applicationStreams();
assert.equal(muted[0].muted, true);
assert.equal(muted[1].muted, false, "muting one application leaves the others alone");
assert.notEqual(muted[0], zen, "the optimistic patch copies rather than writing through the model");
root.controlBusy = true;
const before = root.applicationStreams();
root.toggleStreamMute(muted[1]);
assert.equal(root.applicationStreams(), before, "a refused mute changes nothing on screen");
root.controlBusy = false;
root.toggleStreamMute(null);

// An application that names no icon gets an empty source, so the row draws its
// own mark instead of Quickshell's missing-texture placeholder.
assert.equal(root.streamIcon(zen), "/icons/zen.svg");
assert.equal(root.streamIcon(mpv), "");
assert.equal(root.streamIcon({ icon: "missing" }), "");
assert.equal(root.streamIcon(null), "");

console.log("application mixer level, drag ownership, latch release, mute and icon fallback checks passed");

// The group itself: it has to disappear rather than leave a hole, stay bounded,
// and reuse the routing the panel already owns instead of adding a second path.
const panel = source.slice(source.indexOf("id: audioControlsWindow"), source.indexOf("// Network controls"));
assert.match(panel, /label: "APPLICATIONS"/);
assert.equal((panel.match(/visible: audioControlsWindow\.streams\.length > 0/g) || []).length, 2,
  "both the rule and the card stand down when nothing has played");
assert.match(panel, /Math\.min\(4, audioControlsWindow\.streams\.length\) \* \(root\.rowHeight \+ root\.spaceTight\)/,
  "the group is bounded to four rows and measured from the row it draws");
assert.match(panel, /delegate: ApplicationLevelRow/);
assert.ok(!/setAudioOutputs|audio-device/.test(panel.slice(panel.indexOf('label: "APPLICATIONS"'))),
  "the application group adds no second output-routing path");

const row = source.slice(source.indexOf("component ApplicationLevelRow: Row"), source.indexOf("component ConnectivityRow:"));
assert.match(row, /HoverHandler \{ id: applicationTrackHover \}/);
assert.match(row, /HoverWash \{ hovered: applicationTrackHover\.hovered \}/);
assert.ok(!/HoverWash \{ hovered: applicationLevelMouse\.containsMouse/.test(row),
  "the track's hover comes from the surface, not from the drag area covering it");
assert.match(row, /HoverHandler \{ id: applicationMuteHover \}/);
assert.match(row, /HoverWash \{ hovered: applicationMuteHover\.hovered && !applicationMuteMouse\.pressed \}/);
assert.ok(!/applicationLevelRow\.muted \? root\.dangerColor : applicationMuteMouse\.containsMouse/.test(row),
  "a muted action still receives the neutral hover wash");
assert.ok(!/\b(width|height|font\.pixelSize|radius): [0-9]/.test(row),
  "every size in the row is a token from the block");

console.log("application group visibility, bounds, hover ownership and token use passed");
