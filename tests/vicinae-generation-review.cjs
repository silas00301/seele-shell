const assert = require("node:assert/strict");
const Module = require("node:module");
const originalLoad = Module._load;
const target = "/nix/store/00000000000000000000000000000000-target-system";
const running = "/nix/store/11111111111111111111111111111111-running-system";
const changed = "/nix/store/22222222222222222222222222222222-changed-system";
const calls = [];
const diffs = [];
let confirmed = true;
let confirmations = 0;
let currentTarget = target;
let hookIndex = 0;
let hooks = [];
let effects = [];
function deferred() {
  let resolve, reject;
  const promise = new Promise((a, b) => {
    resolve = a;
    reject = b;
  });
  return { promise, resolve, reject };
}
const Action = Object.assign(() => {}, {
  Style: { Destructive: "destructive" },
});
const react = {
  createElement: (type, props, ...children) => ({
    type,
    props: {
      ...props,
      children: children.length === 1 ? children[0] : children,
    },
  }),
  useState(initial) {
    const index = hookIndex++;
    hooks[index] ??= { value: initial };
    return [
      hooks[index].value,
      (value) => {
        hooks[index].value =
          typeof value === "function" ? value(hooks[index].value) : value;
      },
    ];
  },
  useRef(initial) {
    const index = hookIndex++;
    hooks[index] ??= { current: initial };
    return hooks[index];
  },
  useMemo: (fn) => fn(),
  useEffect(fn, deps) {
    const index = hookIndex++;
    if (
      !hooks[index] ||
      deps.some((value, i) => hooks[index].deps[i] !== value)
    ) {
      effects.push(() => {
        hooks[index]?.cleanup?.();
        hooks[index] = { deps, cleanup: fn() };
      });
    }
  },
};
Module._load = function (name, ...args) {
  if (name === "react") return react;
  if (name === "@raycast/api")
    return {
      Action,
      ActionPanel: "actions",
      Detail: "detail",
      List: {},
      Icon: {},
      Alert: { ActionStyle: { Destructive: "destructive" } },
      Toast: {
        Style: { Animated: "animated", Success: "success", Failure: "failure" },
      },
      confirmAlert: async () => {
        confirmations++;
        return confirmed;
      },
      closeMainWindow: async () => {},
      showToast: async (toast) => toast,
    };
  if (name === "./runtime")
    return {
      binaries: {
        control: "control",
        run0: "run0",
        switchGeneration: "helper",
      },
      run: async (file, args, signal) => {
        calls.push({ file, args, signal });
        if (file === "control" && args[0] === "vicinae-generation-diff") {
          const diff = deferred();
          diffs.push(diff);
          return diff.promise;
        }
        if (file === "control" && args[0] === "vicinae-generation-check") {
          if (currentTarget !== target)
            throw Error("Reviewed identity changed");
          return '{"ok":true}';
        }
        assert.equal(file, "run0");
        return "";
      },
    };
  return originalLoad.call(this, name, ...args);
};
const { GenerationDetail } = require(process.argv[2]);
const generation = {
  generation: 42,
  date: "2026-04-12T10:11:12Z",
  nixosVersion: "26.05",
  kernelVersion: "6.18",
  configurationRevision: "",
  specialisations: [],
  profilePath: "/nix/var/nix/profiles/system-42-link",
  storePath: target,
  runningStorePath: running,
  active: false,
  switchArguments: ["42", target.slice(11), running.slice(11)],
  escaped: {
    nixosVersion: "26\\.05",
    kernelVersion: "6\\.18",
    configurationRevision: "",
    specialisations: [],
  },
};
function render(value = generation) {
  hookIndex = 0;
  const tree = GenerationDetail({ generation: value });
  for (const effect of effects.splice(0)) effect();
  return tree;
}
function action(tree) {
  return tree.props.actions?.props.children?.props.onAction;
}
const settle = () => new Promise((resolve) => setImmediate(resolve));
(async () => {
  assert.equal(action(render()), undefined);
  assert.deepEqual(
    calls[0].args,
    ["vicinae-generation-diff", "42", target.slice(11), running.slice(11)],
    "Review must diff immutable store closures",
  );
  diffs[0].reject(new Error("nvd failed"));
  await settle();
  assert.equal(
    action(render()),
    undefined,
    "Failed diff must never offer activation",
  );

  const second = { ...generation };
  render(second);
  diffs[1].resolve(JSON.stringify({ diff: "    + package" }));
  await settle();
  const oldAction = action(render(second));
  assert.equal(typeof oldAction, "function");
  const third = { ...generation };
  render(third);
  await oldAction();
  assert.equal(
    confirmations,
    0,
    "Actions from a replaced review are invalidated",
  );
  const fourth = { ...generation };
  render(fourth);
  diffs[2].resolve(JSON.stringify({ diff: "    stale successful diff" }));
  await settle();
  assert.equal(
    action(render(fourth)),
    undefined,
    "Late old results must not approve another review",
  );
  diffs[3].resolve(JSON.stringify({ diff: "    reviewed package diff" }));
  await settle();
  const switchAction = action(render(fourth));
  confirmed = false;
  await switchAction();
  assert.equal(calls.filter((call) => call.file === "run0").length, 0);

  confirmed = true;
  currentTarget = changed;
  await switchAction();
  assert.equal(
    calls.filter((call) => call.file === "run0").length,
    0,
    "Changed targets must fail before escalation",
  );
  currentTarget = target;
  const before = confirmations;
  await Promise.all([switchAction(), switchAction()]);
  assert.equal(
    confirmations,
    before + 1,
    "Duplicate actions must share one pending confirmation",
  );
  const switches = calls.filter((call) => call.file === "run0");
  assert.equal(switches.length, 1);
  assert.deepEqual(
    switches[0].args,
    ["helper", "42", target.slice(11), running.slice(11)],
    "Authorization must carry both reviewed identities to the native helper",
  );
  for (const hook of hooks) hook?.cleanup?.();
  console.log(
    "Vicinae immutable diff, failed/stale review, confirmation and native identity handoff tests passed",
  );
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
