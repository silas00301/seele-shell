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

// Date copying follows the calendar's local day, including leap days and years.
assert(time.calendarDate(new Date(2026, 0, 1, 0, 1)) === "2026-01-01", "midnight uses local date fields");
assert(time.calendarDate(new Date(2028, 1, 29, 23, 59)) === "2028-02-29", "leap dates remain valid near midnight");
assert(time.calendarDate(new Date(NaN)) === "", "invalid dates cannot reach the clipboard");
assert(days.filter(cell => cell.inMonth).map(time.calendarCopyDate).every((value, i) => value === `2026-08-${String(i + 1).padStart(2, "0")}`), "each real day carries its exact ISO date");
assert(cells.filter(cell => cell.week || !cell.inMonth).every(cell => time.calendarCopyDate(cell) === ""), "week numbers and blank cells cannot be copied");
assert(time.calendarCells(new Date(2026, 11, 31, 23, 59), 1).some(cell => cell.date === "2027-01-01"), "scrolling across a year supplies the next year's dates");

// Execute the actual QML request/completion methods, with only the process and
// timer replaced. This guards acknowledgement, overlap, failure and stdin data.
const shellSource = fs.readFileSync(require("node:path").join(require("node:path").dirname(process.argv[2]), "shell.qml"), "utf8");
const calendarContext = {
  Time: time,
  selectedDate: "",
  copyStatus: "",
  copyPending: false,
  calendarClipboard: { payload: "", stdinEnabled: false, running: false },
  calendarCopyTimeout: { restart() { this.active = true; }, stop() { this.active = false; } },
};
vm.createContext(calendarContext);
for (const name of ["copyCalendarDate", "finishCalendarCopy"]) {
  const match = shellSource.match(new RegExp(`      function ${name}\\([^]*?\\n      }`));
  assert(match, `production ${name} handler exists`);
  vm.runInContext(match[0], calendarContext);
}
calendarContext.copyCalendarDate(cells[0]);
assert(!calendarContext.copyPending, "week click cannot start a process");
calendarContext.copyCalendarDate({ inMonth: true, date: "2026-08-23" });
assert(calendarContext.selectedDate === "2026-08-23" && calendarContext.copyPending, "click selects the requested day and begins copy");
assert(calendarContext.calendarClipboard.payload === "2026-08-23" && calendarContext.calendarClipboard.stdinEnabled, "clipboard text is exact stdin payload");
assert(!calendarContext.copyStatus.startsWith("Copied"), "starting a process must not claim success");
calendarContext.copyCalendarDate({ inMonth: true, date: "2026-08-24" });
assert(calendarContext.calendarClipboard.payload === "2026-08-23", "a second click cannot replace an in-flight payload");
calendarContext.calendarClipboard.running = false;
calendarContext.finishCalendarCopy(0, 0);
assert(calendarContext.copyStatus === "Copied 2026-08-23" && !calendarContext.copyPending && !calendarContext.calendarCopyTimeout.active, "only successful completion acknowledges the exact date");
calendarContext.copyCalendarDate({ inMonth: true, date: "2026-08-24" });
calendarContext.calendarClipboard.running = false;
calendarContext.finishCalendarCopy(1, 0);
assert(calendarContext.copyStatus === "Could not copy date", "nonzero clipboard exit reports failure");
calendarContext.copyCalendarDate({ inMonth: true, date: "2026-08-24" });
calendarContext.calendarClipboard.running = false;
calendarContext.finishCalendarCopy(0, 1);
assert(calendarContext.copyStatus === "Could not copy date", "a crashed process cannot report success");
calendarContext.copyCalendarDate({ inMonth: true, date: "2026-08-24" });
calendarContext.calendarClipboard.running = false;
calendarContext.finishCalendarCopy(-1, 1);
calendarContext.calendarClipboard.running = false;
calendarContext.finishCalendarCopy(0, 0);
assert(calendarContext.copyStatus === "Could not copy date" && !calendarContext.copyPending, "timeout releases the request and ignores late completion");
const calendarProcess = shellSource.match(/id: calendarClipboard[^]*?onStarted: \{([^]*?)\n        }/);
assert(calendarProcess, "production clipboard process exists");
let calendarWritten = "";
const stdin = { payload: "2026-08-23", stdinEnabled: true, write(value) { calendarWritten = value; } };
vm.createContext(stdin);
vm.runInContext(calendarProcess[1], stdin);
assert(calendarWritten === "2026-08-23" && !stdin.stdinEnabled, "process writes exact date then closes stdin");
console.log("Calendar date and clipboard tests passed");

assert(time.moveCalendarDate(new Date(2026, 7, 23), "2026-08-31", 1).date === "2026-09-01", "keyboard movement crosses a month");
assert(time.moveCalendarDate(new Date(2026, 7, 23), "2026-12-31", 1).monthOffset === 5, "keyboard movement scrolls into the correct next-year month");
assert(time.moveCalendarDate(new Date(2028, 1, 1), "2028-02-28", 1).date === "2028-02-29", "keyboard movement retains leap day");
assert(time.moveCalendarDate(new Date(2026, 2, 1), "2026-03-28", 1).date === "2026-03-29", "DST movement counts calendar days");
assert(time.moveCalendarDate(now, "", 0).date === "2026-08-23", "Home selects local today");
assert(time.moveCalendarDate(now, "2031-08-31", 1) === null, "navigation stays inside the rendered month range");
assert(time.moveCalendarDate(now, "2026-02-30", 1) === null, "invalid selected dates cannot roll into another month");
calendarContext.root = { now };
calendarContext.ListView = { Contain: 2 };
calendarContext.calendarMonths = { positionViewAtIndex(index, mode) { this.index = index; this.mode = mode; } };
vm.runInContext(shellSource.match(/      function moveCalendarSelection\([^]*?\n      }/)[0], calendarContext);
calendarContext.selectedDate = "2026-08-31";
calendarContext.moveCalendarSelection(1, false);
assert(calendarContext.selectedDate === "2026-09-01" && calendarContext.calendarMonths.index === 61, "production arrow handler moves selection and scrolls the month");
calendarContext.moveCalendarSelection(0, true);
assert(calendarContext.selectedDate === "2026-08-23" && calendarContext.calendarMonths.index === 60, "production Home handler returns to today");
calendarContext.copyPending = true;
calendarContext.moveCalendarSelection(1, false);
assert(calendarContext.selectedDate === "2026-08-23", "keyboard movement cannot relabel a pending copy");
