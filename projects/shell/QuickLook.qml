import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Widgets
import "../shared" as Shared
import "quicklook.js" as QuickLook

// Quick Look: one centered window that shows what a highlighted file is,
// without starting the application that owns it.
//
// The panel never contains an editable control. That is what makes Space its
// dismissal — the same key that opened it — instead of a shortcut competing
// with typing: nothing inside here has a caret to steal. Every surface that
// can open it has the same obligation, which is why the compositor binds no
// global Space and the Yazi binding lives in that file manager's own
// selection layer rather than over its prompt.
Scope {
  id: preview

  required property var theme

  property bool active: false
  property string screenName: ""
  property int generation: 0
  property var paths: []
  property var items: []
  property int index: 0
  property int pageNumber: 1
  property string pagePath: ""
  property string pageError: ""
  property bool pageLoading: false
  property bool loading: false
  property string error: ""
  // Playback is explicit and belongs to the file in front of the reader, so
  // it never survives a move to the next one.
  property bool mediaPlaying: false

  readonly property var item: index >= 0 && index < items.length ? items[index] : null
  readonly property var summary: item ? QuickLook.summary(item)
    : ({ glyph: "", kind: "", title: "", detail: "", note: "", drawable: false })
  readonly property string footer: item ? QuickLook.hint(item, items.length) : ""
  readonly property string counter: items.length > 1 ? (index + 1) + " of " + items.length : ""

  function send(message) {
    if (worker.running) worker.write(JSON.stringify(message) + "\n")
  }

  function open(screen, list) {
    var requested = (list || []).filter(function (path) { return String(path || "") !== "" })
    if (requested.length === 0) return
    generation++
    screenName = String(screen || "")
    paths = requested
    active = true
    loading = true
    error = ""
    items = []
    index = 0
    resetPage()
    watchdog.restart()
    if (worker.running) request()
    else worker.running = true
  }

  function request() {
    send({ command: "open", id: generation, paths: paths })
  }

  function close() {
    if (!active) return
    active = false
    loading = false
    items = []
    paths = []
    resetPage()
    watchdog.stop()
    // The worker owns the rendered pages; cancelling is what deletes them.
    send({ command: "cancel", id: generation })
  }

  // Leaving a preview releases both the page it drew and any playback it
  // started, so the next file always starts silent and on its first page.
  function resetPage() {
    mediaPlaying = false
    pageNumber = 1
    pagePath = ""
    pageError = ""
    pageLoading = false
  }

  function fail(message) {
    error = message
    loading = false
    watchdog.stop()
  }

  function accept(message) {
    if (!active || message.id !== generation) return
    if (message.event === "items") {
      loading = false
      watchdog.stop()
      items = message.items || []
      index = 0
      if (items.length === 0) fail("Nothing to preview")
      else showCurrent()
      return
    }
    if (message.event !== "page") return
    // A page that arrived for a file or a page the reader has already left is
    // not the picture in front of them.
    if (message.index !== index || message.page !== pageNumber) return
    pageLoading = false
    pageError = String(message.error || "")
    pagePath = pageError === "" ? String(message.path || "") : ""
  }

  function showCurrent() {
    resetPage()
    if (!item || item.kind !== "pdf" || String(item.error || "") !== "") return
    requestPage(1)
  }

  function requestPage(number) {
    pageNumber = number
    pagePath = ""
    pageError = ""
    pageLoading = true
    send({ command: "page", id: generation, index: index, page: number })
  }

  function move(delta) {
    if (items.length < 2) return
    index = QuickLook.step(items.length, index, delta)
    showCurrent()
  }

  function movePage(delta) {
    if (!item || item.kind !== "pdf") return
    var next = QuickLook.page(Number(item.pages || 0), pageNumber, delta)
    if (next > 0 && next !== pageNumber) requestPage(next)
  }

  function copyPath() {
    if (!item) return
    clipboard.payload = String(item.path || "")
    if (clipboard.payload === "") return
    clipboard.stdinEnabled = true
    clipboard.running = true
  }

  // Handing the file to its own application is the way out of a preview, so
  // the preview closes with it rather than staying behind the window it opened.
  function launch() {
    if (!item || item.kind === "unavailable") return
    var path = String(item.path || "")
    close()
    if (path !== "") Quickshell.execDetached(["xdg-open", path])
  }

  // Only a body that is actually a scrollable view answers these keys; a
  // picture or a page has nothing to move.
  function scrollable(body) {
    return !!body && body.contentHeight !== undefined && body.contentY !== undefined
  }

  function scroll(body, amount) {
    if (!scrollable(body)) return
    var limit = Math.max(0, body.contentHeight - body.height)
    body.contentY = Math.max(0, Math.min(limit, body.contentY + amount))
  }

  // `body` is the loaded preview of the window the key arrived at, so a text
  // preview scrolls while a document turns pages under the same keys.
  function key(event, body) {
    event.accepted = true
    var paging = !!item && item.kind === "pdf"
    if (event.key === Qt.Key_Escape || event.key === Qt.Key_Space) {
      if (!event.isAutoRepeat) close()
      return
    }
    if (event.modifiers & Qt.ControlModifier) {
      if (event.key === Qt.Key_C && !event.isAutoRepeat) copyPath()
      return
    }
    if (event.modifiers & (Qt.AltModifier | Qt.MetaModifier)) return
    switch (event.key) {
    case Qt.Key_Return:
    case Qt.Key_Enter:
      if (!event.isAutoRepeat) launch()
      return
    case Qt.Key_P:
      if (!event.isAutoRepeat && !!item && (item.kind === "audio" || item.kind === "video"))
        mediaPlaying = !mediaPlaying
      return
    case Qt.Key_Right:
    case Qt.Key_L:
      move(1)
      return
    case Qt.Key_Left:
    case Qt.Key_H:
      move(-1)
      return
    case Qt.Key_Down:
    case Qt.Key_J:
      if (paging) movePage(1)
      else scroll(body, preview.theme.rowHeight)
      return
    case Qt.Key_Up:
    case Qt.Key_K:
      if (paging) movePage(-1)
      else scroll(body, -preview.theme.rowHeight)
      return
    case Qt.Key_PageDown:
      if (paging) movePage(1)
      else scroll(body, preview.scrollable(body) ? body.height : 0)
      return
    case Qt.Key_PageUp:
      if (paging) movePage(-1)
      else scroll(body, preview.scrollable(body) ? -body.height : 0)
      return
    case Qt.Key_Home:
      if (paging) movePage(-Number(item.pages || 0))
      else scroll(body, preview.scrollable(body) ? -body.contentHeight : 0)
      return
    case Qt.Key_End:
      if (paging) movePage(Number(item.pages || 0))
      else scroll(body, preview.scrollable(body) ? body.contentHeight : 0)
      return
    default:
      return
    }
  }

  Process {
    id: clipboard
    property string payload: ""
    command: ["wl-copy", "--type", "text/plain;charset=utf-8"]
    onStarted: {
      write(payload)
      stdinEnabled = false
      payload = ""
    }
  }

  Process {
    id: worker
    command: ["seele-quicklook"]
    running: false
    stdinEnabled: true
    onStarted: if (preview.active) preview.request()
    stdout: SplitParser {
      onRead: data => {
        try { preview.accept(JSON.parse(data)) }
        catch (_) { preview.fail("This preview is unavailable") }
      }
    }
    onRunningChanged: if (!running && preview.active) preview.fail("This preview is unavailable")
  }

  Timer {
    id: watchdog
    interval: 15000
    onTriggered: preview.fail("Reading this file timed out")
  }

  Connections {
    target: Quickshell
    function onScreensChanged() {
      // The window is pinned to the output it opened on; a rearranged desktop
      // is a new one.
      if (preview.active) preview.close()
    }
  }

  Variants {
    model: Quickshell.screens

    PanelWindow {
      id: window
      required property var modelData
      readonly property bool panelActive: preview.active && preview.screenName === modelData.name

      screen: modelData
      visible: true
      implicitWidth: Math.min(preview.theme.quickLookWidth, modelData.width - preview.theme.panelMargin * 2)
      implicitHeight: Math.min(preview.theme.quickLookHeight, modelData.height - preview.theme.panelMargin * 2)
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      mask: Region {
        width: window.panelActive ? window.width : 0
        height: window.panelActive ? window.height : 0
      }
      WlrLayershell.layer: WlrLayer.Overlay
      // The surface stays mapped so the preview appears without a round trip.
      // Exclusive focus is what makes Space a dismissal rather than a key the
      // window underneath also receives.
      WlrLayershell.keyboardFocus: panelActive ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
      WlrLayershell.namespace: "seele-shell-quicklook"

      FocusScope {
        id: scope
        anchors.fill: parent
        focus: window.panelActive
        Keys.onPressed: event => preview.key(event, body.item)
        // The compositor can grant focus after the surface was already mapped,
        // so claim it whenever it arrives rather than only when opening.
        onActiveFocusChanged: if (activeFocus && window.panelActive) forceActiveFocus()

        Shared.PanelSurface {
          anchors.fill: parent
          theme: preview.theme
          visible: window.panelActive

          Item {
            anchors.fill: parent
            anchors.margins: preview.theme.panelMargin

            Shared.PanelHeader {
              id: header
              theme: preview.theme
              width: parent.width
              anchors.top: parent.top
              glyph: preview.summary.glyph || "󰈈"
              title: preview.summary.title || "Quick Look"
              detail: preview.error !== "" ? preview.error
                : preview.loading ? "Reading…"
                : preview.summary.detail
              detailColor: preview.error !== "" ? preview.theme.red : preview.theme.subtext

              Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: preview.counter !== ""
                text: preview.counter
                textFormat: Text.PlainText
                color: preview.theme.overlay
                font.family: preview.theme.fontFamily
                font.pixelSize: preview.theme.textLabel
              }
            }

            ClippingRectangle {
              id: well
              anchors.top: header.bottom
              anchors.topMargin: preview.theme.panelSpacing
              anchors.bottom: hint.top
              anchors.bottomMargin: preview.theme.panelSpacing
              width: parent.width
              radius: preview.theme.radius
              color: preview.theme.wellColor
              clip: true
              HoverHandler { id: bodyHover }

              Loader {
                id: body
                anchors.fill: parent
                active: window.panelActive
                sourceComponent: preview.error !== "" || preview.loading || !preview.item ? stateView
                  : !preview.summary.drawable ? stateView
                  : preview.item.kind === "image" ? pictureView
                  : preview.item.kind === "animation" ? animationView
                  : preview.item.kind === "pdf" ? documentView
                  : preview.item.kind === "directory" ? folderView
                  : preview.item.kind === "audio" || preview.item.kind === "video" ? mediaView
                  : textView
              }
            }

            Text {
              id: hint
              anchors.bottom: parent.bottom
              width: parent.width
              text: preview.footer
              textFormat: Text.PlainText
              elide: Text.ElideRight
              color: preview.theme.overlay
              font.family: preview.theme.fontFamily
              font.pixelSize: preview.theme.textCaption
            }
          }
        }
      }

      // One body per kind. Each is a component rather than a visibility
      // toggle, so leaving a preview releases the decoder and the file with it.
      Component {
        id: stateView
        Shared.EmptyState {
          theme: preview.theme
          glyph: preview.loading ? "" : preview.summary.glyph || "󰈔"
          title: preview.error !== "" ? preview.error
            : preview.loading ? "Reading…"
            : preview.summary.note !== "" ? preview.summary.note
            : "This kind of file has no preview"
          // The header already carries the facts; repeating the whole line
          // here would say "Unavailable" twice about the same file.
          detail: preview.error !== "" || preview.loading
            || preview.summary.kind === "unavailable" ? "" : preview.summary.detail
          tint: preview.error !== "" ? preview.theme.red : preview.theme.overlay
        }
      }

      Component {
        id: pictureView
        Item {
          Image {
            id: picture
            anchors.fill: parent
            anchors.margins: preview.theme.cardPadding
            source: preview.item ? QuickLook.url(String(preview.item.path || "")) : ""
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            cache: false
            // Both dimensions bound the decode, so an enormous photograph is
            // scaled on the way in instead of after it is already in memory.
            sourceSize.width: Math.max(1, Math.round(width))
            sourceSize.height: Math.max(1, Math.round(height))
          }
          Shared.EmptyState {
            anchors.fill: parent
            theme: preview.theme
            visible: picture.status === Image.Error
            glyph: "󰀦"
            title: "This image cannot be drawn"
            detail: preview.summary.detail
          }
        }
      }

      Component {
        id: animationView
        AnimatedImage {
          anchors.fill: parent
          anchors.margins: preview.theme.cardPadding
          source: preview.item ? QuickLook.url(String(preview.item.path || "")) : ""
          fillMode: Image.PreserveAspectFit
          cache: false
          playing: true
          sourceSize.width: Math.max(1, Math.round(width))
          sourceSize.height: Math.max(1, Math.round(height))
        }
      }

      Component {
        id: documentView
        Item {
          Image {
            id: page
            anchors.fill: parent
            anchors.margins: preview.theme.cardPadding
            source: preview.pagePath !== "" ? QuickLook.url(preview.pagePath) : ""
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            cache: false
            sourceSize.width: Math.max(1, Math.round(width))
            sourceSize.height: Math.max(1, Math.round(height))
          }
          Shared.EmptyState {
            anchors.fill: parent
            theme: preview.theme
            visible: preview.pageError !== "" || preview.pageLoading || preview.pagePath === ""
            glyph: preview.pageLoading ? "" : "󰀦"
            title: preview.pageLoading ? "Drawing page " + preview.pageNumber + "…"
              : preview.pageError !== "" ? preview.pageError
              : "This page cannot be drawn"
          }
          // The page it is drawn over can be any colour, so the counter
          // carries its own material rather than trusting the paper.
          Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: preview.theme.spaceMedium
            visible: !!preview.item && Number(preview.item.pages || 0) > 1
              && preview.pagePath !== ""
            width: pageCount.implicitWidth + preview.theme.spaceLarge * 2
            height: preview.theme.chipHeight - preview.theme.spaceTight
            radius: preview.theme.radiusSmall
            color: preview.theme.floatColor
            border.width: preview.theme.hairline
            border.color: preview.theme.panelBorder
            Text {
              id: pageCount
              anchors.centerIn: parent
              text: preview.pageNumber + " / " + (preview.item ? preview.item.pages : 0)
              textFormat: Text.PlainText
              color: preview.theme.subtext
              font.family: preview.theme.fontFamily
              font.pixelSize: preview.theme.textCaption
            }
          }
        }
      }

      Component {
        id: textView
        Shared.SeeleFlickable {
          theme: preview.theme
          anchors.fill: parent
          anchors.margins: preview.theme.cardPadding
          anchors.rightMargin: preview.theme.scrollInset
          clip: true
          contentWidth: width
          contentHeight: bodyText.implicitHeight
          ScrollBar.vertical: Shared.SlimScrollBar { theme: preview.theme; popupHovered: bodyHover.hovered }

          Text {
            id: bodyText
            width: parent.width
            text: preview.item ? String(preview.item.text || "") : ""
            // Markdown is rendered; everything else is shown exactly as the
            // bytes read, so a source file is never re-interpreted as markup.
            textFormat: preview.item && preview.item.kind === "markdown"
              ? Text.MarkdownText : Text.PlainText
            wrapMode: Text.Wrap
            color: preview.theme.text
            font.family: preview.theme.fontFamily
            font.pixelSize: preview.theme.textBody
          }
        }
      }

      Component {
        id: folderView
        Shared.SeeleFlickable {
          theme: preview.theme
          anchors.fill: parent
          anchors.margins: preview.theme.cardPadding
          anchors.rightMargin: preview.theme.scrollInset
          clip: true
          contentWidth: width
          contentHeight: entries.implicitHeight
          ScrollBar.vertical: Shared.SlimScrollBar { theme: preview.theme; popupHovered: bodyHover.hovered }

          Column {
            id: entries
            width: parent.width
            spacing: preview.theme.spaceTight
            Repeater {
              model: preview.item ? (preview.item.entries || []) : []
              Row {
                required property var modelData
                spacing: preview.theme.spaceSmall
                Text {
                  text: modelData.directory ? "󰉋" : "󰈔"
                  color: preview.theme.overlay
                  font.family: preview.theme.fontFamily
                  font.pixelSize: preview.theme.textBody
                }
                Text {
                  text: modelData.name
                  textFormat: Text.PlainText
                  color: preview.theme.text
                  font.family: preview.theme.fontFamily
                  font.pixelSize: preview.theme.textBody
                }
              }
            }
          }
        }
      }

      // Loaded from its own file for the same reason the camera preview is:
      // a QtMultimedia backend that will not start must cost this one body,
      // not the shell.
      Component {
        id: mediaView
        Item {
          Loader {
            id: mediaLoader
            anchors.fill: parent
            asynchronous: true
            source: ""
            Component.onCompleted: setSource("QuickLookMedia.qml", { "theme": preview.theme })
          }
          Binding {
            target: mediaLoader.item
            property: "path"
            value: preview.item ? String(preview.item.path || "") : ""
            when: mediaLoader.status === Loader.Ready
          }
          Binding {
            target: mediaLoader.item
            property: "video"
            value: !!preview.item && preview.item.kind === "video"
            when: mediaLoader.status === Loader.Ready
          }
          Binding {
            target: mediaLoader.item
            property: "playing"
            value: preview.mediaPlaying
            when: mediaLoader.status === Loader.Ready
          }
          Connections {
            target: mediaLoader.item
            function onToggled() { preview.mediaPlaying = !preview.mediaPlaying }
            function onFinished() { preview.mediaPlaying = false }
          }
          Shared.EmptyState {
            anchors.fill: parent
            theme: preview.theme
            visible: mediaLoader.status === Loader.Error
            glyph: "󰀦"
            title: "Playback is unavailable"
            detail: preview.summary.detail
          }
        }
      }
    }
  }
}
