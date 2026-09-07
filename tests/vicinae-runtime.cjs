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
  const status = useStatus();
  worker.stdout.write('{"volume":42,"muted":false}\n');
  worker.stdout.write('{"dnd":true}\n');
  assert.deepEqual(states[0], { volume: 42, muted: false, dnd: true });
  worker.stdout.write('{"muted":true}\n');
  assert.equal(states[0].volume, 42);
  assert.equal(states[0].muted, true);
  status.refresh();
  assert.equal(worker.stdin.read().toString(), "all\n");
  worker.stdout.write("invalid JSON\n");
  assert.equal(states[1], true);
  cleanup();
  assert(worker.stdin.writableEnded);
  worker.stdout.write('{"volume":99}\n');
  assert.equal(states[0].volume, 42);
  console.log(
    "Vicinae focus handoff, error privacy, field patches, and worker cleanup tests passed",
  );
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
