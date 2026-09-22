#!/usr/bin/env bash
set -euo pipefail
worker=$1
font=$2
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Production Tesseract and the asynchronous strip pool: a three-line query,
# a wrap across y=512, ordinary prose, and two independent aligned links.
magick -size 1400x1100 xc:'#11111b' -font "$font" -pointsize 24 -fill '#cdd6f4' \
  -annotate +80+140 'https://example.org/docs/' \
  -annotate +80+174 'chapter?view=full' \
  -annotate +80+208 '&page=2#overview' \
  -annotate +80+502 'https://nixos.org/manual/' \
  -annotate +80+536 'stable/?view=plain' \
  -annotate +80+750 'https://example.com/start' \
  -annotate +80+784 'This is a normal paragraph.' \
  -annotate +80+890 'https://example.org/one' \
  -annotate +80+924 'https://example.org/two' \
  -depth 8 "$work/wrapped.ppm"
magick -size 1400x650 xc:'#11111b' -font "$font" -pointsize 24 -fill '#cdd6f4' \
  -annotate +80+100 'https://256.1.1.1/path' \
  -annotate +80+180 'https://example..com/path' \
  -annotate +80+260 'https://example.org:70000/path' \
  -annotate +80+340 'https://example.org/bad%%2X' \
  -annotate +80+540 'https://example.org/valid' \
  -depth 8 "$work/invalid.ppm"
# Sparse OCR may split in the middle of an identifier rather than at URI
# punctuation. Retain the full query, including the trailing parameter.
magick -size 1000x400 xc:'#11111b' -font "$font" -pointsize 24 -fill '#cdd6f4' \
  -annotate +120+140 'https://example.org/docs/' \
  -annotate +120+178 'chapter?view=full&page=2' \
  -pointsize 18 -fill '#a6adc8' \
  -annotate +120+280 'An ordinary paragraph stays outside the highlights.' \
  -depth 8 "$work/query-split.ppm"
node - "$worker" "$work" <<'NODE'
const {spawnSync} = require('node:child_process')
const assert = require('node:assert/strict')
function scan(name) {
  const r = spawnSync(process.argv[2], ['--image', process.argv[3] + '/' + name + '.ppm'], {encoding:'utf8', timeout:15000})
  assert.ifError(r.error)
  assert.equal(r.status, 0, r.stderr)
  const events = r.stdout.trim().split('\n').map(JSON.parse)
  assert.equal(events.at(-1).failedAreas, 0)
  const links = events.flatMap(e => e.links || [])
  assert.equal(events.at(-1).count, links.length)
  assert.equal(new Set(links.map(l => l.number)).size, links.length)
  return links
}
const expected = [
  ['https://example.org/docs/chapter?view=full&page=2#overview',3],
  ['https://nixos.org/manual/stable/?view=plain',2],
  ['https://example.com/start',1],
  ['https://example.org/one',1],
  ['https://example.org/two',1]
]
// Repeat to exercise different strip-completion orders without hint changes.
for (let n = 0; n < 2; n++) {
  const links = scan('wrapped')
  assert.deepEqual(links.map(l => l.uri).sort(), expected.map(([uri]) => uri).sort())
  for (const [uri, count] of expected) {
    const link = links.find(l => l.uri === uri)
    assert.equal(link.text, uri)
    assert.equal(link.regions.length, count, uri)
    assert.deepEqual(link.regions[0], {x0:link.x0,y0:link.y0,w:link.w,h:link.h})
    for (const r of link.regions) {
      assert.ok(r.x0 >= 0 && r.y0 >= 0 && r.w > 0 && r.h > 0)
      assert.ok(r.x0 + r.w <= 1 && r.y0 + r.h <= 1)
    }
  }
}
assert.deepEqual(scan('invalid').map(l => l.uri), ['https://example.org/valid'])
const split = scan('query-split')
assert.equal(split.length, 1)
// Some English OCR/font combinations read the split final l as L. Preserve
// those recognized bytes; never compensate by truncating or guessing a glyph.
assert.match(split[0].uri, /^https:\/\/example\.org\/docs\/chapter\?view=ful[lL]&page=2$/)
assert.equal(split[0].regions.length, 2)
assert.ok(split[0].regions[1].x0 + split[0].regions[1].w > 0.40, 'highlight must include the complete query')
console.log('URI real OCR multiline, cross-strip, prose separation and malformed-link checks passed')
NODE
