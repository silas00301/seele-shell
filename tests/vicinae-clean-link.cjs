const assert = require("node:assert/strict");
const Module = require("node:module");
const { EventEmitter } = require("node:events");
const originalLoad = Module._load;
let hooks = [], effects = [], hookIndex = 0;
let readResult, reads = 0, copyResult;
const children = [], copies = [], toasts = [];
const Action = () => {};
const Detail = () => {};
const react = {
  createElement: (type, props, ...children) => ({ type, props: { ...props, children } }),
  useState(initial) {
    const i = hookIndex++;
    hooks[i] ??= { value: initial };
    return [hooks[i].value, (value) => { hooks[i].value = value; }];
  },
  useRef(initial) { const i = hookIndex++; return hooks[i] ??= { current: initial }; },
  useEffect(fn) { const i = hookIndex++; if (!hooks[i]) { hooks[i] = {}; effects.push(() => { hooks[i].cleanup = fn(); }); } },
};
Module._load = function(name, ...args) {
  if (name === "react") return react;
  if (name === "@raycast/api") return {
    Action, ActionPanel: "actions", Detail, Icon: { CopyClipboard: "clipboard" },
    Clipboard: {
      readText: () => { reads++; return readResult; },
      copy: async (text) => { copies.push(text); await copyResult; },
    },
    showToast: async (toast) => { toasts.push(toast); },
    Toast: { Style: { Success: "success", Failure: "failure" } },
  };
  if (name === "./runtime") return { binaries: { control: "/native/seele-control" } };
  if (name === "node:child_process") return {
    execFile(file, argv, options, callback) {
      const stdin = new EventEmitter();
      const entry = { file, argv, options, callback, stdin };
      stdin.end = (input) => { entry.input = input; };
      children.push(entry);
      return { stdin };
    },
  };
  return originalLoad.call(this, name, ...args);
};
const { default: Command, loadPreview } = require(process.argv[2]);
const settle = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => { let resolve, reject; const promise = new Promise((a,b) => { resolve=a; reject=b; }); return {promise,resolve,reject}; };
const preview = { original: "https://a.test/?utm_source=private", cleaned: "https://a.test/", removed: ["utm_source"], status: "cleaned", message: "Review", markdown: "Original and cleaned preview" };
function render() { hookIndex=0; const tree=Command(); for(const effect of effects.splice(0)) effect(); return tree; }
function action(tree) { return tree.props.actions?.props.children[0]?.props.onAction; }
function unmount() { for(const hook of hooks) hook?.cleanup?.(); }
function reset(read=Promise.resolve(preview.original)) { unmount(); hooks=[]; effects=[]; readResult=read; copyResult=undefined; }
(async () => {
  reset();
  assert.equal(action(render()), undefined);
  await settle();
  const child = children.at(-1);
  assert.equal(child.file, "/native/seele-control");
  assert.deepEqual(child.argv, ["vicinae-clean-link"]);
  assert.equal(child.input, preview.original);
  assert.equal(child.options.timeout, 5000);
  assert.equal(child.options.maxBuffer, 256*1024);
  child.callback(null, JSON.stringify(preview)); await settle();
  const tree=render();
  assert.equal(tree.props.markdown, preview.markdown);
  assert.equal(copies.length, 0, "preview must not write clipboard");
  const readsBefore=reads, childrenBefore=children.length;
  render(); render(); await settle();
  assert.equal(reads, readsBefore); assert.equal(children.length, childrenBefore);
  const pending=deferred(); copyResult=pending.promise;
  const copy=action(tree); const first=copy(); await copy();
  assert.deepEqual(copies, [preview.cleaned], "duplicate copies coalesce");
  pending.resolve(); await first;
  unmount(); await copy(); assert.equal(copies.length, 1);
  assert.equal(child.options.signal.aborted, true);

  for (const status of ["protected", "unchanged", "ambiguous"]) {
    reset(); render(); await settle();
    children.at(-1).callback(null, JSON.stringify({...preview, status, cleaned: preview.original})); await settle();
    const tree=render();
    assert.equal(tree.props.actions.props.children[0].props.title, "Copy Unchanged Link");
    await action(tree)(); assert.equal(copies.at(-1), preview.original);
  }
  reset(); render(); await settle(); children.at(-1).callback(new Error("private-secret subprocess output")); await settle();
  const failed=render(); assert.equal(action(failed), undefined);
  assert(!JSON.stringify(failed).includes("private-secret"));
  reset(Promise.reject(new Error("private-secret clipboard error"))); render(); await settle();
  assert.equal(action(render()), undefined);
  assert(!JSON.stringify(render()).includes("private-secret"));
  reset(); render(); await settle(); children.at(-1).callback(null, "not json private-secret"); await settle();
  assert.equal(action(render()), undefined);

  const read=deferred(); reset(read.promise); render(); unmount();
  const count=children.length; read.resolve(preview.original); await settle();
  assert.equal(children.length,count,"late clipboard read cannot spawn after unmount");
  reset(); render(); await settle(); const stale=children.at(-1); unmount();
  stale.callback(null, JSON.stringify(preview)); await settle(); assert.equal(action(render()),undefined);
  const aborted = new AbortController(); aborted.abort();
  await assert.rejects(loadPreview(preview.original,aborted.signal));
  assert.equal(children.length,count+1);
  const beforeLarge=children.length;
  await assert.rejects(loadPreview("x".repeat(16385),new AbortController().signal));
  await assert.rejects(loadPreview("ü".repeat(8193),new AbortController().signal));
  assert.equal(children.length,beforeLarge,"oversized UTF-8 input must never enter child stdin");
  reset(); render(); await settle(); children.at(-1).callback(null,JSON.stringify(preview)); await settle();
  const failure=deferred(); copyResult=failure.promise;
  const failingCopy=action(render())(); failure.reject(new Error("private-secret copy error")); await failingCopy;
  assert.equal(toasts.at(-1).title,"Could not copy link");
  const ep=loadPreview(preview.original,new AbortController().signal);
  children.at(-1).stdin.emit("error",new Error("private-secret EPIPE"));
  await assert.rejects(ep,/Link preview unavailable/);
  assert(!JSON.stringify(toasts).includes("private"));
  console.log("Clean Link preview, stdin-only handoff, explicit copy, cancellation, and failure privacy passed");
})().catch(error=> {console.error(error);process.exitCode=1;});
