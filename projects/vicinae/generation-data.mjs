// Date/locale rendering belongs to the launcher's Intl implementation. Native
// snapshots already contain bounded one-line metadata and escaped Markdown.
export function formatGenerationDate(value, fallback = value) {
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return fallback || "Unknown";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "medium",
  }).format(date);
}

const units = [
  ["year", 31536000],
  ["month", 2592000],
  ["week", 604800],
  ["day", 86400],
  ["hour", 3600],
  ["minute", 60],
];

// "3 days ago" is what decides whether a rollback target is the right one; the
// exact timestamp stays beside it rather than being replaced by this.
export function formatGenerationAge(value, now = Date.now()) {
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return "";
  const seconds = Math.round((date.valueOf() - now) / 1000);
  const format = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  for (const [unit, size] of units)
    if (Math.abs(seconds) >= size)
      return format.format(Math.trunc(seconds / size), unit);
  return format.format(seconds, "second");
}
