// Drive the actual Caffeinate launcher component against controlled native
// replies. Rendering may compose no session text, no duration and no policy.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const Module = require("node:module");
const originalLoad = Module._load;

let hookIndex = 0;
const hooks = [];
const calls = [];
const toasts = [];
let refreshes = 0;
let reply = { ok: true };
let display = { active: false, headline: "Caffeinate", detail: "", barText: "", hoverText: "", task: null };
const tasks = [
  { key: "transfer:g1", kind: "transfer", label: "Sending 3 file(s) to iPhone", project: null, pid: null },
  { key: "process:4242:991", kind: "build", label: "nix build", project: "seele", pid: 4242 },
  { key: "process:77:12", kind: "process", label: "ghostty", project: null, pid: 77 },
];

const Action = () => {};
const List = { Item: "List.Item", Section: "List.Section" };
const Icon = new Proxy(
  {},
  { get: (_, name) => (typeof name === "string" ? `icon:${name}` : undefined) },
);
const react = {
  createElement: (type, props, ...children) => ({
    type,
    props: { ...props, children: children.length === 1 ? children[0] : children },
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
};
Module._load = function (name, ...args) {
  if (name === "react") return react;
  if (name === "@raycast/api")
    return {
      Action,
      ActionPanel: "actions",
      Icon,
      List,
      Toast: { Style: { Failure: "failure" } },
      showToast: async (toast) => {
        toasts.push(toast);
        return toast;
      },
    };
  if (name === "./runtime")
    return {
      binaries: { control: "control" },
      run: async (file, argv) => {
        calls.push({ file, argv });
        const operation = argv[1];
        if (operation === "snapshot")
          return JSON.stringify({ ok: true, session: {}, display });
        if (operation === "tasks") return JSON.stringify({ ok: true, tasks });
        return JSON.stringify(reply);
      },
      // The polling adapter has its own fixture; this one controls its results.
      useQuery: () => ({
        data: queryTurn() === 0 ? display : tasks,
        error: undefined,
        loading: false,
        refresh: () => {
          refreshes++;
        },
      }),
    };
  return originalLoad.call(this, name, ...args);
};

let turn = 0;
function queryTurn() {
  return turn++ % 2;
}

const bundle = process.argv[2];
const Command = require(bundle).default;
const source = fs.readFileSync(bundle, "utf8");

function render() {
  hookIndex = 0;
  turn = 0;
  return Command();
}
function walk(node, visit) {
  if (!node || typeof node !== "object") return;
  if (Array.isArray(node)) {
    for (const child of node) walk(child, visit);
    return;
  }
  visit(node);
  walk(node.props?.children, visit);
}
function items(tree) {
  const found = [];
  walk(tree, (node) => {
    if (node.type === List.Item) found.push(node);
  });
  return found;
}
function titled(tree, title) {
  const item = items(tree).find((node) => node.props.title === title);
  assert.ok(item, `no row titled ${title}`);
  return item;
}
function primary(item) {
  const found = [];
  walk(item.props.actions, (node) => {
    if (node.type === Action) found.push(node);
  });
  assert.ok(found.length, "a row offers no action");
  return found[0].props.onAction;
}
function last() {
  return calls[calls.length - 1];
}

(async () => {
  // Presets hand a duration to the service verbatim; nothing is parsed here.
  await primary(titled(render(), "For 1h"))();
  assert.deepEqual(last().argv, [
    "vicinae-caffeinate",
    "start",
    '{"mode":"duration","duration":"1h"}',
  ]);
  await primary(titled(render(), "Until stopped"))();
  assert.deepEqual(last().argv, [
    "vicinae-caffeinate",
    "start",
    '{"mode":"manual"}',
  ]);

  // A typed duration reaches native validation exactly as it was typed.
  render().props.onSearchTextChange("1h30 ");
  await primary(titled(render(), "For 1h30 "))();
  assert.deepEqual(last().argv, [
    "vicinae-caffeinate",
    "start",
    '{"mode":"duration","duration":"1h30 "}',
  ]);
  assert.doesNotMatch(source, /3600|Date\.now|new Date/, "durations are not parsed or counted here");

  // A task starts from its opaque key, never from a PID the row happens to show.
  render().props.onSearchTextChange("");
  await primary(titled(render(), "nix build"))();
  assert.deepEqual(last().argv, [
    "vicinae-caffeinate",
    "start",
    '{"mode":"task","task":"process:4242:991"}',
  ]);

  // Builds and transfers come before other processes, in the service's order.
  const titles = items(render()).map((item) => item.props.title);
  assert.ok(
    titles.indexOf("Sending 3 file(s) to iPhone") < titles.indexOf("nix build"),
    "transfers and builds keep the order the service gave them",
  );
  assert.ok(
    titles.indexOf("nix build") < titles.indexOf("ghostty"),
    "other processes follow builds and transfers",
  );

  // A refused start shows the native message and changes nothing.
  const before = refreshes;
  reply = { ok: false, message: "That task has already ended. Refresh and choose another." };
  await primary(titled(render(), "nix build"))();
  assert.equal(toasts[toasts.length - 1].message, reply.message);
  assert.equal(refreshes, before, "a refused start refreshes nothing");
  reply = { ok: true };

  // The session row states what the projection said and offers Stop.
  display = {
    active: true,
    headline: "Until a task ends",
    detail: "nix build",
    barText: "\u{f0176}",
    hoverText: "Caffeinate · Until a task ends · nix build",
    task: tasks[1],
  };
  const session = titled(render(), "Until a task ends");
  assert.equal(session.props.subtitle, "nix build");
  assert.deepEqual(session.props.accessories, [{ text: "nix build" }]);
  const stop = primary(session);
  const pending = calls.length;
  await Promise.all([stop(), stop()]);
  assert.equal(calls.length, pending + 1, "one Stop is in flight at a time");
  assert.deepEqual(last().argv, ["vicinae-caffeinate", "stop"]);

  // Closing the launcher is not an end of the session.
  assert.doesNotMatch(source, /closeMainWindow/, "this command dismisses nothing");

  console.log(
    "Vicinae Caffeinate duration handoff, task identity, ordering, refusal and single Stop passed",
  );
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
