#!/usr/bin/env bash
set -euo pipefail

worker=$1
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/bin" "$work/runtime"

# A synthetic frame rather than a captured one: the worker's job is to hand
# back the exact bytes at an exact pixel, so the fixture states what every
# pixel is. (0,0) is the shell's own accent, which is also what proves an
# exact token match survives capture, file and seek untouched.
node - "$work/frame.ppm" <<'NODE'
const fs = require('node:fs')
const [width, height] = [8, 4]
const pixels = Buffer.alloc(width * height * 3)
for (let y = 0; y < height; y++) {
  for (let x = 0; x < width; x++) {
    const at = (y * width + x) * 3
    pixels[at] = x * 16
    pixels[at + 1] = y * 32
    pixels[at + 2] = 128
  }
}
pixels[0] = 0xb4; pixels[1] = 0xbe; pixels[2] = 0xfe
fs.writeFileSync(process.argv[2], Buffer.concat([Buffer.from(`P6\n${width} ${height}\n255\n`), pixels]))
NODE

printf '#!%s\n' "$(command -v bash)" > "$work/bin/grim"
cat >> "$work/bin/grim" <<'GRIM'
set -euo pipefail
test "$1" = -t && test "$2" = ppm && test "$3" = -o && test "$5" = -
case "$4" in
  DP-1|DP-2) cat "$COLOR_FIXTURE" ;;
  slow) exec sleep 30 ;;
  *) exit 1 ;;
esac
GRIM
chmod +x "$work/bin/grim"

PATH="$work/bin:$PATH" XDG_RUNTIME_DIR="$work/runtime" COLOR_FIXTURE="$work/frame.ppm" \
node - "$worker" <<'NODE'
const { spawn } = require('node:child_process')
const fs = require('node:fs')
const path = require('node:path')
const assert = require('node:assert/strict')
const readline = require('node:readline')
const worker = spawn(process.argv[2], [], { stdio: ['pipe', 'pipe', 'inherit'] })
const lines = readline.createInterface({ input: worker.stdout })
const messages = []
lines.on('line', line => messages.push(JSON.parse(line)))
const exit = new Promise(resolve => worker.on('exit', (code, signal) => resolve({ code, signal })))
const timer = setTimeout(() => { worker.kill('SIGKILL'); throw Error('colour worker test timed out') }, 60000)
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
const send = message => worker.stdin.write(JSON.stringify(message) + '\n')
async function next(id, event, token) {
  for (;;) {
    const index = messages.findIndex(m => m.id === id && m.event === event
      && (token === undefined || m.token === token))
    if (index >= 0) return messages.splice(index, 1)[0]
    const failed = messages.findIndex(m => m.id === id && m.event === 'error')
    if (failed >= 0 && event !== 'error') {
      const message = messages.splice(failed, 1)[0]
      throw Error(`worker failed before ${event}: ${message.message}`)
    }
    await delay(10)
  }
}
async function gone(target) {
  for (let i = 0; i < 500; i++) {
    if (!fs.existsSync(target)) return
    await delay(10)
  }
  throw Error('a superseded capture survived: ' + target)
}
async function clean() {
  for (let i = 0; i < 500; i++) {
    if (fs.readdirSync(process.env.XDG_RUNTIME_DIR).length === 0) return
    await delay(10)
  }
  throw Error('capture files survived cancellation')
}
async function main() {
  send({ command: 'capture', id: 1, outputs: ['DP-2', 'DP-1'] })
  const frames = await next(1, 'frames')
  assert.equal(frames.frames.length, 2)
  for (const frame of frames.frames) {
    // The capture is a private runtime file, never a screenshot-library entry.
    assert.deepEqual(fs.readFileSync(frame.path), fs.readFileSync(process.env.COLOR_FIXTURE))
    assert.equal(fs.statSync(frame.path).mode & 0o777, 0o600)
    assert.equal(fs.statSync(path.dirname(frame.path)).mode & 0o777, 0o700)
    assert.equal(frame.width, 8)
    assert.equal(frame.height, 4)
  }

  // Normalized points land on whole pixels, and one is the surface's far edge
  // rather than a pixel that exists.
  const readings = [
    [0, 0, [0xb4, 0xbe, 0xfe]],
    [0.5, 0.5, [4 * 16, 2 * 32, 128]],
    [0.99999, 0.99999, [7 * 16, 3 * 32, 128]],
    [1, 1, [7 * 16, 3 * 32, 128]],
    [0.2, 0.8, [1 * 16, 3 * 32, 128]],
  ]
  for (const [index, [x, y, expected]] of readings.entries()) {
    send({ command: 'sample', id: 1, token: index + 1, output: 'DP-1', x, y, commit: false })
    const sample = await next(1, 'sample', index + 1)
    assert.deepEqual([sample.r, sample.g, sample.b], expected, `sample at ${x},${y}`)
    assert.equal(sample.output, 'DP-1')
    assert.equal(sample.commit, false)
  }

  // Answers come back in the order they were asked for, which is what lets the
  // shell trust the last one it sees.
  for (let token = 20; token < 30; token++) {
    send({ command: 'sample', id: 1, token, output: 'DP-2', x: (token - 20) / 10, y: 0, commit: token === 29 })
  }
  const ordered = []
  for (let token = 20; token < 30; token++) ordered.push((await next(1, 'sample', token)).token)
  assert.deepEqual(ordered, [20, 21, 22, 23, 24, 25, 26, 27, 28, 29])

  // An unknown output and a superseded session are both answered with silence
  // rather than with a wrong pixel.
  send({ command: 'sample', id: 1, token: 40, output: 'missing', x: 0, y: 0, commit: false })
  send({ command: 'sample', id: 77, token: 41, output: 'DP-1', x: 0, y: 0, commit: false })
  send({ command: 'sample', id: 1, token: 42, output: 'DP-1', x: 0, y: 0, commit: false })
  const settled = await next(1, 'sample', 42)
  assert.deepEqual([settled.r, settled.g, settled.b], [0xb4, 0xbe, 0xfe])
  assert.equal(messages.filter(m => m.token === 40 || m.token === 41).length, 0)

  send({ command: 'cancel', id: 1 })
  await clean()

  send({ command: 'capture', id: 2, outputs: ['missing'] })
  await next(2, 'error')
  await clean()

  // Cancelling mid-capture stops grim instead of waiting it out.
  send({ command: 'capture', id: 3, outputs: ['slow'] })
  await delay(100)
  send({ command: 'cancel', id: 3 })
  await clean()

  // A second capture supersedes the first and releases its files.
  send({ command: 'capture', id: 4, outputs: ['DP-1'] })
  const first = await next(4, 'frames')
  send({ command: 'capture', id: 5, outputs: ['DP-1'] })
  const second = await next(5, 'frames')
  assert.notEqual(first.frames[0].path, second.frames[0].path)
  await gone(first.frames[0].path)
  assert.ok(fs.existsSync(second.frames[0].path))

  worker.stdin.end()
  assert.deepEqual(await exit, { code: 0, signal: null })
  await clean()

  const terminating = spawn(process.argv[2], [], { stdio: ['pipe', 'pipe', 'inherit'] })
  const finished = new Promise(resolve => terminating.on('exit', (code, signal) => resolve({ code, signal })))
  const termLines = readline.createInterface({ input: terminating.stdout })
  const captured = new Promise(resolve => termLines.on('line', line => {
    if (JSON.parse(line).event === 'frames') resolve()
  }))
  terminating.stdin.write(JSON.stringify({ command: 'capture', id: 6, outputs: ['DP-1'] }) + '\n')
  await captured
  terminating.kill('SIGTERM')
  assert.deepEqual(await finished, { code: 0, signal: null })
  await clean()
  clearTimeout(timer)
  console.log('colour capture, exact pixel sampling, ordering, supersession and cleanup passed')
}
main().catch(error => { clearTimeout(timer); worker.kill('SIGKILL'); console.error(error); process.exitCode = 1 })
NODE
