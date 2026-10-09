.pragma library

// Material 3 Expressive's shape library, drawn by Canvas. Every shape in the
// set is star-shaped about its centre, so each one is a radius sampled at the
// same angles, and a morph between two of them is those radii blended sample
// by sample. That is what lets the loading indicator flow from a burst into a
// pill without a seam, and lets a mark change shape when its state changes.
var samples = 144

function circular(fn) {
  var radii = []
  for (var index = 0; index < samples; ++index) radii.push(fn(index / samples * Math.PI * 2))
  return radii
}

// A scalloped outline: `lobes` soft points pushed out of a circle, with the
// valleys between them `depth` of the radius deep. Four-lobed shapes put
// their lobes on the diagonals, as Material draws them, so they read as a
// softened square rather than as a diamond.
function scallop(lobes, depth) {
  var offset = lobes === 4 ? Math.PI / 4 : 0
  return circular(function(angle) { return 1 - depth * (1 - Math.cos(lobes * (angle - offset))) / 2 })
}

// A superellipse: a squircle at `power` 4, an oval or a pill when the axes
// differ. Material's square and pill shapes are rounded rather than cut.
function superellipse(width, height, power) {
  return circular(function(angle) {
    var x = Math.pow(Math.abs(Math.cos(angle)) / width, power)
    var y = Math.pow(Math.abs(Math.sin(angle)) / height, power)
    return Math.pow(x + y, -1 / power)
  })
}

// A regular polygon with its corners rounded by averaging each radius with
// its neighbours, which turns every corner into an arc and leaves the flats.
function polygon(sides, rounding) {
  var sector = Math.PI * 2 / sides
  var sharp = circular(function(angle) {
    var local = ((angle + Math.PI / 2) % sector + sector) % sector - sector / 2
    return Math.cos(sector / 2) / Math.cos(local)
  })
  var reach = Math.max(1, Math.round(samples * rounding / sides))
  var rounded = []
  for (var index = 0; index < samples; ++index) {
    var sum = 0
    for (var offset = -reach; offset <= reach; ++offset) sum += sharp[(index + offset + samples) % samples]
    rounded.push(sum / (reach * 2 + 1))
  }
  return rounded
}

var library = {
  circle: circular(function() { return 1 }),
  square: superellipse(1, 1, 4),
  oval: superellipse(1, 0.74, 2),
  pill: superellipse(1, 0.62, 3),
  pentagon: polygon(5, 0.35),
  cookie4: scallop(4, 0.18),
  cookie6: scallop(6, 0.13),
  cookie9: scallop(9, 0.1),
  cookie12: scallop(12, 0.07),
  clover4: scallop(4, 0.34),
  sunny: scallop(8, 0.12),
  softBurst: scallop(10, 0.16)
}

// The order Material's loading indicator walks through its shapes.
var loadingSequence = ["softBurst", "cookie9", "pentagon", "pill", "sunny", "cookie4", "oval"]

function radii(name) {
  return library[name] || library.circle
}

function blend(from, to, amount) {
  var a = radii(from)
  var b = radii(to)
  var result = []
  for (var index = 0; index < samples; ++index) result.push(a[index] + (b[index] - a[index]) * amount)
  return result
}

// Fills `shape` (an array of radii) centred in a width by height box, scaled
// so its widest extent touches the box, and turned by `rotation` degrees.
function fill(context, shape, width, height, rotation, color) {
  var turn = (rotation || 0) * Math.PI / 180
  var extentX = 0
  var extentY = 0
  for (var index = 0; index < samples; ++index) {
    var angle = index / samples * Math.PI * 2 + turn
    extentX = Math.max(extentX, Math.abs(Math.cos(angle) * shape[index]))
    extentY = Math.max(extentY, Math.abs(Math.sin(angle) * shape[index]))
  }
  var scale = Math.min(width / 2 / extentX, height / 2 / extentY)
  context.beginPath()
  for (var point = 0; point < samples; ++point) {
    var theta = point / samples * Math.PI * 2 + turn
    var x = width / 2 + Math.cos(theta) * shape[point] * scale
    var y = height / 2 + Math.sin(theta) * shape[point] * scale
    if (point === 0) context.moveTo(x, y)
    else context.lineTo(x, y)
  }
  context.closePath()
  context.fillStyle = color
  context.fill()
}
