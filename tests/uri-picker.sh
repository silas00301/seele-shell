#!/usr/bin/env bash
set -euo pipefail
export OMP_THREAD_LIMIT=1 OMP_NUM_THREADS=1

worker=$1
font=$2
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/bin" "$work/runtime"

# Dim browser address text alongside bright chrome. The old whole-image
# threshold misses this; keep the fixture synthetic rather than saving a capture.
magick -size 1272x40 xc:'#555779' -fill none -stroke '#aeb1c8' \
  -draw 'roundrectangle 1,1 1270,37 6,6' -stroke none \
  -font "$font" -pointsize 16 -fill '#80839f' \
  -annotate +43+25 'www.example.org/channel' -fill '#ffffff' \
  -draw 'rectangle 1214,10 1229,24 rectangle 1243,10 1256,24' \
  -depth 8 "$work/address-bar.ppm"
node - "$worker" "$work/address-bar.ppm" <<'NODE'
const { spawnSync } = require('node:child_process')
const assert = require('node:assert/strict')
const result = spawnSync(process.argv[2], ['--image', process.argv[3]], { encoding: 'utf8', timeout: 15000 })
assert.ifError(result.error)
assert.equal(result.status, 0, result.stderr)
const messages = result.stdout.trim().split('\n').map(line => JSON.parse(line))
const links = messages.flatMap(message => message.links || [])
assert.deepEqual(links.map(link => link.uri), ['https://www.example.org/channel'])
assert.ok(links[0].x0 > 0.03 && links[0].x0 < 0.04)
assert.equal(messages.at(-1).failedAreas, 0)
console.log('URI dim browser address-bar OCR passed')
NODE

# Real OCR, including a line crossing the 512px strip boundary. No captured
# personal data, network access or graphical session is needed for this test.
magick -size 1400x1100 xc:'#11111b' -font "$font" -pointsize 30 -fill '#cdd6f4' \
  -annotate +90+130 'https://example.org/alpha' \
  -annotate +90+520 'https://nixos.org' \
  -annotate +90+950 'https://example.com/beta' \
  -pointsize 20 -annotate +90+300 'determinate.url = "https://flakehub.com/f/DeterminateSystems/determinate/3";' \
  -annotate +90+620 'hello. World' \
  -annotate +90+670 'hello. Com' \
  -pointsize 16 -annotate +90+750 'https://example.org/docs/page-4?view=plain#section' \
  -depth 8 "$work/frame.ppm"

# Decode codes from the same frozen frame as ordinary links. The URI QR spans
# an OCR seam; the second QR preserves markup, Unicode and embedded newlines.
zint -b QRCODE --quietzones --scale=2 -d 'https://example.org/code.;!' -o "$work/uri.png"
printf '<b>Grüße</b>\nsecond line\n' > "$work/payload.txt"
zint -b QRCODE --quietzones --scale=2 --eci=26 -i "$work/payload.txt" -o "$work/text.png"
zint -b CODE128 --quietzones --notext --scale=2 -d 'SKU-042' -o "$work/barcode.png"
magick "$work/frame.ppm" \
  "$work/uri.png" -geometry +950+430 -composite \
  "$work/text.png" -geometry +950+60 -composite \
  "$work/barcode.png" -geometry +850+850 -composite \
  -depth 8 "$work/frame.ppm"
printf '#!%s\n' "$(command -v bash)" > "$work/bin/grim"
cat >> "$work/bin/grim" <<'GRIM'
set -euo pipefail
test "$1" = -t && test "$2" = ppm && test "$3" = -o && test "$5" = -
case "$4" in
  DP-1|DP-2) cat "$URI_FIXTURE" ;;
  slow) exec sleep 30 ;;
  *) exit 1 ;;
esac
GRIM
chmod +x "$work/bin/grim"

PATH="$work/bin:$PATH" XDG_RUNTIME_DIR="$work/runtime" URI_FIXTURE="$work/frame.ppm" \
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
const timer = setTimeout(() => { worker.kill('SIGKILL'); throw Error('URI integration test timed out') }, 60000)
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
const send = message => worker.stdin.write(JSON.stringify(message) + '\n')
async function next(id, event) {
  for (;;) {
    const index = messages.findIndex(m => m.id === id && m.event === event)
    if (index >= 0) return messages.splice(index, 1)[0]
    const failed = messages.findIndex(m => m.id === id && m.event === 'error')
    if (failed >= 0 && event !== 'error') {
      const message = messages.splice(failed, 1)[0]
      throw Error(`worker failed before ${event}: ${message.message}`)
    }
    await delay(10)
  }
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
    assert.deepEqual(fs.readFileSync(frame.path), fs.readFileSync(process.env.URI_FIXTURE))
    assert.equal(fs.statSync(frame.path).mode & 0o777, 0o600)
    assert.equal(fs.statSync(path.dirname(frame.path)).mode & 0o777, 0o700)
    assert.equal(frame.width, 1400)
    assert.equal(frame.height, 1100)
  }
  const done = await next(1, 'done')
  assert.equal(done.failedAreas, 0)
  const links = messages.filter(m => m.id === 1 && m.event === 'links').flatMap(m => m.links)
  assert.equal(new Set(links.map(l => l.number)).size, links.length)
  for (const output of ['DP-1', 'DP-2']) {
    const expected = ['https://example.org/alpha', 'https://nixos.org', 'https://example.com/beta', 'https://example.org/docs/page-4?view=plain#section', 'https://flakehub.com/f/DeterminateSystems/determinate/3']
    assert.deepEqual(links.filter(l => l.output === output && !l.code).map(l => l.uri).sort(), [...expected].sort())
    const codes = links.filter(l => l.output === output && l.code)
    assert.deepEqual(codes.map(l => l.text).sort(), ['https://example.org/code.;!', '<b>Grüße</b>\nsecond line\n', 'SKU-042'].sort())
    for (const code of codes) {
      assert.equal(code.uri, code.text.startsWith('https:') ? code.text : '')
      assert.ok(code.x0 > 0.6 && code.x0 + code.w <= 1)
      assert.ok(code.y0 >= 0 && code.y0 + code.h <= 1)
      if (code.uri) assert.ok(code.y0 < 512 / 1100 && code.y0 + code.h > 512 / 1100)
    }
    for (const uri of expected) {
      const found = links.filter(l => l.output === output && l.uri === uri)
      assert.equal(found.length, 1, `${output}: ${uri}`)
      const link = found[0]
      assert.ok(link.x0 >= 0 && link.x0 + link.w <= 1)
      assert.ok(link.y0 >= 0 && link.y0 + link.h <= 1)
      if (uri === 'https://nixos.org') assert.ok(link.y0 < 512 / 1100 && link.y0 + link.h > 512 / 1100)
    }
  }
  console.log(`URI OCR: ${links.length} links on two outputs; capture ${frames.captureMs}ms, total ${done.elapsedMs}ms`)
  send({ command: 'cancel', id: 1 })
  await clean()
  send({ command: 'capture', id: 11, outputs: ['DP-1', 'DP-2'] })
  const warmFrames = await next(11, 'frames')
  const warmDone = await next(11, 'done')
  assert.equal(warmDone.count, done.count)
  console.log(`URI warm OCR: capture ${warmFrames.captureMs}ms, total ${warmDone.elapsedMs}ms`)
  send({ command: 'cancel', id: 11 })
  await clean()
  send({ command: 'capture', id: 2, outputs: ['DP-1'] })
  await next(2, 'frames')
  send({ command: 'cancel', id: 2 })
  await clean()
  send({ command: 'capture', id: 3, outputs: ['missing'] })
  await next(3, 'error')
  await clean()
  send({ command: 'capture', id: 4, outputs: ['slow'] })
  await delay(100)
  send({ command: 'cancel', id: 4 })
  await clean()
  send({ command: 'capture', id: 5, outputs: ['DP-1'] })
  await next(5, 'frames')
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
  console.log('URI capture, cancellation, failure and EOF cleanup passed')
}
main().catch(error => { clearTimeout(timer); worker.kill('SIGKILL'); console.error(error); process.exitCode = 1 })
NODE
