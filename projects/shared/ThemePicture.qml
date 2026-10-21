import QtQuick

// The picture a theme is linked to, cropped to fill and clipped to its
// corners. It is drawn by a Canvas because a rounded clip there needs no
// shader, so the picture draws the same under every scene graph backend,
// the software one the pixel baselines and the panel tests render with
// included. The image is decoded once, about as large as `sourceSize` needs:
// an Image reads its shape at that width and the Canvas draws the same cached
// decode. A file that cannot be read draws nothing, which leaves whatever
// lies beneath.
Canvas {
  id: picture

  // An absolute path, as the theme catalog gives it; empty draws nothing.
  property string path: ""
  // The box the image is decoded to cover. Keep it steady while the item
  // animates, so a resize repaints rather than decoding again.
  property size sourceSize: Qt.size(width, height)
  property real topLeftRadius: 0
  property real topRightRadius: 0
  property real bottomLeftRadius: 0
  property real bottomRightRadius: 0
  // Whether the picture is on screen.
  readonly property bool shown: picture.drawn

  // Each segment escaped, as the wallpaper's own, so a `#`, `?` or `%` in a
  // name reaches Qt as part of the path.
  readonly property string url: picture.path === "" ? "" : "file://" + picture.path.split("/").map(encodeURIComponent).join("/")
  readonly property real ratio: Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1
  // Wide enough to cover the box for any landscape image a desk shows, in
  // device pixels; the height follows the image's own shape.
  readonly property int decodedWidth: Math.ceil((picture.sourceSize.width + picture.sourceSize.height) * picture.ratio)
  readonly property real aspect: probe.status === Image.Ready && probe.implicitHeight > 0 ? probe.implicitWidth / probe.implicitHeight : 0
  property string loaded: ""
  property bool drawn: false

  function request() {
    if (picture.loaded !== "") picture.unloadImage(picture.loaded)
    picture.loaded = ""
    if (picture.url !== "" && picture.aspect > 0) {
      picture.loaded = picture.url
      picture.loadImage(picture.url, Qt.size(picture.decodedWidth, 0))
    }
    picture.requestPaint()
  }
  // One corner's arc, from the edge before it to the edge after it.
  function corner(context, x, y, toX, toY, radius) {
    if (radius > 0) context.arcTo(x, y, toX, toY, radius)
    else context.lineTo(x, y)
  }

  onUrlChanged: request()
  onAspectChanged: request()
  onDecodedWidthChanged: request()
  onImageLoaded: requestPaint()
  // A Canvas does not follow what onPaint reads, so every input repaints it:
  // the size, and the corners a theme with another radius gives it.
  onWidthChanged: requestPaint()
  onHeightChanged: requestPaint()
  onTopLeftRadiusChanged: requestPaint()
  onTopRightRadiusChanged: requestPaint()
  onBottomLeftRadiusChanged: requestPaint()
  onBottomRightRadiusChanged: requestPaint()
  Component.onDestruction: if (picture.loaded !== "") picture.unloadImage(picture.loaded)
  onPaint: {
    var context = getContext("2d")
    context.reset()
    picture.drawn = picture.loaded !== "" && picture.isImageLoaded(picture.loaded) && picture.width > 0 && picture.height > 0
    if (!picture.drawn) return
    var right = picture.width, bottom = picture.height
    context.beginPath()
    context.moveTo(picture.topLeftRadius, 0)
    picture.corner(context, right, 0, right, bottom, picture.topRightRadius)
    picture.corner(context, right, bottom, 0, bottom, picture.bottomRightRadius)
    picture.corner(context, 0, bottom, 0, 0, picture.bottomLeftRadius)
    picture.corner(context, 0, 0, right, 0, picture.topLeftRadius)
    context.closePath()
    context.clip()
    var scale = Math.max(right / picture.aspect, bottom)
    var drawWidth = scale * picture.aspect, drawHeight = scale
    context.drawImage(picture.loaded, (right - drawWidth) / 2, (bottom - drawHeight) / 2, drawWidth, drawHeight)
  }

  // Reads the image's shape from the decode the Canvas then draws.
  Image {
    id: probe
    visible: false
    asynchronous: true
    source: picture.decodedWidth > 0 ? picture.url : ""
    sourceSize.width: picture.decodedWidth
  }
}
