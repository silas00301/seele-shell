// Drive the real picker, and the shared presentation it now uses, against
// controlled native replies. No launcher, no desktop and no subprocess.
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
});
const ActionPanel = component("action-panel", {
  Section: component("action-panel.section"),
});
const Metadata = component("metadata", {
  Label: component("metadata.label"),
  Separator: component("metadata.separator"),
  TagList: component("metadata.taglist", {
    Item: component("metadata.taglist.item"),
  }),
});
const Detail = component("list.item.detail", { Metadata });
const List = component("list", {
  Section: component("list.section"),
  EmptyView: component("list.empty"),
  Item: component("list.item", { Detail }),
});
const Icon = new Proxy({}, { get: (_target, key) => String(key) });
const Color = new Proxy({}, { get: (_target, key) => String(key) });

const toasts = [];
const calls = [];
let resolve, reject;

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
  useEffect() {},
};

let refreshes = 0;
const query = {
  data: undefined,
  error: undefined,
  loading: false,
  refresh: () => refreshes++,
};

Module._load = function (name, ...args) {
  if (name === "react") return react;
  if (name === "@raycast/api")
    return {
      Action,
      ActionPanel,
      List,
      Icon,
      Color,
      Keyboard: {},
      Toast: {
        Style: { Failure: "failure", Animated: "animated", Success: "success" },
      },
      showToast: async (value) => {
        const toast = { ...value };
        toasts.push(toast);
        return toast;
      },
    };
  if (name === "./runtime")
    return {
      binaries: { theme: "native-theme" },
      useQuery: () => query,
      run: (file, argv) => {
        calls.push([file, [...argv]]);
        return new Promise((yes, no) => {
          resolve = yes;
          reject = no;
        });
      },
    };
  return originalLoad.call(this, name, ...args);
};

const Command = require(process.argv[2]).default;
const store = { hooks: [], index: 0 };
function render() {
  active = store;
  store.index = 0;
  const tree = Command();
  active = null;
  return tree;
}
// Presentational components are expanded the way the launcher would expand
// them, so an assertion sees the rows and actions a person actually reads.
function* walk(node) {
  if (!node || typeof node !== "object") return;
  if (Array.isArray(node)) {
    for (const child of node) yield* walk(child);
    return;
  }
  if (typeof node.type === "function" && !node.type.host) {
    const nested = { hooks: [], index: 0 };
    const previous = active;
    active = nested;
    const rendered = node.type(node.props);
    active = previous;
    yield* walk(rendered);
    return;
  }
  yield node;
  // A row's preview and its metadata panel are props rather than children.
  yield* walk(node.props?.detail);
  yield* walk(node.props?.metadata);
  yield* walk(node.props?.children);
}
const elements = (tree, type) =>
  [...walk(tree)].filter((node) => node.type === type);
const rows = (tree) => elements(tree, List.Item);
const row = (tree, title) => {
  const found = rows(tree).find((node) => node.props.title === title);
  assert(found, `missing row: ${title}`);
  return found;
};
const actions = (node) => [...walk(node.props.actions)];
const action = (node, title) => {
  const found = actions(node).find((entry) => entry.props?.title === title);
  assert(found, `missing action: ${title}`);
  return found.props;
};

// The picker awaits its own toast before it runs anything, so the fixture lets
// pending microtasks settle before it answers as the native helper.
const flush = () => new Promise((yes) => setImmediate(yes));
// A fixture that stops awaiting halfway leaves an empty event loop and exits
// zero, which would read as a pass; the run has to reach its own end.
let finished = false;
process.on("exit", (code) => {
  if (code === 0 && !finished) {
    console.error("the theme picker fixture never finished");
    process.exitCode = 1;
  }
});

const palette = {};
for (const key of [
  "base00",
  "base01",
  "base02",
  "base03",
  "base04",
  "base05",
  "base06",
  "base07",
  "base08",
  "base09",
  "base0A",
  "base0B",
  "base0C",
  "base0D",
  "base0E",
  "base0F",
])
  palette[key] = "#123456";
const theme = (id, name, mode) => ({
  id,
  name,
  mode,
  palette,
  base: "#1e1e2e",
  surface: "#313244",
  text: "#cdd6f4",
  accent: "#b4befe",
  red: "#f38ba8",
  green: "#a6e3a1",
  yellow: "#f9e2af",
});
const mocha = theme("catppuccin-mocha", "Catppuccin Mocha", "dark");
const nord = theme("nord", "Nord", "dark");
const dawn = theme("rose-pine-dawn", "Rosé Pine Dawn", "light");
query.data = { current: mocha.id, themes: [mocha, nord, dawn] };

(async () => {
  let tree = render();
  assert.equal(calls.length, 0, "Rendering starts no subprocess");
  assert.equal(tree.props.isShowingDetail, true);

  // The current theme leads its own section; everything else is grouped by
  // what it is, so light and dark are never mixed into one run of rows.
  const sections = elements(tree, List.Section);
  assert.deepEqual(
    sections.map((section) => section.props.title),
    ["Current", "Dark", "Light"],
  );
  assert.deepEqual(
    sections.map((section) => rows(section).map((entry) => entry.props.title)),
    [["Catppuccin Mocha"], ["Nord"], ["Rosé Pine Dawn"]],
  );

  // What each row says about itself: the selection is tagged, and every other
  // row states whether applying it makes the desktop light or dark.
  const current = row(tree, "Catppuccin Mocha");
  assert.deepEqual(current.props.accessories, [
    { tag: { value: "Current", color: "Green" } },
  ]);
  assert.deepEqual(row(tree, "Nord").props.accessories, [
    { icon: "Moon", tooltip: "Dark theme" },
  ]);
  assert.deepEqual(row(tree, "Rosé Pine Dawn").props.accessories, [
    { icon: "Sun", tooltip: "Light theme" },
  ]);
  assert.equal(current.props.icon.tintColor, mocha.accent);
  for (const word of ["catppuccin", "mocha", "dark"])
    assert(
      current.props.keywords.includes(word),
      `a preset is searchable by ${word}`,
    );

  // The preview names its palette roles rather than showing six anonymous
  // swatches, and says what a selection reaches immediately.
  const detail = [...walk(current.props.detail)];
  const labels = detail
    .filter((node) => node.type === Metadata.Label)
    .map((node) => node.props.title);
  assert.deepEqual(labels, ["Mode", "Background", "Surface", "Foreground"]);
  const swatches = detail.filter((node) => node.type === Metadata.TagList.Item);
  assert.deepEqual(
    swatches.map((node) => node.props.color),
    [mocha.accent, mocha.red, mocha.green, mocha.yellow],
  );
  assert.match(current.props.detail.props.markdown, /# Catppuccin Mocha/);
  assert.match(current.props.detail.props.markdown, /Applied now/);
  assert.doesNotMatch(
    row(tree, "Nord").props.detail.props.markdown,
    /Applied now/,
    "only the saved selection reports itself as applied",
  );

  // Ctrl, not Cmd: the shared shortcut table is this desktop's, and the same
  // refresh is reachable from a row, the empty view and the failed catalog.
  assert.deepEqual(action(current, "Refresh Themes").shortcut, {
    modifiers: ["ctrl"],
    key: "r",
  });
  assert.deepEqual(action(current, "Copy Base16 Palette").shortcut, {
    modifiers: ["ctrl", "shift"],
    key: "c",
  });
  assert.deepEqual(
    JSON.parse(action(current, "Copy Base16 Palette").content),
    palette,
  );
  assert.equal(
    action(row(tree, "Nord"), "Apply Theme").title,
    "Apply Theme",
    "a theme that is not applied offers to apply it",
  );
  assert(
    actions(current).some(
      (entry) => entry.props?.title === "Reapply Theme",
    ),
    "the applied theme offers to publish and reload it again",
  );

  // Applying is one request, reported while it runs.
  const apply = action(row(tree, "Nord"), "Apply Theme").onAction;
  const first = apply();
  await apply();
  await flush();
  assert.deepEqual(
    calls,
    [["native-theme", ["set", "nord"]]],
    "Duplicate applies coalesce",
  );
  assert.equal(toasts.length, 1);
  assert.equal(toasts[0].style, "animated");
  assert.match(toasts[0].title, /Applying Nord/);
  assert.equal(render().props.isLoading, true);
  resolve(JSON.stringify({ id: "nord", pending: [] }));
  await first;
  assert.equal(refreshes, 1);
  assert.equal(render().props.isLoading, false);
  assert.equal(toasts[0].style, "success");
  assert.equal(toasts[0].message, "Applied");

  // A theme that is saved but not everywhere live says so, and never claims
  // to have failed.
  const second = apply();
  await flush();
  resolve(JSON.stringify({ id: "nord", pending: ["Ghostty", "tmux"] }));
  await second;
  assert.equal(toasts.at(-1).style, "success");
  assert.match(toasts.at(-1).message, /still to reload: Ghostty, tmux/);

  // A failed request keeps the child's own output out of the interface.
  const third = apply();
  await flush();
  reject(Error("private child error"));
  await third;
  assert.equal(toasts.at(-1).style, "failure");
  assert(!JSON.stringify(toasts).includes("private child error"));

  // Nothing to show, for either reason, still offers the way out of it.
  const empty = elements(tree, List.EmptyView)[0];
  assert.equal(empty.props.title, "No matching theme");
  assert(actions(empty).some((entry) => entry.props?.title === "Refresh Themes"));

  query.error = "private query error";
  query.data = undefined;
  tree = render();
  assert.equal(rows(tree).length, 1, "a failed catalog lists no themes");
  assert.equal(tree.props.isShowingDetail, false);
  const unavailable = rows(tree)[0];
  assert.equal(unavailable.props.title, "Themes unavailable");
  assert(!JSON.stringify(unavailable.props).includes("private query error"));
  assert(
    actions(unavailable).some((entry) => entry.props?.title === "Refresh Themes"),
  );

  finished = true;
  console.log(
    "Theme picker sections, previews, shared shortcuts, single-flight applies, reload reporting and retry passed",
  );
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
