const PROFILE_ROOT = "/nix/var/nix/profiles";

export const runningSystemPath = "/run/current-system";

function generationNumber(value) {
  if (!Number.isSafeInteger(value) || value < 1)
    throw new TypeError("Generation must be a positive integer");
  return value;
}

function oneLine(value, fallback = "Unknown") {
  if (typeof value !== "string") return fallback;
  const text = value
    .replace(/[\u0000-\u001f\u007f-\u009f]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  return text || fallback;
}

export function generationPath(generation) {
  return `${PROFILE_ROOT}/system-${generationNumber(generation)}-link`;
}

export function switchGenerationArguments(generation) {
  return [String(generationNumber(generation))];
}

export function parseGenerations(payload) {
  const rows = JSON.parse(payload);
  if (!Array.isArray(rows))
    throw new TypeError("Generation list must be an array");

  const seen = new Set();
  const generations = rows.map((row) => {
    if (!row || typeof row !== "object")
      throw new TypeError("Generation entry must be an object");
    const generation = generationNumber(row.generation);
    if (seen.has(generation))
      throw new TypeError("Generation numbers must be unique");
    seen.add(generation);

    const date = oneLine(row.date);
    const specialisations = Array.isArray(row.specialisations)
      ? row.specialisations.map((value) => oneLine(value)).filter(Boolean)
      : [];
    return {
      generation,
      date,
      nixosVersion: oneLine(row.nixosVersion),
      kernelVersion: oneLine(row.kernelVersion),
      configurationRevision: oneLine(row.configurationRevision, ""),
      specialisations,
      profilePath: generationPath(generation),
      storePath: undefined,
      active: false,
    };
  });
  return generations.sort((left, right) => right.generation - left.generation);
}

export function markActiveGenerations(
  generations,
  runningStorePath,
  storePaths,
) {
  return generations.map((generation) => {
    const storePath = storePaths.get(generation.generation);
    return {
      ...generation,
      storePath,
      runningStorePath,
      // nixos-rebuild's `current` field describes the profile pointer, which
      // can differ from the configuration that is actually running.
      active: Boolean(storePath && storePath === runningStorePath),
    };
  });
}

export function formatGenerationDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return oneLine(value);
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "medium",
  }).format(date);
}

export function escapeMarkdown(value) {
  return oneLine(value).replace(/([\\`*_{}\[\]<>#+.!|~-])/g, "\\$1");
}

export function formatPackageDiff(output, maximumLength = 120_000) {
  let text = String(output)
    .replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/\r\n?/g, "\n")
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/g, "")
    .trim();
  if (!text) return "_No package changes reported._";
  if (text.length > maximumLength)
    text = `${text.slice(0, maximumLength)}\n… diff truncated`;
  return text
    .split("\n")
    .map((line) => `    ${line}`)
    .join("\n");
}
