const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const notes = {};
vm.createContext(notes);
vm.runInContext(fs.readFileSync(process.argv[2], 'utf8'), notes);

// Search is over what a row actually shows.
const all = [
  { title: 'Trip plan', name: 'Trip plan', excerpt: 'Berlin hotel' },
  { title: 'Work', name: '2026-01-02 1030', excerpt: 'Meeting' },
];
assert.equal(notes.filter(all, 'BERLIN trip').length, 1);
assert.equal(notes.filter(all, '1030').length, 1, 'the filename is searchable too');
assert.equal(notes.filter(all, 'absent').length, 0);
assert.equal(notes.filter(all, '').length, 2);

assert.equal(notes.duration(61000), '1:01');
assert.equal(notes.duration(-10), '0:00');
assert.equal(notes.label({ title: '', name: 'Untitled 3' }), 'Untitled 3');
assert.equal(notes.label(null), 'Untitled note');

const now = new Date(2026, 8, 9, 15, 0, 0).getTime();
assert.equal(notes.when(new Date(2026, 8, 9, 9, 5, 0).getTime() / 1000, now), '09:05');
assert.equal(notes.when(new Date(2026, 8, 8, 23, 0, 0).getTime() / 1000, now), 'Yesterday');
assert.equal(notes.when(new Date(2026, 0, 3, 9, 0, 0).getTime() / 1000, now), '03.01');
assert.equal(notes.when(new Date(2025, 0, 3, 9, 0, 0).getTime() / 1000, now), '2025-01-03');

// Every command is a replacement plus a caret, so the editor can apply it
// without rewriting the document and losing the undo stack with it.
function run(text, command) {
  assert.ok(command, 'command expected');
  return {
    text: text.slice(0, command.start) + command.text + text.slice(command.end),
    caret: command.caret,
  };
}

let source = 'hello world';
let result = run(source, notes.wrap(source, 0, 5, '**'));
assert.equal(result.text, '**hello** world');
assert.equal(result.caret, 2, 'the caret lands inside the emphasis it just opened');
result = run(result.text, notes.wrap(result.text, 2, 7, '**'));
assert.equal(result.text, 'hello world', 'the same shortcut takes the emphasis off again');

source = '# Title';
assert.equal(run(source, notes.heading(source, 3, 1)).text, 'Title', 'asking for the level a line has clears it');
assert.equal(run(source, notes.heading(source, 3, 3)).text, '### Title');
source = 'plain';
result = run(source, notes.heading(source, 5, 2));
assert.equal(result.text, '## plain');
assert.equal(result.caret, 8, 'the caret keeps its place in the words');

source = 'milk';
result = run(source, notes.task(source, 4));
assert.equal(result.text, '- [ ] milk');
result = run(result.text, notes.task(result.text, 10));
assert.equal(result.text, '- [x] milk');
result = run(result.text, notes.task(result.text, 10));
assert.equal(result.text, '- milk', 'a finished task drops its box rather than cycling forever');

source = 'read the docs';
result = run(source, notes.link(source, 5, 13));
assert.equal(result.text, 'read [the docs]()');
assert.equal(result.caret, 16, 'the caret waits inside the parentheses for the address');
source = 'https://example.com';
result = run(source, notes.link(source, 0, source.length));
assert.equal(result.text, '[](https://example.com)');
assert.equal(result.caret, 1, 'a pasted address only needs its label typed');

source = '- item';
result = run(source, notes.newline(source, 6));
assert.equal(result.text, '- item\n- ');
source = '3) third';
assert.equal(run(source, notes.newline(source, 8)).text, '3) third\n4) ');
source = '- [ ] task';
assert.equal(run(source, notes.newline(source, 10)).text, '- [ ] task\n- [ ] ');
source = '- item\n- ';
assert.equal(run(source, notes.newline(source, 9)).text, '- item\n', 'an empty item ends the list');
assert.equal(notes.newline('paragraph', 9), null, 'ordinary text is left to the editor');

// An embed is its own line, and removing one never touches the file it names.
source = 'Some thoughts';
result = run(source, notes.embed(source, source.length, 'Voice memo 1.wav'));
assert.equal(
  result.text,
  'Some thoughts\n\n![[Voice memo 1.wav]]\n',
  'a blank line keeps the embed out of the paragraph above it',
);
result = run(result.text, notes.unembed(result.text, 'Voice memo 1.wav'));
assert.equal(result.text, 'Some thoughts\n\n');
assert.equal(
  run('', notes.embed('', 0, 'Voice memo 1.wav')).text,
  '![[Voice memo 1.wav]]\n',
  'an audio-only note starts with its recording',
);
assert.equal(
  run('Body\n\n', notes.embed('Body\n\n', 6, 'Voice memo 1.wav')).text,
  'Body\n\n![[Voice memo 1.wav]]\n',
  'a blank line already there is not doubled',
);
assert.equal(notes.unembed('nothing here', 'Voice memo 1.wav'), null);
assert.equal(
  run('![[a.wav|alias]]\ntail', notes.unembed('![[a.wav|alias]]\ntail', 'a.wav')).text,
  'tail',
  'an aliased embed is still the same recording',
);
const literal = '![[Memo (2).wav]]\n';
assert.equal(run(literal, notes.unembed(literal, 'Memo (2).wav')).text, '', 'a name is matched literally');

assert.match(notes.status('conflict'), /disk/);
assert.match(notes.status('gone'), /moved|deleted/);
assert.equal(notes.status('dirty'), 'Unsaved changes');
assert.equal(
  notes.obsidianUri('/home/x/Documents/Main/', 'Inbox/A note.md'),
  'obsidian://open?vault=Main&file=Inbox%2FA%20note.md',
);
assert.equal(notes.obsidianUri('', 'Inbox/A.md'), '');

console.log('Notes search, date, editing command and Obsidian link checks passed');
