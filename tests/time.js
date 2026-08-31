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
assert(cells.length === 48, "a month must render six week-numbered rows");
assert(cells[0].week && cells[0].label === 31, "August 2026 must begin in ISO week 31");
assert(cells[1].day === 27 && !cells[1].inMonth, "a month must include leading Monday-first days");
assert(cells.some((cell) => cell.today && cell.day === 23), "the current day must be marked");
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

const instant = new Date("2026-08-23T21:30:07Z");
assert(time.offsetTime(instant, "+0530", true) === "03:00:07", "expanded clocks must include seconds across a day boundary");
assert(time.offsetTime(instant, "-0700", false) === "14:30", "compact offset times must omit seconds");
assert(time.offsetTime(instant, "invalid", true) === "", "invalid UTC offsets must not produce a clock");
