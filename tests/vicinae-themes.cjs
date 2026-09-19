// Drive the real picker against controlled native replies.
const assert = require("node:assert/strict");
const Module = require("node:module");
const original = Module._load;
const calls = [],
  toasts = [],
  hooks = [];
let index = 0,
  refreshes = 0,
  resolve,
  reject;
const theme = {
  id: "catppuccin-mocha",
  name: "Catppuccin Mocha",
  mode: "dark",
  base: "#1e1e2e",
  text: "#cdd6f4",
  accent: "#b4befe",
  red: "#f38ba8",
  green: "#a6e3a1",
  yellow: "#f9e2af",
};
const query = {
  data: { current: theme.id, themes: [theme] },
  loading: false,
  refresh: () => refreshes++,
};
const Item = Object.assign("item", {});
function component(name) {
  return Object.assign(function () {}, { tag: name });
}
const Detail = component("detail");
const Metadata = component("metadata");
Metadata.TagList = component("palette");
Metadata.TagList.Item = component("swatch");
Detail.Metadata = Metadata;
const List = component("list");
List.Item = component("item");
List.Item.Detail = Detail;
List.EmptyView = component("empty");
const Action = component("action");
Module._load = function (name, ...args) {
  if (name === "react")
    return {
      createElement: (type, props, ...children) => ({
        type,
        props: { ...props, children: children.flat(Infinity) },
      }),
      useRef: (initial) => (hooks[index++] ??= { current: initial }),
      useState(initial) {
        const slot = index++;
        hooks[slot] ??= { value: initial };
        return [hooks[slot].value, (value) => (hooks[slot].value = value)];
      },
    };
  if (name === "@raycast/api")
    return {
      Action,
      ActionPanel: component("actions"),
      List,
      Icon: new Proxy({}, { get: (_, key) => key }),
      Color: {},
      Toast: { Style: { Failure: "failure" } },
      showToast: async (value) => toasts.push(value),
    };
  if (name === "./runtime")
    return {
      binaries: { theme: "native-theme" },
      useQuery: () => query,
      run: (file, args) => {
        calls.push([file, args]);
        return new Promise((yes, no) => {
          resolve = yes;
          reject = no;
        });
      },
    };
  return original.call(this, name, ...args);
};
const Command = require(process.argv[2]).default;
function render() {
  index = 0;
  return Command();
}
function all(node, tag) {
  if (!node || typeof node !== "object") return [];
  return [
    ...(node.type?.tag === tag ? [node] : []),
    ...(node.props.children ?? []).flatMap((child) => all(child, tag)),
  ];
}
(async () => {
  let tree = render();
  assert.equal(calls.length, 0, "Rendering starts no subprocess");
  const row = all(tree, "item")[0];
  assert.equal(row.props.id, theme.id);
  assert.equal(row.props.accessories[1].tooltip, "Current theme");
  assert.equal(all(row.props.detail.props.metadata, "swatch").length, 6);
  const apply = all(row.props.actions, "action")[0].props.onAction;
  const first = apply();
  await apply();
  assert.deepEqual(
    calls,
    [["native-theme", ["set", theme.id]]],
    "Duplicate applies coalesce",
  );
  assert.equal(render().props.isLoading, true);
  resolve(JSON.stringify({ id: theme.id, pending: [] }));
  await first;
  assert.equal(refreshes, 1);
  assert.equal(render().props.isLoading, false);
  assert.equal(toasts.length, 0, "Successful selection is visible in the row");
  const second = apply();
  resolve(JSON.stringify({ id: theme.id, pending: ["Ghostty"] }));
  await second;
  assert.equal(toasts.at(-1).message, "Ghostty");
  const third = apply();
  reject(Error("private child error"));
  await third;
  assert(!JSON.stringify(toasts).includes("private child error"));
  query.error = "private query error";
  query.data = undefined;
  tree = render();
  const empty = all(tree, "empty")[0];
  assert.equal(empty.props.title, "Themes unavailable");
  assert(all(empty.props.actions, "action").length);
  console.log(
    "Theme picker previews, selection, duplicate actions, reload failures and retry passed",
  );
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
