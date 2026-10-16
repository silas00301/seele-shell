import QtQuick

// The desktop's wallpaper, cropped to fill. Changing `path` loads the new image
// behind the one on screen and shows it only once it has decoded, so switching
// to a theme with its own wallpaper, or back to one without, goes from one
// picture straight to the next instead of showing the bare colour underneath
// while the file loads. A file that cannot be read draws nothing, which leaves
// that colour: the contract's fallback for a missing wallpaper.
Item {
  id: wallpaper

  // An absolute path, as Theme.wallpaperImage gives it; empty draws nothing.
  property string path: ""
  // The path whose image is on screen, once it has loaded or failed to.
  readonly property string shownPath: front
  readonly property int status: flipped ? second.status : first.status

  property string front: ""
  property bool waiting: false
  property bool flipped: false
  readonly property Image incoming: flipped ? first : second

  // Each segment is escaped, so a `#`, `?` or `%` in a file's name stays part
  // of the name instead of starting a fragment or query.
  function source(path) {
    return path === "" ? "" : "file://" + path.split("/").map(encodeURIComponent).join("/")
  }
  function request() {
    if (path === front) {
      waiting = false
      incoming.source = ""
      return
    }
    waiting = true
    incoming.source = source(path)
    settle()
  }
  // The incoming image replaces the shown one as soon as it is no longer
  // loading: decoded, or known to be unreadable.
  function settle() {
    if (!waiting || incoming.status === Image.Loading) return
    var previous = flipped ? second : first
    waiting = false
    front = path
    flipped = !flipped
    previous.source = ""
  }

  onPathChanged: request()
  Component.onCompleted: {
    front = path
    first.source = source(path)
  }

  Image {
    id: first
    anchors.fill: parent
    visible: !wallpaper.flipped
    fillMode: Image.PreserveAspectCrop
    asynchronous: true
    cache: true
    onStatusChanged: wallpaper.settle()
  }
  Image {
    id: second
    anchors.fill: parent
    visible: wallpaper.flipped
    fillMode: Image.PreserveAspectCrop
    asynchronous: true
    cache: true
    onStatusChanged: wallpaper.settle()
  }
}
