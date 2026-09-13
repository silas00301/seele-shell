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
