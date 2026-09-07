// No timeout guesses for numbers: 1 and 10 can coexist. A complete, unique
// number opens immediately after scanning; Enter can open an existing match
// while other regions are still being recognized.
function selection(links, digits, complete, confirm) {
  if (!digits) return null
  var exact = null
  var matches = 0
  for (var i = 0; i < links.length; i++) {
    var number = String(links[i].number)
    if (number.indexOf(digits) === 0) matches++
    if (number === digits) exact = links[i]
  }
  return exact && (confirm || (complete && matches === 1)) ? exact : null
}

function overlap(a, b) {
  return Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x))
    * Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y))
}

// Prefer the left or right margin of the URI. Dense rows and screen edges
// fall back above/below it, minimizing collisions with both links and badges.
function layout(links, output, width, height, badgeWidth, badgeHeight, gap) {
  var local = links.filter(function(link) { return link.output === output })
  var boxes = local.map(function(link) {
    return { x: link.x0 * width, y: link.y0 * height, w: link.w * width, h: link.h * height }
  })
  var placed = []
  var result = {}
  for (var i = 0; i < local.length; i++) {
    var box = boxes[i]
    var candidates = [
      { x: box.x - badgeWidth - gap, y: box.y + (box.h - badgeHeight) / 2 },
      { x: box.x + box.w + gap, y: box.y + (box.h - badgeHeight) / 2 },
      { x: box.x, y: box.y - badgeHeight - gap },
      { x: box.x, y: box.y + box.h + gap }
    ]
    var best = null
    var bestScore = Infinity
    for (var j = 0; j < candidates.length; j++) {
      var raw = candidates[j]
      var candidate = { x: Math.max(0, Math.min(raw.x, width - badgeWidth)),
        y: Math.max(0, Math.min(raw.y, height - badgeHeight)), w: badgeWidth, h: badgeHeight }
      var score = Math.abs(raw.x - candidate.x) + Math.abs(raw.y - candidate.y)
      for (var k = 0; k < boxes.length; k++) score += overlap(candidate, boxes[k])
      for (var n = 0; n < placed.length; n++) score += overlap(candidate, placed[n]) * 10
      if (score < bestScore) { best = candidate; bestScore = score }
    }
    placed.push(best)
    result[local[i].number] = best
  }
  return result
}
