import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const { nativeFunctions } = createRequire(import.meta.url)(
  "./native-functions.cjs",
);
const native = nativeFunctions().Functions;
const call = (name, ...args) => native.call(`vicinae.${name}`, args);
const escapeMarkdown = (...args) => call("escapeMarkdown", ...args);
const formatPackageDiff = (...args) => call("formatPackageDiff", ...args);
const generationPath = (...args) => call("generationPath", ...args);
const isStorePath = (...args) => call("isStorePath", ...args);
const parseGenerations = (...args) => call("parseGenerations", ...args);
const switchGenerationArguments = (...args) =>
  call("switchGenerationArguments", ...args);
const markActiveGenerations = (rows, running, paths) =>
  call("markActiveGenerations", rows, running, Object.fromEntries(paths));

const running = "/nix/store/11111111111111111111111111111111-running-system";
const target = "/nix/store/00000000000000000000000000000000-old-system";
const generations = parseGenerations(
  JSON.stringify([
    {
      generation: 8,
      date: "2026-04-12T10:11:12Z",
      nixosVersion: "26.05",
      kernelVersion: "6.18.1",
      configurationRevision: "abc123",
      specialisations: ["gaming"],
      current: true,
    },
    {
      generation: 12,
      date: "2026-04-13T10:11:12Z",
      nixosVersion: "26.05",
      kernelVersion: "6.18.2",
      configurationRevision: "def456",
      specialisations: [],
      current: false,
    },
  ]),
);
assert.deepEqual(
  generations.map((generation) => generation.generation),
  [12, 8],
);
assert.equal(generations[1].active, false);
assert.equal(generations[1].profilePath, "/nix/var/nix/profiles/system-8-link");

const marked = markActiveGenerations(
  generations,
  running,
  new Map([
    [8, target],
    [12, running],
  ]),
);
assert.equal(marked[0].active, true);
assert.equal(marked[0].runningStorePath, running);
assert.equal(marked[1].active, false);
assert.equal(marked[1].active, false, "profile current flag must be ignored");

assert.equal(generationPath(42), "/nix/var/nix/profiles/system-42-link");
assert.deepEqual(switchGenerationArguments(42, target, running), [
  "42",
  target.slice(11),
  running.slice(11),
]);
for (const value of [0, -1, 1.5, "4", "4; reboot", NaN, Infinity]) {
  assert.throws(() => generationPath(value));
  assert.throws(() => switchGenerationArguments(value, target, running));
}
assert.throws(() =>
  parseGenerations(JSON.stringify([{ generation: 1 }, { generation: 1 }])),
);

for (const path of [
  "/tmp/system",
  target + "/bin/thing",
  "/nix/store/short-hash",
  "/nix/store/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-system",
  target + "\n",
]) {
  assert.equal(isStorePath(path), false);
  assert.throws(() => switchGenerationArguments(42, path, running));
  assert.throws(() => switchGenerationArguments(42, target, path));
}
assert.throws(() => parseGenerations(" ".repeat(4 * 1024 * 1024 + 1)));
assert.throws(() =>
  parseGenerations(
    JSON.stringify(
      Array.from({ length: 4097 }, (_, generation) => ({
        generation: generation + 1,
      })),
    ),
  ),
);
assert.throws(() =>
  parseGenerations(JSON.stringify([{ generation: 1, date: "x".repeat(4097) }])),
);
assert.throws(() =>
  parseGenerations(
    JSON.stringify([{ generation: 1, specialisations: Array(65).fill("a") }]),
  ),
);
// The four-worker filesystem resolver and cancellation contract are exercised
// in tools::vicinae native fixtures, rather than a JavaScript worker pool.

const diff = formatPackageDiff("\u001b[31m- old\u001b[0m\n```\n+ new\u0000");
assert(!diff.includes("\u001b"));
assert(!diff.includes("\u0000"));
assert(diff.split("\n").every((line) => line.startsWith("    ")));
assert.match(formatPackageDiff("abcdef", 3), /diff truncated/);
assert.equal(formatPackageDiff("\n"), "_No package changes reported._");
assert.equal(escapeMarkdown("kernel *preview*"), "kernel \\*preview\\*");

const interfaceSource = readFileSync(process.argv[3], "utf8");
const packageSource = readFileSync(process.argv[4], "utf8");
assert.match(interfaceSource, /confirmAlert\(/);
assert.match(interfaceSource, /Alert\.ActionStyle\.Destructive/);
assert.match(interfaceSource, /Action\.Style\.Destructive/);
assert.match(interfaceSource, /vicinae-generation-check/);
assert.match(interfaceSource, /generation\.switchArguments/);
assert.match(interfaceSource, /binaries\.run0/);
assert.doesNotMatch(
  interfaceSource,
  /garbage.collect|collect.garbage|delete.generation/i,
);
assert.match(packageSource, /name = "repo-tools"/);
assert.match(packageSource, /generationSwitch}\/bin\/seele-switch-generation/);
assert.doesNotMatch(packageSource, /NIXOS_NO_CHECK/);
// Privileged argument, path and race behavior is exercised by the native
// repo-tools generation fixtures, without activating this host.

console.log("Vicinae NixOS generation parsing and diff safety tests passed");
