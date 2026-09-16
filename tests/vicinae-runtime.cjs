const assert = require("node:assert/strict");
const Module = require("node:module");
const { EventEmitter } = require("node:events");
const { PassThrough } = require("node:stream");
const originalLoad = Module._load;
const events = [];
const states = [];
let cleanup;
let worker;
Module._load = function (name, ...args) {
  if (name === "@raycast/api")
    return {
      closeMainWindow: async () => {
        await Promise.resolve();
        events.push("closed");
      },
      showToast: async (toast) => events.push(toast),
      Toast: { Style: { Failure: "failure" } },
    };
  if (name === "react")
    return {
      useState(initial) {
        const index = states.push(initial) - 1;
        return [
          initial,
          (value) => {
            states[index] =
              typeof value === "function" ? value(states[index]) : value;
          },
        ];
      },
      useEffect(effect) {
        cleanup = effect();
      },
      useRef: (value) => ({ current: value }),
      useCallback: (fn) => fn,
    };
  if (name === "./runtime")
    return { binaries: { control: "/test/seele-control" } };
  if (name === "node:child_process")
    return {
      ...originalLoad.call(this, name, ...args),
      spawn(file, args) {
        assert.equal(file, "/test/seele-control");
        assert.deepEqual(args, ["watch-status"]);
        worker = new EventEmitter();
        worker.stdout = new PassThrough();
        worker.stdin = new PassThrough();
        worker.kill = () => {
          throw new Error("Clean EOF should not require termination");
        };
        worker.stdin.on("finish", () => worker.emit("close", 0));
        return worker;
      },
    };
  return originalLoad.call(this, name, ...args);
};
(async () => {
  const runtime = require(process.argv[2]);
  await runtime.perform(
    "Focus",
    async () => {
      events.push("action");
    },
    true,
  );
  assert.deepEqual(events, ["closed", "action"]);
  await runtime.perform("Switch", async () => {
    throw new Error("private subprocess output");
  });
  assert.equal(events[2].style, "failure");
  assert(!JSON.stringify(events).includes("private subprocess output"));
  const { useStatus } = require(process.argv[3]);
  const begin = () => {
    states.length = 0;
    return useStatus();
  };
  const status = begin();
  worker.stdout.write('{"volume":42,"muted":false}\n');
  worker.stdout.write('{"dnd":true}\n');
  assert.deepEqual(states[0], { volume: 42, muted: false, dnd: true });
  worker.stdout.write('{"muted":true}\n');
  assert.equal(states[0].volume, 42);
  assert.equal(states[0].muted, true);
  for (let i = 0; i < 1000; i++) status.refresh();
  assert.equal(
    worker.stdin.read().toString(),
    "all\n",
    "Refresh requests must coalesce until a status reply",
  );
  worker.stdout.write('{"volume":');
  worker.stdout.write('43,"injectedUnknownField":{"retain":"nothing"}}\n');
  assert.equal(states[0].volume, 43);
  assert.equal(states[0].injectedUnknownField, undefined);
  status.refresh();
  assert.equal(worker.stdin.read().toString(), "all\n");
  worker.stdout.write("invalid JSON\n");
  assert.equal(states[1], true);
  cleanup();
  assert(worker.stdin.writableEnded);
  worker.stdout.write('{"volume":99}\n');
  assert.equal(states[0].volume, 43);
  for (const bad of [
    '{"volume":"42"}\n',
    '{"muted":1}\n',
    '{"volume":1e999}\n',
    '{"headphones":{"connected":true,"name":42}}\n',
    '{"audioDevices":[{"id":1}]}\n',
    '{"batteries":{}}\n',
    '{"tailscale":{"available":true}}\n',
    '{"connection":42}\n',
    '{"bluetoothConnected":-1}\n',
    "[]\n",
    " ".repeat(256 * 1024 + 1),
  ]) {
    begin();
    worker.stdout.write(bad);
    assert.equal(
      states[1],
      true,
      "Malformed or oversized frames must stop the worker",
    );
    assert.equal(worker.stdout.listenerCount("data"), 0);
    assert.equal(worker.stdin.writableEnded, true);
    worker.stdout.emit("data", Buffer.from('{"volume":99}\n'));
    assert.deepEqual(states[0], {});
    cleanup();
  }
  begin();
  const device = {
    id: 1,
    kind: "output",
    name: "Speakers",
    node: "speaker",
    profile: null,
    selected: true,
    default: true,
  };
  worker.stdout.write(
    JSON.stringify({
      headphones: { connected: false, name: "", ignored: "x" },
      audioDevices: [device],
    }) + "\n",
  );
  assert.deepEqual(states[0], {
    headphones: { connected: false, name: "" },
    audioDevices: [device],
  });
  // Peripherals name themselves: an unrenderable battery is dropped and an
  // out-of-range reading is bounded, without stopping the whole feed.
  worker.stdout.write(
    JSON.stringify({
      connection: "home",
      batteries: [
        { kind: "system", name: "BAT0", percent: 250.6, status: "Charging" },
        { kind: "device", name: "bad\u0001name", percent: 40, status: "" },
        { kind: "device", name: "Mouse", percent: "40", status: "" },
      ],
    }) + "\n",
  );
  assert.equal(states[0].connection, "home");
  assert.deepEqual(states[0].batteries, [
    { kind: "system", name: "BAT0", percent: 100, status: "Charging" },
  ]);
  worker.emit("close", 0);
  assert.equal(states[1], true);
  cleanup();

  // A polled view must not flash its loading indicator on every tick, while a
  // first load and an explicit refresh still report that they are working.
  states.length = 0;
  let loads = 0;
  let deliver;
  const settle = async () => {
    for (let i = 0; i < 4; i++) await Promise.resolve();
  };
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const query = runtime.useQuery(() => {
    loads++;
    return new Promise((resolve) => {
      deliver = resolve;
    });
  }, 50);
  assert.equal(loads, 1);
  assert.equal(states[2], true, "The first load reports that it is working");
  deliver("first");
  await settle();
  assert.equal(states[0], "first");
  assert.equal(states[2], false);
  await wait(90);
  assert.equal(loads, 2, "The interval keeps polling on its own");
  assert.equal(
    states[2],
    false,
    "A background poll must not flash the loading indicator",
  );
  deliver("second");
  await settle();
  assert.equal(states[0], "second");
  query.refresh();
  assert.equal(loads, 3);
  assert.equal(
    states[2],
    true,
    "An explicit refresh reports that it is working",
  );
  deliver("third");
  await settle();
  assert.equal(states[2], false);
  cleanup();
  const polled = loads;
  await wait(90);
  assert.equal(loads, polled, "No query survives the view that owns it");

  console.log(
    "Vicinae focus handoff, error privacy, field patches, quiet polling, and worker cleanup tests passed",
  );
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
