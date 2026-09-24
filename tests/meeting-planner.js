const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const shell = fs.readFileSync(process.argv[2], 'utf8');
const methods = ['requestMeeting', 'parseClockData'].map(name =>
  shell.match(new RegExp(`^  function ${name}\\([^\\n]*\\) \\{\\n[\\s\\S]*?^  \\}`, 'm'))[0]).join('\n');
const timer = shell.match(/id: meetingDebounce[\s\S]*?onTriggered: (\{[\s\S]*?^    \})/m)[1];
const sent = [];
let scheduled = false;
const state = vm.createContext({meetingRequestId: 0, meetingSentId: 0, meetingInFlight: false,
  meetingPending: false, meetingError: '', meetingData: {}, meetingSelection: {},
  clockError: '', clockData: {}, console,
  meetingDebounce: {restart() {scheduled = true;}},
  clockProcess: {running: true, write(text) {sent.push(JSON.parse(text));}}
});
state.root = state;
vm.runInContext(methods + '\nfunction tick() ' + timer, state);
state.requestMeeting({date: '2026-10-25', minute: 30, duration: 60});
assert(state.meetingPending);
state.tick();
assert.equal(sent.length, 1);
assert(state.meetingInFlight);
for (let minute = 45; minute <= 300; minute += 15) {
  state.requestMeeting({date: '2026-10-25', minute, duration: 60});
  state.tick();
}
assert.equal(sent.length, 1, 'scrubbing cannot queue more than one native calculation');
state.parseClockData(JSON.stringify({requestId: 1, meeting: {minute: 30}}));
assert(state.meetingPending, 'stale reply must never enable copy');
assert.equal(state.meetingData.minute, undefined);
assert.equal(state.meetingInFlight, false);
assert(scheduled);
state.tick();
assert.equal(sent.length, 2);
assert.equal(sent[1].meeting.minute, 300, 'only the final scrub position is submitted');
state.parseClockData(JSON.stringify({requestId: sent[1].requestId, meeting: {minute: 300}}));
assert.equal(state.meetingPending, false);
assert.equal(state.meetingData.minute, 300);
state.requestMeeting({date: 'invalid'});
state.tick();
state.parseClockData(JSON.stringify({requestId: sent[2].requestId, meetingError: 'Invalid date'}));
assert.equal(state.meetingError, 'Invalid date');
assert.equal(state.meetingPending, false);
assert.equal(state.meetingData.minute, 300, 'invalid requests preserve the last valid projection');
state.parseClockData(JSON.stringify({zones: [], pinned: []}));
assert.equal(state.meetingError, 'Invalid date', 'live clock refresh must not erase planner errors');
console.log('meeting planner: production coordinator coalescing, stale replies and validation states passed');
