.import "../shared/Native.js" as Bridge

function overlap(a, b) { return Bridge.call("uri_picker.overlap", [a, b]) }
function caption(box, width, height, textWidth, textHeight, gap) { return Bridge.call("uri_picker.caption", [box, width, height, textWidth, textHeight, gap]) }
function layout(links, output, width, height, badgeWidth, badgeHeight, gap) { return Bridge.call("uri_picker.layout", [links, output, width, height, badgeWidth, badgeHeight, gap]) }
function selection(links, digits, complete, confirm) {
  var index = Bridge.call("uri_picker.selection", [links, digits, complete, confirm])
  return index === null ? null : links[index]
}
