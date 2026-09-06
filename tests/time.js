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
