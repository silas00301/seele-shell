const assert = require("node:assert/strict");
const fs = require("node:fs");

const store = fs.readFileSync(process.argv[2], "utf8");
const shell = fs.readFileSync(process.argv[3], "utf8");
const notifications = fs.readFileSync(process.argv[4], "utf8");

assert.match(shell, /id: prFocusStore/);
assert.match(shell, /id: prFocusPin/);
assert.match(shell, /id: prFocusEnter/);
assert.match(shell, /prFocusStore\.exit\(\)/);
assert.match(shell, /function prFocus\(action: string\): string/);
assert.match(shell, /setPrFocus\(active, Date\.now\(\) \/ 1000\)/);
assert.match(notifications, /state\.setPrFocus=function\(enabled,timestamp\)/);
assert.match(store, /command: \["seele-github-status", "focus", target\]/);
assert.match(store, /SEELE_FOCUS_PULL/);
assert.doesNotMatch(store, /notify-send|send_message|create_draft/);

function qmlMethod(name) {
  const start = store.indexOf("  function " + name + "(");
  assert.ok(start >= 0, name);
  const brace = store.indexOf("{", start);
  let depth = 1, end = brace + 1;
  while (depth && end < store.length) {
    if (store[end] === "{") depth++;
    if (store[end] === "}") depth--;
    end++;
  }
  return store.slice(start, end);
}

const ready = {
  state: "ready",
  checks: "PENDING",
  comment: "latest inline comment",
  commentAuthor: "reviewer",
  repository: "silas00301/seele",
  number: 183,
  title: "Return to the previous workspace",
  url: "https://github.com/silas00301/seele/pull/183"
};

const life = {
  configured: false,
  active: false,
  url: "",
  host: "github.com",
  snapshot: { state: "idle", checks: "", comment: "", number: 0 },
  lastAttempt: 0,
  received: false,
  requestPending: false,
  requestId: 0,
  worker: null,
  Date: { now: () => 100000 },
  JSON,
  watchdog: { stop() { life.stopped = true; }, restart() { life.restarted = true; } },
  workerFactory: {
    createObject(_parent, props) {
      return { ...props, running: false, destroyed: false, destroy() { this.destroyed = true; } };
    }
  }
};
life.store = life;
const vm = require("node:vm");
const context = vm.createContext(life);
for (const name of ["idleSnapshot", "enter", "exit", "toggle", "refresh", "accept", "finish"]) {
  vm.runInContext(qmlMethod(name), context);
}

assert.equal(context.enter(), false, "focus cannot start without the configured pull request");
assert.equal(context.active, false);

context.configured = true;
context.url = ready.url;
assert.equal(context.enter(), true);
assert.equal(context.active, true);
assert.equal(context.requestPending, true);
const worker = context.worker;
assert.equal(worker.target, ready.url);
assert.equal(context.enter(), true, "entering again stays on the same pull request");
assert.equal(context.worker, worker, "a second enter does not start another refresh");

context.accept(worker.token, JSON.stringify(ready));
context.finish(worker.token, "");
assert.equal(context.snapshot.checks, "PENDING");
assert.equal(context.snapshot.comment, "latest inline comment");
assert.equal(context.requestPending, false);
assert.equal(worker.destroyed, true);

assert.equal(context.exit(), true);
assert.equal(context.active, false);
assert.equal(context.snapshot.state, "idle");
assert.equal(context.snapshot.comment, "");
assert.equal(context.snapshot.checks, "");
context.accept(worker.token, JSON.stringify({ state: "ready", checks: "FAILURE", comment: "stale" }));
assert.equal(context.snapshot.comment, "", "a late refresh cannot restore the pin");

context.configured = true;
assert.equal(context.toggle(), true);
assert.equal(context.active, true);
assert.equal(context.toggle(), true);
assert.equal(context.active, false);
assert.equal(context.snapshot.state, "idle");

console.log("Pull request focus enter, exit, pin clearing and stale replies passed");
