const assert = require("node:assert/strict");
const Module = require("node:module");
const { nativeFunctions } = require("./native-functions.cjs");
const native = (op, ...args) =>
  nativeFunctions().Functions.call(`vicinae.${op}`, args);
const input = [];
for (const key of [
  "RETURN",
  "ESCAPE",
  "S",
  "1",
  "2",
  "3",
  "4",
  "XF86AudioRaiseVolume",
  "mouse:272",
  "code:42",
  "adiaeresis",
  "é",
  "🌸",
]) {
  for (const modmask of [0, 1, 4, 8, 64, 65, 68, 72, 77])
    input.push({
      key,
      modmask,
      dispatcher: "exec",
      arg: "literal $data",
      description: "__lua 42",
    });
}
input.push(
  { key: "S", modmask: 68, description: "  Explicit label  " },
  { key: "x", description: "lua\ufeff42" },
  { key: "x", description: "lua\u008542" },
);
const rows = native("keybindings", input);
const calls = [];
const original = Module._load;
Module._load = function (name, ...args) {
  if (name === "./runtime")
    return {
      binaries: { control: "control", hyprctl: "hyprctl", wtype: "wtype" },
      perform: async (_name, fn, close) => {
        assert.equal(close, true);
        await fn();
      },
      run: async (file, args) => {
        if (file === "control" && args[0] === "vicinae-keybindings")
          return JSON.stringify(rows);
        if (file === "hyprctl") return JSON.stringify(input);
        if (file === "control" && args[0] === "vicinae-input-keybinding") {
          const typed = native("keybindingInput", JSON.parse(args[1]));
          if (typed.length) calls.push(["wtype", typed]);
          return "";
        }
        calls.push([file, Array.from(args)]);
        return "";
      },
    };
  if (name === "react") return {};
  if (name === "@raycast/api") return {};
  return original.call(this, name, ...args);
};
(async () => {
  const adapter = require(process.argv[2]);
  const current = await adapter.loadBindings(new AbortController().signal);
  assert(!current.some((row) => row.key.startsWith("mouse:")));
  assert.equal(
    current.find((row) => row.key === "S" && row.modmask === 68).description,
    "Explicit label",
  );
  assert.equal(
    current.find(
      (row) => row.key === "XF86AudioRaiseVolume" && row.modmask === 0,
    ).description,
    "Hyprland keybinding",
  );
  const selected = current.find(
    (row) => row.key === "RETURN" && row.modmask === 77,
  );
  await adapter.executeBinding(selected);
  assert.deepEqual(calls.pop(), [
    "wtype",
    [
      "-M",
      "logo",
      "-M",
      "ctrl",
      "-M",
      "alt",
      "-M",
      "shift",
      "-k",
      "Return",
      "-m",
      "shift",
      "-m",
      "alt",
      "-m",
      "ctrl",
      "-m",
      "logo",
    ],
  ]);
  for (const value of [{ key: "x", modmask: -1 }, { key: "\n" }, { key: 42 }])
    assert.throws(() => native("keybindingInput", value));
  if (process.argv[3]) {
    const reference = require(process.argv[3]);
    const previous = await reference.loadBindings(new AbortController().signal);
    const projection = (items) =>
      items.map(({ id, shortcut, action, description, key, modmask = 0 }) => ({
        id,
        shortcut,
        action,
        description,
        key,
        modmask,
      }));
    assert.deepEqual(projection(current), projection(previous));
    for (let i = 0; i < current.length; i++) {
      calls.length = 0;
      await reference.executeBinding(previous[i]);
      const old = calls.splice(0);
      await adapter.executeBinding(current[i]);
      assert.deepEqual(calls.splice(0), old);
    }
  }
  console.log(
    `Vicinae keybinding native policy/host actions passed (${current.length} rows${process.argv[3] ? ", exact baseline differential" : ""})`,
  );
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
