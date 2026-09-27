const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const shell = fs.readFileSync(process.argv[2], 'utf8');
const methods = ['requestMeeting', 'refreshMeeting', 'meetingRequest', 'parseClockData'].map(name =>
  shell.match(new RegExp(`^  function ${name}\\([^\\n]*\\) \\{\\n[\\s\\S]*?^  \\}`, 'm'))[0]).join('\n');
const timer = shell.match(/id: meetingDebounce[\s\S]*?onTriggered: (\{[\s\S]*?^    \})/m)[1];
const sent = [];
const days = [];
let scheduled = false;
// Calendar blocks in epoch seconds; the stand-in filters like the store does.
const blocks = [];
const state = vm.createContext({meetingRequestId: 0, meetingSentId: 0, meetingInFlight: false,
  meetingPending: false, meetingError: '', meetingData: {}, meetingSelection: {}, meetingPlanning: true,
  clockError: '', clockData: {}, console, Date, Math, Object,
  meetingDebounce: {restart() {scheduled = true;}},
  calendarStore: {
    busy(from, to) { return blocks.filter(block => block.end > from && block.start < to).slice().sort((a, b) => a.start - b.start); },
    forDay(date) { days.push(date); }
  },
  clockProcess: {running: true, write(text) {sent.push(JSON.parse(text));}}
});
state.root = state;
vm.runInContext(methods + '\nfunction tick() ' + timer, state);
const plan = (start, date, extra) => JSON.stringify({requestId: sent.at(-1).requestId,
  meeting: Object.assign({start, duration: 60, day: {date}}, extra || {})});

state.requestMeeting({start: 1790690400, duration: 60});
assert(state.meetingPending);
state.tick();
assert.equal(sent.length, 1);
assert.deepEqual(sent[0].meeting.busy, [], 'no calendar means no busy time');
assert(state.meetingInFlight);
for (let step = 1; step <= 16; step++) {
  state.requestMeeting({start: 1790690400 + step * 900, duration: 60});
  state.tick();
}
assert.equal(sent.length, 1, 'scrubbing cannot queue more than one native calculation');
state.parseClockData(JSON.stringify({requestId: 1, meeting: {start: 1790690400, day: {date: '2026-09-29'}}}));
assert(state.meetingPending, 'stale reply must never enable copy');
assert.equal(state.meetingData.start, undefined);
assert.deepEqual(days, [], 'a stale reply does not move the calendar');
assert.equal(state.meetingInFlight, false);
assert(scheduled);
state.tick();
assert.equal(sent.length, 2);
assert.equal(sent[1].meeting.start, 1790690400 + 16 * 900, 'only the final scrub position is submitted');
state.parseClockData(plan(1790704800, '2026-09-29'));
assert.equal(state.meetingPending, false);
assert.equal(state.meetingData.start, 1790704800);
assert.deepEqual(days, ['2026-09-29'], 'a new day asks the calendar for its events');

// Busy time travels with the request: around where the selection is headed,
// nearest first, bounded, as bare intervals without titles.
const anchor = 1790704800 + 86400;
for (let i = 0; i < 120; i++) blocks.push({start: anchor - 60000 + i * 1000, end: anchor - 59000 + i * 1000, title: 'Private ' + i});
blocks.push({start: anchor + 200000, end: anchor + 201000, title: 'Too far'});
state.requestMeeting({start: 1790704800, duration: 60, days: 1});
state.tick();
const busy = sent[2].meeting.busy;
assert.equal(busy.length, 96, 'the worker bound is respected');
assert(busy.every(pair => pair.length === 2 && pair.every(Number.isInteger)));
assert(!JSON.stringify(sent[2]).includes('Private'), 'event titles never reach the worker');
assert(!busy.some(pair => pair[0] === anchor + 200000), 'blocks beyond a day and a half are left out');
assert.deepEqual(busy[0], [anchor - 1000, anchor], 'the nearest blocks are kept');
state.parseClockData(plan(anchor, '2026-09-30'));
assert.deepEqual(days, ['2026-09-29', '2026-09-30']);

// A calendar change re-asks for what is on screen, without moving the day or
// asking the calendar again; an unanswered move is re-sent as it was.
state.refreshMeeting();
state.tick();
assert.deepEqual([sent[3].meeting.start, sent[3].meeting.days], [anchor, undefined]);
state.parseClockData(plan(anchor, '2026-09-30'));
assert.deepEqual(days, ['2026-09-29', '2026-09-30'], 'a refresh for the same day does not ask the calendar again');
state.requestMeeting({start: anchor, duration: 60, days: 1});
state.refreshMeeting();
state.tick();
assert.equal(sent[4].meeting.days, 1, 'a pending day move survives a calendar refresh');
state.parseClockData(plan(anchor + 86400, '2026-10-01'));
state.meetingPlanning = false;
scheduled = false;
state.refreshMeeting();
assert(!scheduled && !state.meetingPending, 'a closed planner does not ask again');
state.meetingPlanning = true;

state.requestMeeting({date: 'invalid'});
state.tick();
state.parseClockData(JSON.stringify({requestId: sent.at(-1).requestId, meetingError: 'Invalid date'}));
assert.equal(state.meetingError, 'Invalid date');
assert.equal(state.meetingPending, false);
assert.equal(state.meetingData.start, anchor + 86400, 'invalid requests preserve the last valid projection');
assert.deepEqual(sent.at(-1).meeting.busy, [], 'an unreadable date sends no busy time');
state.parseClockData(JSON.stringify({zones: [], pinned: []}));
assert.equal(state.meetingError, 'Invalid date', 'live clock refresh must not erase planner errors');
console.log('meeting planner: production coordinator coalescing, busy time, calendar days, refreshes and validation states passed');
