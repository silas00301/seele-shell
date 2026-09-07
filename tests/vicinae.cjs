const assert = require("node:assert/strict");
const { visibleClients, focusWindow, focusWorkspace, audioArguments } = require(
  process.argv[2],
);
const client = (address, order, extra = {}) => ({
  address,
  focusHistoryID: order,
  class: "ghostty",
  mapped: true,
  hidden: false,
  ...extra,
});
assert.deepEqual(
  visibleClients([
    client("0x1", 2),
    client("0x2", 0),
    client("0x3", 1, { hidden: true }),
    client("0x4", 1, { mapped: false }),
    client("0x5", 1, { class: "Vicinae" }),
  ]).map((c) => c.address),
  ["0x2", "0x1"],
);
assert.equal(
  focusWindow("0xABC12"),
  'hl.dsp.focus({ window = "address:0xABC12" })',
);
for (const address of ['0x1" }); os.execute("bad")', "", "active"])
  assert.throws(() => focusWindow(address));
assert.equal(focusWorkspace(12), "hl.dsp.focus({ workspace = 12 })");
for (const id of [-1, 0, 1.5, "1", Infinity])
  assert.throws(() => focusWorkspace(id));
assert.deepEqual(audioArguments({ id: 20, profile: null }), [
  "audio-device",
  "20",
]);
assert.deepEqual(audioArguments({ id: 10, profile: 0 }), [
  "audio-device",
  "10",
  "0",
]);
assert.throws(() => audioArguments({ id: "20;bad", profile: null }));
assert.throws(() => audioArguments({ id: 20, profile: -1 }));
console.log("Vicinae window targeting and audio profile tests passed");
