// Renders the actual Vicinae view components against mocked host APIs. It
// proves what each row offers and which argument vector every action sends,
// without a desktop, a launcher, or a single subprocess.
const assert = require("node:assert/strict");
const Module = require("node:module");
const originalLoad = Module._load;

const component = (name, extra = {}) =>
  Object.assign(
    function () {
      return null;
    },
    { hostName: name, host: true },
    extra,
  );
const Action = component("action", {
  Style: { Destructive: "destructive", Regular: "regular" },
  CopyToClipboard: component("action.copy"),
  Push: component("action.push"),
});
const ActionPanel = component("action-panel", {
  Submenu: component("action-panel.submenu"),
  Section: component("action-panel.section"),
});
const List = component("list", {
  Section: component("list.section"),
  EmptyView: component("list.empty"),
  Dropdown: component("list.dropdown", {
    Item: component("list.dropdown.item"),
  }),
  Item: component("list.item"),
});
const Icon = new Proxy({}, { get: (_target, key) => String(key) });
const Color = new Proxy({}, { get: (_target, key) => String(key) });

const calls = [];
let confirmed = false;
let statusData = {};
const replies = new Map();

const raycast = {
  Action,
  ActionPanel,
  List,
  Icon,
  Color,
  Alert: { ActionStyle: { Destructive: "destructive" } },
  Toast: {
    Style: { Failure: "failure", Animated: "animated", Success: "success" },
  },
  Clipboard: {
    copy: async (value) => {
      calls.push(["clipboard", [value]]);
    },
  },
  confirmAlert: async (options) => {
    calls.push(["confirm", [options.title]]);
    return confirmed;
  },
  closeMainWindow: async () => calls.push(["close", []]),
  showToast: async (toast) => toast,
};

// --- host React ------------------------------------------------------------
let active = null;
const react = {
  createElement: (type, props, ...children) => ({
    type,
    props: {
      ...(props ?? {}),
      children:
        children.length === 0
          ? undefined
          : children.length === 1
            ? children[0]
            : children,
    },
  }),
  useState(initial) {
    const store = active;
    const index = store.index++;
    store.hooks[index] ??= {
      value: typeof initial === "function" ? initial() : initial,
    };
    const hook = store.hooks[index];
    return [
      hook.value,
      (value) => {
        hook.value = typeof value === "function" ? value(hook.value) : value;
      },
    ];
  },
  useRef(initial) {
    const store = active;
    const index = store.index++;
    store.hooks[index] ??= { current: initial };
    return store.hooks[index];
  },
  useMemo: (fn) => fn(),
  useCallback: (fn) => fn,
  useEffect(fn, deps) {
    const store = active;
    const index = store.index++;
    const previous = store.hooks[index];
    if (
      !previous ||
      !deps ||
      deps.some((value, i) => previous.deps[i] !== value)
    )
      store.effects.push(() => {
        previous?.cleanup?.();
        store.hooks[index] = { deps, cleanup: fn() };
      });
  },
};

function renderer(Component) {
  const store = { hooks: [], index: 0, effects: [] };
  return (props = {}) => {
    active = store;
    store.index = 0;
    const tree = Component(props);
    for (const effect of store.effects.splice(0)) effect();
    active = null;
    return tree;
  };
}

// Presentational components without hooks are expanded here, the way the
// launcher would, so an assertion sees the rows a person actually reads.
function* walk(node) {
  if (!node || typeof node !== "object") return;
  if (Array.isArray(node)) {
    for (const child of node) yield* walk(child);
    return;
  }
  if (typeof node.type === "function" && !node.type.host) {
    const store = { hooks: [], index: 0, effects: [] };
    const previous = active;
    active = store;
    const rendered = node.type(node.props);
    active = previous;
    yield* walk(rendered);
    return;
  }
  yield node;
  // The search bar accessory is a sibling of the rows rather than a child.
  yield* walk(node.props?.searchBarAccessory);
  yield* walk(node.props?.children);
}

const elements = (tree, type) =>
  [...walk(tree)].filter((node) => node.type === type);
const items = (tree) => elements(tree, List.Item);
const titles = (tree) => items(tree).map((item) => item.props.title);
const item = (tree, title) => {
  const found = items(tree).find((node) => node.props.title === title);
  assert(found, `missing row: ${title}`);
  return found;
};
const actions = (row) => [...walk(row.props.actions)];
const action = (row, title) => {
  const found = actions(row).find((node) => node.props?.title === title);
  assert(found, `missing action: ${title} on ${row.props.title}`);
  return found.props;
};
const sectionTitles = (tree) =>
  elements(tree, List.Section).map((node) => node.props.title);
// Live rows refresh themselves after acting; the command is what matters here.
const lastCommand = () => calls.filter(([file]) => file !== "refresh").at(-1);

// --- host modules ----------------------------------------------------------
let queryState = { data: undefined, error: undefined, loading: true };
let queryPending;
let queryStarted = false;
const runtime = {
  binaries: {
    control: "control",
    shell: "shellctl",
    run0: "run0",
    switchGeneration: "helper",
  },
  run: async (file, args) => {
    calls.push([file, [...args]]);
    const reply = replies.get(`${file} ${args[0]}`);
    if (reply === undefined) return "";
    return reply;
  },
  perform: async (title, run, dismiss = false) => {
    if (dismiss) calls.push(["close", []]);
    try {
      await run();
      return true;
    } catch (error) {
      calls.push(["toast", [title]]);
      return false;
    }
  },
  shell: async (args) => {
    calls.push(["shellctl", [...args]]);
  },
  control: async (args) => {
    calls.push(["control", [...args]]);
  },
  useQuery(load) {
    if (!queryStarted) {
      queryStarted = true;
      queryPending = load(new AbortController().signal).then(
        (data) => {
          queryState = { data, error: undefined, loading: false };
        },
        (error) => {
          queryState = {
            data: undefined,
            error: String(error),
            loading: false,
          };
        },
      );
    }
    return { ...queryState, refresh: () => calls.push(["refresh", []]) };
  },
};
const status = {
  useStatus: () => ({
    data: statusData,
    error: false,
    loading: false,
    refresh: () => calls.push(["refresh", []]),
  }),
};

Module._load = function (name, ...args) {
  if (name === "@raycast/api") return raycast;
  if (name === "react") return react;
  if (name === "./runtime") return runtime;
  if (name === "./status") return status;
  return originalLoad.call(this, name, ...args);
};

function reset() {
  calls.length = 0;
  queryState = { data: undefined, error: undefined, loading: true };
  queryStarted = false;
  queryPending = undefined;
}

const [, , controlsPath, windowsPath, audioPath, keybindingsPath] =
  process.argv;

(async () => {
  // --- Seele Controls ------------------------------------------------------
  reset();
  statusData = {
    volume: 42,
    muted: true,
    microphoneVolume: 70,
    microphoneMuted: false,
    dnd: false,
    wifiAvailable: true,
    wifiEnabled: true,
    connection: "home",
    connectionType: "802-11-wireless",
    connectivity: "full",
    bluetoothAvailable: false,
    batteries: [
      { kind: "device", name: "AirPods Pro", percent: 12, status: "" },
    ],
    tailscale: {
      available: true,
      connected: true,
      needsLogin: false,
      name: "nerv",
      tailnet: "example.ts.net",
      onlinePeers: 2,
      peers: 5,
    },
    headphones: { connected: true, name: "AirPods Pro" },
  };
  const controls = renderer(require(controlsPath).default);
  const root = controls();
  assert.deepEqual(calls, [], "Rendering must not run a single command");
  const rows = titles(root);
  for (const expected of [
    "Output Volume",
    "Microphone Volume",
    "Do Not Disturb",
    "Wi-Fi",
    "Tailscale",
    "AirPods Pro",
    "Windows and Workspaces",
    "Keybindings",
    "NixOS Generations",
    "Control Center",
    "Quick AI Prompt",
    "System Health",
  ])
    assert(rows.includes(expected), `Seele Controls is missing ${expected}`);
  assert(
    !rows.includes("Bluetooth"),
    "An unavailable radio must not show a dead row",
  );
  assert(
    rows.includes("AirPods"),
    "A connected pair of AirPods renames its own panel row",
  );
  const volume = item(root, "Output Volume");
  assert.deepEqual(volume.props.accessories, [
    { tag: { value: "Muted", color: "Red" } },
    { text: "42%" },
  ]);
  assert.equal(volume.props.icon, "SpeakerOff");
  await action(volume, "Unmute").onAction();
  assert.deepEqual(lastCommand(), ["shellctl", ["volume", "mute"]]);
  await action(volume, "50%").onAction();
  assert.deepEqual(lastCommand(), ["shellctl", ["volume", "50"]]);
  await action(item(root, "Wi-Fi"), "Turn off Wi-Fi").onAction();
  assert.deepEqual(lastCommand(), ["control", ["wifi", "toggle"]]);
  await action(item(root, "Do Not Disturb"), "15 Minutes").onAction();
  assert.deepEqual(lastCommand(), [
    "shellctl",
    ["notification", "snooze", "15"],
  ]);
  assert.deepEqual(item(root, "AirPods Pro").props.accessories, [
    { tag: { value: "12%", color: "Red" } },
  ]);
  await action(item(root, "System Health"), "Open System Health").onAction();
  assert.deepEqual(lastCommand(), ["shellctl", ["control", "system-health"]]);

  // --- Windows and workspaces ---------------------------------------------
  reset();
  const client = (address, order, workspace) => ({
    address,
    title: `Window ${address}`,
    class: address === "0xa" ? "ghostty" : "zen",
    workspace: { id: workspace, name: String(workspace) },
    monitor: 0,
    focusHistoryID: order,
  });
  replies.set(
    "control vicinae-desktop",
    JSON.stringify({
      clientGroups: [[client("0xa", 0, 1)], [client("0xb", 1, 2)]],
      workspaces: [
        { id: 1, name: "1", monitor: "DP-1", windows: 1 },
        { id: 2, name: "2", monitor: "DP-1", windows: 1 },
      ],
    }),
  );
  const windows = renderer(require(windowsPath).default);
  windows();
  await queryPending;
  let desktop = windows();
  assert.deepEqual(titles(desktop).slice(0, 2), ["Window 0xa", "Window 0xb"]);
  const focused = item(desktop, "Window 0xa");
  assert.deepEqual(focused.props.accessories[0], {
    tag: { value: "Focused", color: "Green" },
  });
  assert.equal(focused.props.icon, "Terminal");
  assert.equal(item(desktop, "Window 0xb").props.icon, "Compass");
  calls.length = 0;
  await action(focused, "Focus Window").onAction();
  assert.deepEqual(calls, [
    ["close", []],
    ["control", ["vicinae-focus", "window", "0xa"]],
  ]);
  calls.length = 0;
  await action(focused, "Close Window").onAction();
  assert.deepEqual(calls, [
    ["control", ["application", "quit", "0xa"]],
    ["refresh", []],
  ]);
  calls.length = 0;
  confirmed = false;
  await action(focused, "Force Quit Application").onAction();
  assert.deepEqual(
    calls,
    [["confirm", ["Force quit ghostty?"]]],
    "A declined force quit must reach no command",
  );
  confirmed = true;
  calls.length = 0;
  await action(focused, "Force Quit Application").onAction();
  assert.deepEqual(calls, [
    ["confirm", ["Force quit ghostty?"]],
    ["control", ["application", "force-quit", "0xa"]],
    ["refresh", []],
  ]);
  const dropdown = elements(desktop, List.Dropdown)[0];
  assert(dropdown, "The workspace filter must be offered");
  dropdown.props.onChange("2");
  desktop = windows();
  assert.deepEqual(
    titles(desktop).filter((title) => title.startsWith("Window")),
    ["Window 0xb"],
  );
  dropdown.props.onChange("99");
  desktop = windows();
  assert.deepEqual(
    titles(desktop).filter((title) => title.startsWith("Window")),
    ["Window 0xa", "Window 0xb"],
    "A workspace that disappeared must not hide the whole desktop",
  );

  // --- Audio devices -------------------------------------------------------
  reset();
  const speakers = {
    id: 1,
    kind: "output",
    name: "Speakers",
    node: "speaker",
    profile: null,
    default: true,
    selected: true,
  };
  const headphones = {
    id: 2,
    kind: "output",
    name: "AirPods Pro",
    node: "airpods",
    profile: null,
    default: false,
    selected: true,
  };
  const microphone = {
    id: 3,
    kind: "input",
    name: "Yeti",
    node: "yeti",
    profile: null,
    default: true,
  };
  statusData = {
    volume: 30,
    muted: false,
    audioDevices: [speakers, headphones, microphone],
  };
  const audio = renderer(require(audioPath).default);
  const devices = audio();
  assert.deepEqual(titles(devices), ["Speakers", "AirPods Pro", "Yeti"]);
  assert.deepEqual(item(devices, "Speakers").props.accessories[0], {
    tag: { value: "Default", color: "Green" },
  });
  assert.deepEqual(item(devices, "AirPods Pro").props.accessories[0], {
    tag: { value: "Also playing", color: "Blue" },
  });
  assert.equal(item(devices, "AirPods Pro").props.icon, "Airpods");
  assert.equal(
    elements(devices, List.Section)[0].props.subtitle,
    "2 outputs playing together",
  );
  calls.length = 0;
  await action(item(devices, "AirPods Pro"), "Use Only This Device").onAction();
  assert.deepEqual(calls[0], [
    "control",
    ["vicinae-audio", JSON.stringify(headphones), "select"],
  ]);
  calls.length = 0;
  await action(item(devices, "AirPods Pro"), "Remove from Playback").onAction();
  assert.deepEqual(calls[0], [
    "control",
    ["vicinae-audio", JSON.stringify(headphones), "toggle"],
  ]);

  // --- Keybindings ---------------------------------------------------------
  reset();
  const binding = (id, shortcut, modifiers, key) => ({
    id,
    shortcut,
    action: "exec seele-shellctl menu",
    description: `Binding ${id}`,
    key,
    modmask: 0,
    modifiers,
    inputAllowed: true,
  });
  replies.set(
    "control vicinae-keybindings",
    JSON.stringify([
      binding("a", "Super + Shift + S", ["Super", "Shift"], "S"),
      binding("b", "Super + D", ["Super"], "D"),
      binding("c", "XF86AudioPlay", [], "XF86AudioPlay"),
      binding("d", "Ctrl + Alt + Delete", ["Ctrl", "Alt"], "Delete"),
    ]),
  );
  const keybindings = renderer(require(keybindingsPath).default);
  keybindings();
  await queryPending;
  const bindings = keybindings();
  assert.deepEqual(sectionTitles(bindings), [
    "Super",
    "Ctrl + Alt",
    "Super + Shift",
    "Single keys",
  ]);
  const row = item(bindings, "Binding b");
  assert.deepEqual(row.props.accessories, [
    { tag: { value: "Super + D", color: "PrimaryText" } },
  ]);
  calls.length = 0;
  await action(row, "Input Keybinding").onAction();
  assert.deepEqual(calls, [
    ["close", []],
    [
      "control",
      ["vicinae-input-keybinding", JSON.stringify({ key: "D", modmask: 0 })],
    ],
  ]);
  await action(row, "Copy Command").onAction();
  assert.deepEqual(lastCommand(), ["clipboard", ["exec seele-shellctl menu"]]);

  console.log(
    "Vicinae view rows, state presentation, filters and action argument vectors passed",
  );
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
