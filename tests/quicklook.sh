#!/usr/bin/env bash
set -euo pipefail

worker=$1
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/bin" "$work/runtime" "$work/files" "$work/files/folder"

# Poppler is faked rather than installed: what is under test here is which
# page is asked for, where the result is published and when it is released,
# not whether Poppler can draw. The fakes record every invocation so a second
# visit to the same page can be proven not to reach them again.
printf '#!%s\n' "$(command -v bash)" > "$work/bin/pdfinfo"
cat >> "$work/bin/pdfinfo" <<'INFO'
set -euo pipefail
printf '%s\n' "$*" >> "$PDF_CALLS"
case "$1" in
  *sealed.pdf) echo "Error: encrypted" >&2; exit 1 ;;
esac
printf 'Title:          Fixture\nPages:          7\n'
INFO
printf '#!%s\n' "$(command -v bash)" > "$work/bin/pdftoppm"
cat >> "$work/bin/pdftoppm" <<'PPM'
set -euo pipefail
printf '%s\n' "$*" >> "$PDF_CALLS"
root=${*: -1}
page=""
previous=""
for argument in "$@"; do
  test "$previous" != -f || page=$argument
  previous=$argument
done
test "$page" != 9 || exit 1
printf 'page %s' "$page" > "$root.png"
PPM
chmod +x "$work/bin/pdfinfo" "$work/bin/pdftoppm"

printf 'first\xe2\x80\xaeevil\x07\nsecond\n' > "$work/files/notes.txt"
printf '# Title\n\nbody\n' > "$work/files/plan.md"
printf '\x89PNG\r\n\x1a\n rest' > "$work/files/shot.txt"
printf '%%PDF-1.7\n' > "$work/files/report.pdf"
printf '%%PDF-1.7\n' > "$work/files/sealed.pdf"
printf '\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00' > "$work/files/program"
for index in $(seq -w 1 12); do : > "$work/files/folder/entry-$index"; done
mkdir "$work/files/folder/inner"

PATH="$work/bin:$PATH" XDG_RUNTIME_DIR="$work/runtime" PDF_CALLS="$work/calls" \
QUICKLOOK_FILES="$work/files" node - "$worker" <<'NODE'
const { spawn } = require('node:child_process')
const fs = require('node:fs')
const path = require('node:path')
const assert = require('node:assert/strict')
const readline = require('node:readline')

const files = process.env.QUICKLOOK_FILES
const runtime = process.env.XDG_RUNTIME_DIR
const calls = () => fs.existsSync(process.env.PDF_CALLS)
  ? fs.readFileSync(process.env.PDF_CALLS, 'utf8').trim().split('\n').filter(Boolean)
  : []

function start() {
  const worker = spawn(process.argv[2], [], { stdio: ['pipe', 'pipe', 'inherit'] })
  const messages = []
  readline.createInterface({ input: worker.stdout }).on('line', line => messages.push(JSON.parse(line)))
  return { worker, messages }
}
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
async function next(messages, match) {
  for (let i = 0; i < 1000; i++) {
    const index = messages.findIndex(match)
    if (index >= 0) return messages.splice(index, 1)[0]
    await delay(10)
  }
  throw Error('the worker never answered')
}
async function settle(check, description) {
  for (let i = 0; i < 500; i++) {
    if (check()) return
    await delay(10)
  }
  throw Error(description)
}
const workspace = () => fs.readdirSync(runtime).map(entry => path.join(runtime, entry))

async function main() {
  const { worker, messages } = start()
  const send = message => worker.stdin.write(JSON.stringify(message) + '\n')
  const timer = setTimeout(() => { worker.kill('SIGKILL'); throw Error('quick look worker test timed out') }, 60000)

  // One request, one answer, in the order the caller named the files. A path
  // that cannot be described keeps its place so the numbering a reader is
  // walking through never shifts under them.
  send({ command: 'open', id: 1, paths: [
    path.join(files, 'notes.txt'),
    path.join(files, 'gone.txt'),
    path.join(files, 'plan.md'),
    path.join(files, 'shot.txt'),
    path.join(files, 'report.pdf'),
    path.join(files, 'sealed.pdf'),
    path.join(files, 'program'),
    path.join(files, 'folder'),
    '/dev/zero',
    'relative/path',
  ] })
  const opened = await next(messages, m => m.id === 1 && m.event === 'items')
  const kinds = opened.items.map(item => item.kind)
  assert.deepEqual(kinds, [
    'text', 'unavailable', 'markdown', 'image', 'pdf', 'pdf',
    'binary', 'directory', 'unavailable', 'unavailable',
  ])

  // Content is shown to the one account that already owns it, but never the
  // characters that could forge a line of the interface around it.
  assert.equal(opened.items[0].text, 'firstevil\nsecond\n')
  assert.equal(opened.items[0].truncated, true)
  assert.equal(opened.items[2].text, '# Title\n\nbody\n')
  assert.equal(opened.items[2].truncated, false)
  // A file the panel draws itself is described, never read into the reply.
  assert.equal(opened.items[3].text, undefined)
  assert.equal(opened.items[6].text, undefined)

  assert.equal(opened.items[4].pages, 7)
  assert.equal(opened.items[4].error, '')
  // A document Poppler will not open is a stated limit, not a wait.
  assert.equal(opened.items[5].pages, undefined)
  assert.match(opened.items[5].error, /cannot be opened/)

  assert.equal(opened.items[7].total, 13)
  assert.equal(opened.items[7].entries.length, 13)
  assert.deepEqual(opened.items[7].entries[0], { name: 'entry-01', directory: false })
  assert.equal(opened.items[7].entries.find(entry => entry.name === 'inner').directory, true)

  // A device never becomes a preview, and a relative path is refused before
  // anything is opened at all.
  assert.match(opened.items[8].error, /Only files and folders/)
  assert.match(opened.items[9].error, /cannot be previewed/)

  // A rendered page is a private runtime file inside a private directory.
  send({ command: 'page', id: 1, index: 4, page: 3 })
  const page = await next(messages, m => m.event === 'page' && m.page === 3)
  assert.equal(page.index, 4)
  assert.equal(page.error, '')
  assert.equal(fs.readFileSync(page.path, 'utf8'), 'page 3')
  assert.equal(fs.statSync(page.path).mode & 0o777, 0o600)
  assert.equal(fs.statSync(path.dirname(page.path)).mode & 0o777, 0o700)
  const rendering = calls().filter(line => line.includes('-scale-to'))
  assert.equal(rendering.length, 1)
  assert.match(rendering[0], /-singlefile/)
  assert.match(rendering[0], /-f 3 -l 3/)

  // Revisiting a page is answered from the invocation's own directory rather
  // than by rendering it a second time.
  send({ command: 'page', id: 1, index: 4, page: 3 })
  const again = await next(messages, m => m.event === 'page' && m.page === 3)
  assert.equal(again.path, page.path)
  assert.equal(calls().filter(line => line.includes('-scale-to')).length, 1)

  // A page the renderer refuses, and a request against a file that is not a
  // document, are both reported rather than left pending.
  send({ command: 'page', id: 1, index: 4, page: 9 })
  assert.match((await next(messages, m => m.event === 'page' && m.page === 9)).error, /cannot be drawn/)
  send({ command: 'page', id: 1, index: 0, page: 1 })
  assert.match((await next(messages, m => m.event === 'page' && m.index === 0)).error, /cannot be drawn/)

  // A request for another generation is not this panel's, and is ignored
  // rather than answered against the files it is showing.
  send({ command: 'page', id: 99, index: 4, page: 2 })
  await delay(200)
  assert.equal(messages.filter(m => m.event === 'page' && m.page === 2).length, 0)

  // A second preview supersedes the first and takes its pages with it.
  send({ command: 'open', id: 2, paths: [path.join(files, 'report.pdf')] })
  await next(messages, m => m.id === 2 && m.event === 'items')
  await settle(() => !fs.existsSync(page.path), 'a superseded page survived')

  send({ command: 'page', id: 2, index: 0, page: 1 })
  const second = await next(messages, m => m.id === 2 && m.event === 'page')
  assert.ok(fs.existsSync(second.path))
  send({ command: 'cancel', id: 2 })
  await settle(() => !fs.existsSync(second.path), 'a cancelled page survived')

  // Closing stdin ends the worker and removes its workspace entirely.
  worker.stdin.end()
  const exited = await new Promise(resolve => worker.on('exit', (code, signal) => resolve({ code, signal })))
  assert.deepEqual(exited, { code: 0, signal: null })
  await settle(() => workspace().length === 0, 'the workspace survived stdin EOF')

  // A shell reload sends SIGTERM, which has to leave as little behind.
  const terminating = start()
  terminating.worker.stdin.write(JSON.stringify({
    command: 'open', id: 3, paths: [path.join(files, 'report.pdf')],
  }) + '\n')
  await next(terminating.messages, m => m.id === 3 && m.event === 'items')
  assert.ok(workspace().length > 0)
  terminating.worker.kill('SIGTERM')
  const stopped = await new Promise(resolve => terminating.worker.on('exit', (code, signal) => resolve({ code, signal })))
  assert.deepEqual(stopped, { code: 0, signal: null })
  await settle(() => workspace().length === 0, 'the workspace survived SIGTERM')

  clearTimeout(timer)
  console.log('quick look classification, bounds, private pages, supersession and cleanup passed')
}
main().catch(error => { console.error(error); process.exitCode = 1 })
NODE
