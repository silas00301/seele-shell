import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const {
  escapeMarkdown,
  formatPackageDiff,
  generationPath,
  markActiveGenerations,
  parseGenerations,
  switchGenerationArguments,
} = await import(pathToFileURL(process.argv[2]));

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
  "/nix/store/running-system",
  new Map([
    [8, "/nix/store/old-system"],
    [12, "/nix/store/running-system"],
  ]),
);
assert.equal(marked[0].active, true);
assert.equal(marked[0].runningStorePath, "/nix/store/running-system");
assert.equal(marked[1].active, false);
assert.equal(marked[1].active, false, "profile current flag must be ignored");

assert.equal(generationPath(42), "/nix/var/nix/profiles/system-42-link");
assert.deepEqual(switchGenerationArguments(42), ["42"]);
for (const value of [0, -1, 1.5, "4", "4; reboot", NaN, Infinity]) {
  assert.throws(() => generationPath(value));
  assert.throws(() => switchGenerationArguments(value));
}
assert.throws(() =>
  parseGenerations(JSON.stringify([{ generation: 1 }, { generation: 1 }])),
);

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
assert.match(interfaceSource, /selected\.storePath !== generation\.storePath/);
assert.match(
  interfaceSource,
  /selected\.runningStorePath !== generation\.runningStorePath/,
);
assert.match(interfaceSource, /binaries\.run0/);
assert.doesNotMatch(
  interfaceSource,
  /garbage.collect|collect.garbage|delete.generation/i,
);
assert.match(packageSource, /\^\[1-9\]\[0-9\]\*\$/);
assert.match(packageSource, /\/run\/current-system\/sw\/bin\/nix-env/);
assert.match(packageSource, /--switch-generation/);
assert.match(packageSource, /switch-to-configuration" switch/);
assert.doesNotMatch(packageSource, /NIXOS_NO_CHECK/);

console.log("Vicinae NixOS generation parsing and diff safety tests passed");
