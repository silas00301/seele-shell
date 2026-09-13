const assert = require("node:assert/strict");
const { nativeFunctions } = require("./native-functions.cjs");
const call = (name, ...args) =>
  nativeFunctions().Functions.call(`vicinae.${name}`, args);
const client = (address, order, extra = {}) => ({
  address,
  focusHistoryID: order,
  class: "ghostty",
  mapped: true,
  hidden: false,
  ...extra,
});
const snapshot = call(
  "desktop",
  [
    client("0x1", 2),
    client("0x2", 0),
    client("0x3", 1, { hidden: true }),
    client("0x4", 1, { mapped: false }),
    client("0x5", 1, { class: "Vicinae" }),
  ],
  [],
);
assert.deepEqual(
  snapshot.clientGroups.flat().map((c) => c.address),
  ["0x2", "0x1"],
);
const device = (id, profile) => ({
  id,
  profile,
  name: "Speakers",
  kind: "output",
  node: "",
});
assert.deepEqual(
  call("audioSelection", device(20, null), [device(20, null)], false),
  ["audio-device", "20"],
);
assert.deepEqual(
  call("audioSelection", device(10, 0), [device(10, 0)], false),
  ["audio-device", "10", "0"],
);
assert.throws(() => call("audioSelection", device("20;bad", null), [], false));
assert.throws(() => call("audioSelection", device(20, -1), [], false));
const Module = require("node:module");
const original = Module._load;
const calls = [];
Module._load = function (name, ...args) {
  if (name === "./runtime")
    return {
      binaries: { control: "control" },
      run: async (file, args) => calls.push([file, args]),
    };
  return original.call(this, name, ...args);
};
(async () => {
  const desktop = require(process.argv[2]);
  await desktop.focusWindow("0xABC12");
  await desktop.focusWorkspace(12);
  assert.deepEqual(calls, [
    ["control", ["vicinae-focus", "window", "0xABC12"]],
    ["control", ["vicinae-focus", "workspace", "12"]],
  ]);
  console.log(
    "Vicinae native window/audio projection and host action handoff tests passed",
  );
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
