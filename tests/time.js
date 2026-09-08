const fs = require("node:fs");
const vm = require("node:vm");

const source = fs.readFileSync(process.argv[2], "utf8");
const time = {};
vm.createContext(time);
vm.runInContext(source, time, { filename: process.argv[2] });

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

const now = new Date(2026, 7, 23, 21, 30);
const cells = time.calendarCells(now, 0);
const days = cells.filter((cell) => !cell.week);
assert(time.calendarWeeks(now, 0) === 6, "August 2026 spans six Monday-first rows");
assert(cells.length === 48, "a month renders one week number per row it occupies");
assert(cells[0].week && cells[0].label === 31, "August 2026 must begin in ISO week 31");
assert(!cells[1].inMonth && cells[1].day === 0, "a day before the first of the month must be left empty");
assert(days.filter((cell) => cell.inMonth).length === 31, "every day of the month must appear exactly once");
assert(days.filter((cell) => cell.inMonth)[0].day === 1, "the month's own days must start at the first");
assert(!days[days.length - 1].inMonth, "a day after the last of the month must be left empty");
assert(cells.some((cell) => cell.today && cell.day === 23), "the current day must be marked");

// February 2027 starts on a Monday and is exactly four weeks long, so it must
// occupy four rows with no empty cell in any of them.
assert(time.calendarWeeks(now, 6) === 4, "a February beginning on a Monday spans four rows");
const february = time.calendarCells(now, 6);
assert(february.length === 32, "a four-row month must not be padded out to six");
assert(february.filter((cell) => !cell.week).every((cell) => cell.inMonth), "a month that fills its rows must have no empty cells");
assert(february.every((cell) => !cell.today), "only the current month may mark a day as today");

assert(time.isoWeek(new Date(2027, 0, 1, 12)) === 53, "ISO week numbering must cross year boundaries");

const zones = [
  { id: "PST", zone: "PST8PDT", label: "Pacific Time", aliases: "PST PDT", flag: "" },
  { id: "Europe/London", zone: "Europe/London", label: "London", aliases: "UK BST", flag: "🇬🇧" },
];
assert(time.filterZones(zones, "pst").length === 1, "timezone abbreviations must be searchable");
assert(time.filterZones(zones, "Europe/London").length === 1, "IANA timezone names must be searchable");
assert(time.filterZones(zones, "London")[0].flag === "🇬🇧", "city entries must retain their country flag");

const ordered = time.orderZones(zones, ["Europe/London", "PST"], "");
assert(ordered[0].id === "Europe/London" && ordered[1].id === "PST", "multiple pinned zones must lead the list in pin order");
assert(ordered.length === zones.length, "ordering pinned zones must not inject a local-time entry");

const unpinned = time.orderZones(zones, [], "");
assert(unpinned !== zones && unpinned[0] === zones[0] && unpinned[1] === zones[1], "unpinned results remain an independent array in source order");
unpinned.pop();
assert(zones.length === 2, "mutating the ordered array must not mutate the source");
assert(time.orderZones(zones, [], "London").length === 1, "the unpinned fast path must preserve search filtering");

const instant = new Date("2026-08-23T21:30:07Z");
assert(time.offsetTime(instant, "+0530", true) === "03:00:07", "expanded clocks must include seconds across a day boundary");
assert(time.offsetTime(instant, "-0700", false) === "14:30", "compact offset times must omit seconds");
assert(time.offsetTime(instant, "invalid", true) === "", "invalid UTC offsets must not produce a clock");

// Full timestamps preserve the represented instant across date boundaries.
assert(time.clockTimestamp(instant, "+0530") === "2026-08-24T03:00:07+05:30", "fractional offsets cross into the next local day");
assert(time.clockTimestamp(new Date("2027-01-01T00:01:02Z"), "-1200") === "2026-12-31T12:01:02-12:00", "westward timestamps cross the year boundary");
assert(time.clockTimestamp(new Date("2028-02-28T23:59:59Z"), "+0545") === "2028-02-29T05:44:59+05:45", "quarter-hour offsets preserve leap day");
for (const offset of ["+0000", "-0700", "+1400", "-0330", "+1245"]) {
  assert(Date.parse(time.clockTimestamp(instant, offset)) === instant.getTime(), `offset ${offset} preserves the instant`);
}
assert(time.clockTimestamp(new Date("2026-03-29T00:59:59Z"), "+0100") === "2026-03-29T01:59:59+01:00", "pre-DST snapshot remains explicit");
assert(time.clockTimestamp(new Date("2026-03-29T01:00:00Z"), "+0200") === "2026-03-29T03:00:00+02:00", "post-DST snapshot uses its updated offset");
for (const offset of ["", null, "UTC", "+2400", "+1260", "$(touch bad)", "+00:00"]) {
  assert(time.clockTimestamp(instant, offset) === "", "invalid worker offsets cannot reach the clipboard");
}
assert(time.clockTimestamp(new Date(NaN), "+0000") === "", "invalid instants cannot produce a timestamp");
const localTimestamp = time.clockTimestamp(instant);
assert(Date.parse(localTimestamp) === instant.getTime(), "local timestamp preserves the instant in any host timezone");
assert(localTimestamp.slice(11, 13) === String(instant.getHours()).padStart(2, "0"), "local timestamp uses the current local timezone");

const clockShellSource = fs.readFileSync(require("node:path").join(require("node:path").dirname(process.argv[2]), "shell.qml"), "utf8");
const clockContext = {
  Time: time,
  root: { now: instant },
  copyStatus: "",
  copyPending: false,
  clockClipboard: { payload: "", label: "", stdinEnabled: false, running: false },
  clockCopyTimeout: { restart() { this.active = true; }, stop() { this.active = false; } },
  timezoneList: { model: [{ offset: "+0530", label: "Kolkata" }], currentIndex: 0 },
};
vm.createContext(clockContext);
for (const name of ["copyClockTimestamp", "finishClockCopy", "copyClockSelection"]) {
  const match = clockShellSource.match(new RegExp(`      function ${name}\\([^]*?\\n      }`));
  assert(match, `production ${name} handler exists`);
  vm.runInContext(match[0], clockContext);
}
clockContext.copyClockTimestamp("invalid", "bad");
assert(!clockContext.copyPending, "bad offsets do not start the process");
clockContext.copyClockSelection();
assert(clockContext.clockClipboard.payload === "2026-08-24T03:00:07+05:30" && clockContext.copyPending, "keyboard selection copies the selected result's timestamp");
assert(!clockContext.copyStatus.startsWith("Copied"), "starting clipboard process does not acknowledge success");
clockContext.copyClockTimestamp("-0700", "Los Angeles");
assert(clockContext.clockClipboard.label === "Kolkata", "in-flight payload and label cannot be overwritten");
clockContext.clockClipboard.running = false;
clockContext.finishClockCopy(0, 0);
assert(clockContext.copyStatus === "Copied Kolkata" && !clockContext.copyPending && !clockContext.clockCopyTimeout.active, "only successful completion acknowledges the selected zone");
clockContext.copyClockTimestamp(undefined, "local time");
assert(clockContext.clockClipboard.payload === localTimestamp, "local keyboard shortcut uses the local offset");
clockContext.clockClipboard.running = false;
clockContext.finishClockCopy(1, 0);
assert(clockContext.copyStatus === "Could not copy timestamp", "failed clipboard exit reports failure");
clockContext.copyClockTimestamp("+0000", "UTC");
clockContext.clockClipboard.running = false;
clockContext.finishClockCopy(0, 1);
assert(clockContext.copyStatus === "Could not copy timestamp", "crash status never acknowledges success");
clockContext.copyClockTimestamp("+0000", "UTC");
clockContext.clockClipboard.running = false;
clockContext.finishClockCopy(-1, 1);
clockContext.finishClockCopy(0, 0);
assert(clockContext.copyStatus === "Could not copy timestamp" && !clockContext.copyPending, "timeout clears pending state and ignores a late exit");
clockContext.timezoneList.model = [];
clockContext.copyClockSelection();
assert(!clockContext.copyPending, "Enter with no search results cannot start a copy");
const clockProcess = clockShellSource.match(/id: clockClipboard[^]*?onStarted: \{([^]*?)\n        }/);
assert(clockProcess, "production clipboard process exists");
let timestampWritten = "";
const clockStdin = { payload: localTimestamp, stdinEnabled: true, write(value) { timestampWritten = value; } };
vm.createContext(clockStdin);
vm.runInContext(clockProcess[1], clockStdin);
assert(timestampWritten === localTimestamp && !clockStdin.stdinEnabled, "process writes exact timestamp and closes stdin");
console.log("World clock timestamp and clipboard tests passed");
