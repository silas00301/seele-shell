const fs = require('node:fs')
const vm = require('node:vm')
const assert = require('node:assert/strict')
const source = fs.readFileSync(process.argv[2], 'utf8')
const calendar = vm.createContext({ Date })
vm.runInContext(source, calendar)

const base = { calendar_id: 'one', id: 'a', summary: 'A', status: 'confirmed', start: { dateTime: '2026-09-27T09:00:00+02:00' }, end: { dateTime: '2026-09-27T10:00:00+02:00' } }
const recurring = { ...base, id: 'series_20260927T070000Z', summary: 'B', start: { dateTime: '2026-09-27T09:10:00+02:00' }, end: { dateTime: '2026-09-27T10:10:00+02:00' } }
const allDay = { ...base, id: 'all', start: { date: '2026-09-26' }, end: { date: '2026-09-28' } }
const declined = { ...base, id: 'declined', attendees: [{ self: true, responseStatus: 'declined' }] }
const calendars = [{ id: 'one', backgroundColor: '#123456' }, { id: 'two', backgroundColor: '#abcdef' }]
const secondary = { ...base, id: 'secondary', calendar_id: 'two' }
assert.equal(calendar.agenda([base, recurring, allDay, declined], ['one'], '2026-09-27')[0].id, 'all')
assert.equal(calendar.agenda([allDay], ['one'], '2026-09-28').length, 0)
assert.equal(calendar.allDayLabel(allDay), 'All day · 2026-09-26–2026-09-27')
assert.equal(calendar.agenda([declined], ['one'], '2026-09-27').length, 0)
assert.equal(calendar.rsvp({ attendees: [{ self: true, responseStatus: 'needsAction' }] }), 'Unanswered')
assert.equal(calendar.dots([base, secondary, declined], calendars, ['one','two'], '2026-09-27').length, 2)
const before = Date.parse('2026-09-27T08:44:59+02:00')
assert.equal(calendar.indicator([base], ['one'], before), null)
const within = Date.parse('2026-09-27T08:45:00+02:00')
assert.equal(calendar.indicator([base, recurring], ['one'], within).event.id, 'a')
assert.equal(calendar.indicator([base, recurring], ['one'], within).extra, 0)
assert.equal(calendar.indicator([base, recurring], ['one'], Date.parse('2026-09-27T08:55:00+02:00')).extra, 1)
const ongoing = Date.parse('2026-09-27T09:05:00+02:00')
assert.equal(calendar.indicator([base, recurring], ['one'], ongoing).event.id, recurring.id)
assert.equal(calendar.indicator([base], ['one'], Date.parse('2026-09-27T10:00:00+02:00')), null)
assert.equal(calendar.color({ ...base, colorId: '4' }, calendars, { event: { '4': { background: '#fedcba' } } }), '#fedcba')
assert.equal(calendar.safeLink('javascript:alert(1)'), '')
assert.equal(calendar.safeLink('https://evil@example.com/'), '')
assert.equal(calendar.safeLink('https://meet.google.com/abc-defg-hij'), 'https://meet.google.com/abc-defg-hij')
console.log('Calendar policy tests passed')
