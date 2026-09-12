const assert = require("node:assert/strict");
const { Worker } = require("node:worker_threads");
const addon = require(process.argv[2]);
const call = (operation, ...args) =>
  JSON.parse(addon.evaluate(JSON.stringify({ operation, arguments: args })));
for (const input of [undefined, null, 1, {}, [], Buffer.from("{}")])
  assert.throws(() => addon.evaluate(input), TypeError);
assert.throws(() => addon.evaluate("{}", "extra"), TypeError);
assert.throws(
  () => addon.evaluate("é".repeat(8 * 1024 * 1024 + 1)),
  RangeError,
);
for (const input of [
  "",
  "{}",
  "\0",
  '{"operation":"fixture.echo","arguments":[],"extra":true}',
  '{"operation":"fixture.echo","arguments":[' +
    "[".repeat(200) +
    "0" +
    "]".repeat(200) +
    "]}",
]) {
  assert.equal(JSON.parse(addon.evaluate(input)).ok, false);
}
for (const value of ["", "🦀\0🌸 e\u0301", [true, null, { nested: "value" }]])
  assert.deepEqual(call("fixture.echo", value), { ok: true, value });
assert.equal(call("not.aFunction").ok, false);
const retained = call("fixture.echo", "retained value");
for (let i = 0; i < 20_000; i++)
  assert.equal(call("fixture.echo", `copy ${i}`).value, `copy ${i}`);
global.gc?.();
assert.deepEqual(
  retained,
  { ok: true, value: "retained value" },
  "results must own copied bytes after Rust allocations are freed",
);
Promise.all(
  Array.from(
    { length: 4 },
    () =>
      new Promise((resolve, reject) => {
        const worker = new Worker(
          `const{parentPort,workerData}=require('node:worker_threads');const addon=require(workerData);for(let i=0;i<1000;i++){const value=JSON.parse(addon.evaluate(JSON.stringify({operation:'fixture.echo',arguments:[i]})));if(value.value!==i)throw Error('isolation');}parentPort.postMessage(true);`,
          { eval: true, workerData: process.argv[2] },
        );
        worker.once("message", resolve);
        worker.once("error", reject);
        worker.once("exit", (code) => {
          if (code) reject(new Error(`worker ${code}`));
        });
      }),
  ),
)
  .then(() =>
    console.log(
      "Node native ABI bounds, Unicode, ownership and worker isolation checks passed",
    ),
  )
  .catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
