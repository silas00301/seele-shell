const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");
const context = vm.createContext({});
vm.runInContext(fs.readFileSync(process.argv[2], "utf8"), context);
let state = context.initial();
const ready = { state: "ready", host: "github.com", viewer: "fixture", updatedAt: "2026-09-08T00:00:00Z", reviews: [{url:"https://github.com/org/repo/pull/1"}], authored: [], reviewTotal: 1, authoredTotal: 0 };
state = context.receive(state, ready);
assert.equal(state.stale, false);
state = context.receive(state, {state:"error",message:"Offline"});
assert.equal(state.reviews.length, 1, "transient failures preserve visible data");
assert.equal(state.stale, true);
state = context.receive(state, {state:"auth-required",message:"Sign in"});
assert.equal(state.reviews.length, 0, "expired credentials clear cached private data");
assert.equal(state.viewer, "");
assert.equal(context.receive(context.initial(), null).state, "error");
assert.equal(context.due(1000, 5999, "ready", true), false);
assert.equal(context.due(1000, 6000, "ready", true), true);
assert.equal(context.due(1000, 60999, "ready", false), false);
assert.equal(context.due(1000, 61000, "ready", false), true);
assert.equal(context.due(1000, 61000, "rate-limited", false), false);
assert.equal(context.due(1000, 301000, "rate-limited", false), true);
assert.equal(context.safeUrl("https://github.com/org/repo/pull/42", "github.com"), true);
for (const url of ["https://github.com.evil/org/repo/pull/42", "https://user@github.com/org/repo/pull/42", "javascript:alert(1)", "https://github.com/org/repo/pull/42?secret"]) assert.equal(context.safeUrl(url, "github.com"), false);
assert.equal(context.checksLabel("UNKNOWN"), "No check status");
assert.equal(context.reviewLabel({draft:true,review:"APPROVED"}), "Draft");
const store = fs.readFileSync(process.argv[3], "utf8");
assert.match(store, /running: store.active/);
assert.match(store, /requestPending \|\| !GitHub.due/);
assert.match(store, /if \(!GitHub.safeUrl\(url, snapshot.host\)\) return false/);
console.log("GitHub snapshot, stale/auth transitions, refresh bounds and URL checks passed");

// Execute the actual QML methods with small Process/Timer doubles.
function qmlMethod(name) {
  const start = store.indexOf("  function " + name + "(");
  assert.ok(start >= 0, name);
  const brace = store.indexOf("{", start);
  let depth = 1, end = brace + 1;
  while (depth && end < store.length) { if (store[end] === "{") depth++; if (store[end] === "}") depth--; end++; }
  return store.slice(start, end);
}
let now = 100000;
const lifecycle = vm.createContext({
  healthSuccess: 0, healthPublished(){},
  GitHub: context, Date: {now: () => now}, active: true,
  snapshot: context.initial(), lastAttempt: 0, received: false,
  requestPending: false, requestId: 0, worker: null,
  cooldown: {restart() {}}, watchdog: {restart() {}, stop() {}},
  workerFactory: {createObject(parent, props) { return {...props, running: false, destroyed: false, destroy() {this.destroyed = true;}}; }}
});
lifecycle.store = lifecycle;
for (const name of ["refresh", "accept", "finish"]) vm.runInContext(qmlMethod(name), lifecycle);
assert.equal(lifecycle.refresh(false), true);
const first = lifecycle.worker;
assert.equal(lifecycle.requestPending, true);
first.running = false; // Exit has occurred but its completion callback has not run yet.
now += 60000;
assert.equal(lifecycle.refresh(true), false, "pending completion prevents request overlap");
lifecycle.accept(first.token, JSON.stringify(ready));
lifecycle.finish(first.token, "");
assert.equal(lifecycle.requestPending, false);
assert.equal(first.destroyed, true);
assert.equal(lifecycle.snapshot.state, "ready");
assert.equal(lifecycle.refresh(false), true);
const second = lifecycle.worker;
lifecycle.finish(first.token, "old failure");
lifecycle.accept(first.token, JSON.stringify({state:"auth-required"}));
assert.equal(lifecycle.requestPending, true, "old callbacks leave next request pending");
assert.equal(lifecycle.snapshot.state, "ready");
lifecycle.finish(second.token, "GitHub refresh timed out. Try again.");
assert.equal(second.destroyed, true);
assert.equal(lifecycle.requestPending, false, "watchdog recovers a process that never starts/exits");
assert.equal(lifecycle.snapshot.stale, true);
now += 60000;
lifecycle.active = false;
assert.equal(lifecycle.refresh(false), false, "closed panels never refresh automatically");
assert.equal(lifecycle.refresh(true), true);
lifecycle.finish(lifecycle.requestId, "");
assert.equal(lifecycle.snapshot.message, "GitHub refresh could not start.");
now += 60000;
lifecycle.workerFactory.createObject = () => null;
assert.equal(lifecycle.refresh(true), false);
assert.equal(lifecycle.requestPending, false);
console.log("GitHub QML lifecycle, watchdog, closed-panel and stale-callback checks passed");

const panel = fs.readFileSync(require("node:path").join(require("node:path").dirname(process.argv[2]), "shell.qml"), "utf8").split("id: githubSurface")[1];
const panelKeys = panel.match(/Keys.onPressed: event => \{([^]*?)\n        }/)[1];
let opened = 0, moved = 0;
const keyContext = vm.createContext({
  Qt: {Key_Escape:1,Key_J:2,Key_K:3,Key_Down:4,Key_Up:5,Key_Tab:6,Key_Backtab:7,Key_R:8,Key_Return:9,Key_Enter:10,ControlModifier:1,AltModifier:2,MetaModifier:4},
  githubList: {currentIndex:0,incrementCurrentIndex(){moved++},decrementCurrentIndex(){moved--}},
  githubWindow: {entries:[{url:"https://github.com/org/repo/pull/1"}]},
  githubStore: {openPull(){opened++}},
});
vm.runInContext("function press(event) {" + panelKeys + "}", keyContext);
keyContext.press({key:9,modifiers:0,isAutoRepeat:false});
keyContext.press({key:9,modifiers:0,isAutoRepeat:true});
keyContext.press({key:9,modifiers:1,isAutoRepeat:false});
keyContext.press({key:4,modifiers:0,isAutoRepeat:true});
assert.equal(opened, 1, "held Enter and modifier chords cannot repeatedly open browser tabs");
assert.equal(moved, 1, "holding navigation keys continues moving through results");
console.log("GitHub browser keyboard intent checks passed");
