import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "../shared" as Shared
import "ai-prompt.js" as Ai

Scope {
  id: prompt

  required property var theme
  property string usage: "Codex"
  property bool active: false
  property bool alive: false
  property bool busy: false
  property bool inserting: false
  property int generation: 0
  property int request: 0
  property int contextSerial: 0
  property int clipToken: 0
  property int selectionToken: 0
  property int screenToken: 0
  property int directoryToken: 0
  property string screenName: ""
  property var sourceWindow: ({})
  property string promptText: ""
  property string sentPrompt: ""
  property string answer: ""
  property string error: ""
  property string notice: ""
  property bool clipAllowed: false
  property bool clipReady: false
  property bool clipLoading: false
  property string clipPreview: ""
  property string clipText: ""
  property bool clipExpanded: false
  property int clipCharacters: 0
  property bool clipTruncated: false
  property bool selectionAllowed: false
  property bool selectionReady: false
  property bool selectionLoading: false
  property string selectionPreview: ""
  property string selectionText: ""
  property bool selectionExpanded: false
  property int selectionCharacters: 0
  property bool selectionTruncated: false
  property bool screenReady: false
  property bool screenLoading: false
  property string screenPreview: ""
  property bool directoryRequested: false
  property bool directoryReady: false
  property bool directoryLoading: false
  property string directory: ""
  readonly property var mentionedContexts: Ai.mentions(promptText)
  readonly property bool needsAction: answer !== ""
  readonly property bool canInsert: needsAction
    && /^0x[0-9a-f]+$/i.test(String(sourceWindow.address || ""))
    && answer.length <= 65536
  readonly property bool canSend: Ai.canSubmit(
    promptText,
    mentionedContexts,
    clipReady,
    selectionReady,
    directoryReady,
    screenReady,
    busy
  )

  signal usageRefreshRequested()

  function send(message) {
    if (worker.running) worker.write(JSON.stringify(message) + "\n")
  }

  function nextContextToken() {
    contextSerial++
    return contextSerial
  }

  function resetContexts() {
    screenCaptureDelay.stop()
    clipAllowed = false
    clipReady = false
    clipLoading = false
    clipPreview = ""
    clipText = ""
    clipExpanded = false
    clipCharacters = 0
    clipTruncated = false
    clipToken = 0
    selectionAllowed = false
    selectionReady = false
    selectionLoading = false
    selectionPreview = ""
    selectionText = ""
    selectionExpanded = false
    selectionCharacters = 0
    selectionTruncated = false
    selectionToken = 0
    screenReady = false
    screenLoading = false
    screenPreview = ""
    screenToken = 0
    directoryRequested = false
    directoryReady = false
    directoryLoading = false
    directory = ""
    directoryToken = 0
  }

  function open(screen, window) {
    if (alive) close()
    generation++
    request = 0
    screenName = String(screen || "")
    sourceWindow = window || ({})
    promptText = ""
    sentPrompt = ""
    answer = ""
    error = ""
    notice = ""
    busy = false
    inserting = false
    resetContexts()
    alive = true
    active = true
    send({ command: "open", id: generation, screen: screenName, window: sourceWindow })
    usageRefreshRequested()
  }

  function close() {
    screenCaptureDelay.stop()
    if (alive) send({ command: "close", id: generation })
    active = false
    alive = false
    busy = false
    inserting = false
    promptText = ""
    sentPrompt = ""
    answer = ""
    error = ""
    notice = ""
    resetContexts()
  }

  function syncContexts() {
    var values = mentionedContexts
    if (Ai.has(values, "dir")) {
      if (!directoryRequested) {
        directoryToken = nextContextToken()
        directoryRequested = true
        directoryLoading = true
        send({ command: "preview", id: generation, kind: "dir", token: directoryToken })
      }
    } else {
      if (directoryRequested && alive) send({ command: "forget", id: generation, kind: "dir" })
      directoryRequested = false
      directoryReady = false
      directoryLoading = false
      directory = ""
      directoryToken = 0
    }
    if (!Ai.has(values, "clip")) {
      if ((clipReady || clipLoading) && alive) send({ command: "forget", id: generation, kind: "clip" })
      clipAllowed = false
      clipReady = false
      clipLoading = false
      clipPreview = ""
      clipText = ""
      clipExpanded = false
      clipCharacters = 0
      clipTruncated = false
      clipToken = 0
    }
    if (!Ai.has(values, "select")) {
      if ((selectionReady || selectionLoading) && alive) send({ command: "forget", id: generation, kind: "select" })
      selectionAllowed = false
      selectionReady = false
      selectionLoading = false
      selectionPreview = ""
      selectionText = ""
      selectionExpanded = false
      selectionCharacters = 0
      selectionTruncated = false
      selectionToken = 0
    }
    if (!Ai.has(values, "screen")) {
      screenCaptureDelay.stop()
      if ((screenReady || screenLoading) && alive) send({ command: "forget", id: generation, kind: "screen" })
      screenReady = false
      screenLoading = false
      screenPreview = ""
      screenToken = 0
    }
  }

  onPromptTextChanged: syncContexts()

  function grant(kind) {
    error = ""
    notice = ""
    if (kind === "clip") {
      clipToken = nextContextToken()
      clipAllowed = true
      clipReady = false
      clipLoading = true
      clipPreview = ""
      clipText = ""
      clipExpanded = false
    } else if (kind === "select") {
      selectionToken = nextContextToken()
      selectionAllowed = true
      selectionReady = false
      selectionLoading = true
      selectionPreview = ""
      selectionText = ""
      selectionExpanded = false
    } else return
    var token = kind === "clip" ? clipToken : selectionToken
    send({ command: "preview", id: generation, kind: kind, token: token })
  }

  function captureScreen() {
    if (screenLoading) return
    error = ""
    notice = ""
    if (screenReady) send({ command: "forget", id: generation, kind: "screen" })
    screenReady = false
    screenLoading = true
    screenToken = nextContextToken()
    screenPreview = ""
    active = false
    screenCaptureDelay.restart()
  }

  function contextAction(kind) {
    if (kind === "clip" && clipReady) clipExpanded = !clipExpanded
    else if (kind === "select" && selectionReady) selectionExpanded = !selectionExpanded
    else if (kind === "screen") captureScreen()
    else grant(kind)
  }

  function submit() {
    if (!canSend) return
    request++
    sentPrompt = promptText
    answer = ""
    error = ""
    notice = ""
    busy = true
    send({
      command: "submit",
      id: generation,
      request: request,
      prompt: sentPrompt,
      permissions: Ai.permissions(mentionedContexts, clipAllowed, selectionAllowed)
    })
    promptText = ""
    resetContexts()
  }

  function copyAnswer() {
    if (!needsAction) return
    notice = "Copying…"
    send({ command: "copy", id: generation })
  }

  function insertAnswer() {
    if (!canInsert) return
    notice = "Restoring the original window…"
    inserting = true
    active = false
    send({ command: "insert", id: generation })
  }

  function key(event) {
    if (event.key === Qt.Key_Escape) {
      close()
      event.accepted = true
      return
    }
    if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return
    if (event.modifiers & Qt.ShiftModifier) return
    event.accepted = true
    if ((event.modifiers & Qt.ControlModifier) && needsAction) insertAnswer()
    else if (promptText.trim() !== "") submit()
    else if (needsAction) copyAnswer()
  }

  function acceptsContext(kind, token) {
    if (kind === "clip") return token === clipToken && clipAllowed && Ai.has(mentionedContexts, "clip")
    if (kind === "select") return token === selectionToken && selectionAllowed && Ai.has(mentionedContexts, "select")
    if (kind === "screen") return token === screenToken && screenLoading && Ai.has(mentionedContexts, "screen")
    if (kind === "dir") return token === directoryToken && directoryRequested && Ai.has(mentionedContexts, "dir")
    return false
  }

  function accept(message) {
    if (!alive || message.id !== generation) return
    if ((message.event === "preview" || message.event === "context-error")
        && !acceptsContext(String(message.kind || ""), Number(message.token || 0))) return
    if (message.event === "opened") {
      screenName = String(message.screen || screenName)
      var openedWindow = message.window || {}
      sourceWindow = {
        address: String(sourceWindow.address || ""),
        title: String(openedWindow.title || sourceWindow.title || ""),
        app: String(openedWindow.app || sourceWindow.app || "")
      }
    } else if (message.event === "preview") {
      if (message.kind === "clip") {
        clipLoading = false
        clipReady = !!message.available
        clipPreview = String(message.preview || "")
        clipText = String(message.text || "")
        clipCharacters = Number(message.characters || 0)
        clipTruncated = !!message.truncated
      } else if (message.kind === "select") {
        selectionLoading = false
        selectionReady = !!message.available
        selectionPreview = String(message.preview || "")
        selectionText = String(message.text || "")
        selectionCharacters = Number(message.characters || 0)
        selectionTruncated = !!message.truncated
      } else if (message.kind === "screen") {
        screenLoading = false
        screenReady = !!message.available
        screenPreview = String(message.path || "")
        active = true
      } else if (message.kind === "dir") {
        directoryLoading = false
        directoryReady = !!message.available
        directory = String(message.preview || "")
      }
    } else if (message.event === "context-error") {
      if (message.kind === "clip") {
        clipAllowed = false
        clipReady = false
        clipLoading = false
        clipPreview = ""
        clipText = ""
        clipExpanded = false
      } else if (message.kind === "select") {
        selectionAllowed = false
        selectionReady = false
        selectionLoading = false
        selectionPreview = ""
        selectionText = ""
        selectionExpanded = false
      } else if (message.kind === "screen") {
        screenReady = false
        screenLoading = false
        screenPreview = ""
        active = true
      } else if (message.kind === "dir") {
        directoryReady = false
        directoryLoading = false
      }
      error = String(message.message || "Context unavailable")
    } else if (message.event === "started") {
      busy = true
    } else if (message.event === "answer" && message.request === request) {
      busy = false
      sentPrompt = ""
      answer = String(message.text || "")
      error = ""
      notice = message.resumable ? "Session stays private to this panel" : "Answer ready"
    } else if (message.event === "permission") {
      busy = false
      promptText = sentPrompt
      error = "Allow " + (message.kind === "clip" ? "clipboard" : "selection") + " text before sending"
    } else if (message.event === "error") {
      busy = false
      if (message.request === request && sentPrompt !== "") promptText = sentPrompt
      error = String(message.message || "Codex is unavailable")
    } else if (message.event === "copied") {
      notice = "Copied to clipboard"
    } else if (message.event === "inserted") {
      close()
    } else if (message.event === "action-error") {
      inserting = false
      active = true
      notice = ""
      error = String(message.message || "Answer action failed")
    }
  }

  function contextRows() {
    var rows = []
    var values = mentionedContexts
    if (Ai.has(values, "clip")) rows.push({
      kind: "clip", glyph: "󰅌", title: "@clip · clipboard text",
      detail: clipLoading ? "Reading once…" : clipReady ? Ai.preview(clipPreview, clipCharacters, clipTruncated) : "Permission required before Codex can read it",
      warning: !clipReady, loading: clipLoading,
      action: clipReady ? (clipExpanded ? "Collapse" : "Review") : "Allow once",
      expanded: clipExpanded, body: clipText, image: ""
    })
    if (Ai.has(values, "select")) rows.push({
      kind: "select", glyph: "󰒅", title: "@select · primary selection",
      detail: selectionLoading ? "Reading once…" : selectionReady ? Ai.preview(selectionPreview, selectionCharacters, selectionTruncated) : "Permission required before Codex can read it",
      warning: !selectionReady, loading: selectionLoading,
      action: selectionReady ? (selectionExpanded ? "Collapse" : "Review") : "Allow once",
      expanded: selectionExpanded, body: selectionText, image: ""
    })
    if (Ai.has(values, "window")) {
      var app = String(sourceWindow.app || "Unknown application")
      var title = String(sourceWindow.title || "Untitled window")
      rows.push({ kind: "window", glyph: "󰖲", title: "@window · focused window", detail: app + " · " + title, warning: false, loading: false, action: "", expanded: false, body: "", image: "" })
    }
    if (Ai.has(values, "dir")) rows.push({
      kind: "dir", glyph: "󰉋", title: "@dir · terminal directory",
      detail: directoryLoading ? "Reading the focused terminal…" : directoryReady ? directory : "Unavailable · focus a terminal before opening the panel",
      warning: !directoryReady, loading: directoryLoading, action: "", expanded: false, body: "", image: ""
    })
    if (Ai.has(values, "screen")) rows.push({
      kind: "screen", glyph: "󰹑", title: "@screen · current output",
      detail: screenLoading ? "Capturing without the panel…" : screenReady ? (screenName || "Current output") + " · this exact image will be sent" : (screenName || "Unknown output") + " · capture a private preview before sending",
      warning: !screenReady, loading: screenLoading,
      action: screenReady ? "Recapture" : "Capture", expanded: false, body: "", image: screenPreview
    })
    return rows
  }

  Timer {
    id: screenCaptureDelay
    interval: 90
    onTriggered: prompt.send({ command: "preview", id: prompt.generation, kind: "screen", token: prompt.screenToken })
  }

  Process {
    id: worker
    command: ["seele-ai-prompt-worker"]
    running: true
    stdinEnabled: true
    stdout: SplitParser {
      onRead: data => {
        try { prompt.accept(JSON.parse(data)) }
        catch (_) {
          if (prompt.alive) {
            prompt.busy = false
            prompt.error = "The AI prompt helper returned invalid data"
          }
        }
      }
    }
    onRunningChanged: {
      if (!running && prompt.alive) {
        prompt.inserting = false
        prompt.active = true
        prompt.busy = false
        prompt.error = "The AI prompt helper stopped"
      }
    }
  }

  Variants {
    model: Quickshell.screens

    PanelWindow {
      id: promptWindow
      required property var modelData
      readonly property bool panelActive: prompt.active && prompt.screenName === modelData.name
      property bool sawFocus: false

      screen: modelData
      visible: true
      implicitWidth: Math.min(640, modelData.width - prompt.theme.panelMargin * 2)
      implicitHeight: Math.min(570, modelData.height - prompt.theme.panelMargin * 2)
      exclusionMode: ExclusionMode.Ignore
      color: "transparent"
      mask: Region {
        width: promptWindow.panelActive ? promptWindow.width : 0
        height: promptWindow.panelActive ? promptWindow.height : 0
      }
      WlrLayershell.layer: WlrLayer.Overlay
      WlrLayershell.keyboardFocus: panelActive ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
      WlrLayershell.namespace: "seele-shell-prompt"

      onActiveChanged: {
        if (active) sawFocus = true
        else if (sawFocus && panelActive && !prompt.needsAction) prompt.close()
      }

      Connections {
        target: prompt
        function onActiveChanged() {
          if (!prompt.active) {
            promptWindow.sawFocus = false
            return
          }
          if (promptWindow.panelActive) Qt.callLater(function() { promptField.forceActiveFocus() })
        }
      }

      Shared.PanelSurface {
        id: surface
        theme: prompt.theme
        visible: promptWindow.panelActive

        Shared.SeeleFlickable {
          id: panelScroll
          theme: prompt.theme
          anchors.fill: parent
          anchors.margins: prompt.theme.panelMargin
          anchors.rightMargin: prompt.theme.scrollInset
          clip: true
          contentWidth: width
          contentHeight: content.implicitHeight
          ScrollBar.vertical: Shared.SlimScrollBar { theme: prompt.theme; popupHovered: surface.hovered }

          Column {
            id: content
            width: panelScroll.width - prompt.theme.panelMargin + prompt.theme.scrollInset
            spacing: prompt.theme.panelSpacing
            focus: true
            Keys.onPressed: event => prompt.key(event)

            Shared.PanelHeader {
              theme: prompt.theme
              width: parent.width
              glyph: "󱚣"
              title: "Quick AI"
              detail: prompt.busy ? "Codex is thinking…" : prompt.usage
              detailColor: prompt.error !== "" ? prompt.theme.yellow : prompt.theme.subtext
              Shared.ActionButton {
                theme: prompt.theme
                text: "Close"
                onClicked: prompt.close()
              }
            }

            Rectangle {
              width: parent.width
              height: 112
              radius: prompt.theme.radius
              color: prompt.theme.wellColor
              border.width: 1
              border.color: promptField.activeFocus ? prompt.theme.edgeCrown : prompt.theme.cardBorder
              antialiasing: true

              TextArea {
                id: promptField
                anchors.fill: parent
                anchors.margins: prompt.theme.cardPadding
                text: prompt.promptText
                onTextChanged: if (prompt.promptText !== text) prompt.promptText = text
                enabled: !prompt.busy
                placeholderText: prompt.needsAction ? "Ask a follow-up…" : "Ask Codex…  Add @clip, @select, @window, @dir, or @screen"
                color: prompt.theme.text
                placeholderTextColor: prompt.theme.overlay
                selectionColor: prompt.theme.selectedColor
                selectedTextColor: prompt.theme.text
                font.family: prompt.theme.fontFamily
                font.pixelSize: prompt.theme.textLead
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                persistentSelection: true
                background: Item {}
                Keys.onPressed: event => prompt.key(event)
              }
            }

            RowLayout {
              width: parent.width
              spacing: prompt.theme.spaceMedium
              Text {
                Layout.fillWidth: true
                text: "Enter sends · Shift+Enter adds a line · sensitive text needs one-time permission"
                color: prompt.theme.subtext
                font.family: prompt.theme.fontFamily
                font.pixelSize: prompt.theme.textCaption
                wrapMode: Text.WordWrap
              }
              Shared.ActionButton {
                theme: prompt.theme
                text: prompt.busy ? "Thinking…" : "Send"
                enabled: prompt.canSend
                onClicked: prompt.submit()
              }
            }

            Column {
              width: parent.width
              visible: prompt.mentionedContexts.length > 0
              spacing: prompt.theme.spaceSmall

              Shared.SectionRule {
                theme: prompt.theme
                width: parent.width
                label: "CONTEXT"
                detail: prompt.mentionedContexts.length + " selected"
              }

              Repeater {
                model: prompt.contextRows()

                Rectangle {
                  id: contextRow
                  required property var modelData
                  width: parent.width
                  height: Math.max(prompt.theme.rowHeight, contextText.implicitHeight + prompt.theme.cardPadding * 2)
                  radius: prompt.theme.radiusSmall
                  color: prompt.theme.rowColor
                  border.width: 1
                  border.color: prompt.theme.cardBorder
                  antialiasing: true

                  Rectangle {
                    id: contextGlyph
                    anchors.left: parent.left
                    anchors.leftMargin: prompt.theme.cardPadding
                    anchors.top: parent.top
                    anchors.topMargin: prompt.theme.cardPadding
                    width: prompt.theme.chipHeight
                    height: width
                    radius: prompt.theme.radiusSmall
                    color: prompt.theme.activeTint
                    Shared.CenteredGlyph {
                      anchors.fill: parent
                      text: contextRow.modelData.glyph
                      color: prompt.theme.accent
                      font.family: prompt.theme.fontFamily
                      font.pixelSize: prompt.theme.textIcon
                    }
                  }

                  Column {
                    id: contextText
                    anchors.left: contextGlyph.right
                    anchors.leftMargin: prompt.theme.spaceMedium
                    anchors.right: contextAction.visible ? contextAction.left : parent.right
                    anchors.rightMargin: contextAction.visible ? prompt.theme.spaceMedium : prompt.theme.cardPadding
                    anchors.top: parent.top
                    anchors.topMargin: prompt.theme.cardPadding
                    spacing: prompt.theme.spaceTight
                    Text {
                      width: parent.width
                      text: contextRow.modelData.title
                      color: prompt.theme.text
                      font.family: prompt.theme.fontFamily
                      font.pixelSize: prompt.theme.textBody
                      font.weight: prompt.theme.weightMedium
                      elide: Text.ElideRight
                    }
                    Text {
                      width: parent.width
                      text: contextRow.modelData.detail
                      textFormat: Text.PlainText
                      color: contextRow.modelData.warning ? prompt.theme.yellow : prompt.theme.subtext
                      font.family: prompt.theme.fontFamily
                      font.pixelSize: prompt.theme.textCaption
                      wrapMode: Text.WordWrap
                      maximumLineCount: 2
                      elide: Text.ElideRight
                    }
                    Image {
                      width: parent.width
                      height: 112
                      visible: contextRow.modelData.image !== ""
                      source: visible ? "file://" + contextRow.modelData.image : ""
                      fillMode: Image.PreserveAspectFit
                      asynchronous: true
                      cache: false
                    }
                    Text {
                      width: parent.width
                      visible: contextRow.modelData.expanded
                      text: contextRow.modelData.body
                      textFormat: Text.PlainText
                      color: prompt.theme.text
                      font.family: prompt.theme.fontFamily
                      font.pixelSize: prompt.theme.textCaption
                      wrapMode: Text.WrapAnywhere
                    }
                  }

                  Shared.ActionButton {
                    id: contextAction
                    theme: prompt.theme
                    anchors.right: parent.right
                    anchors.rightMargin: prompt.theme.cardPadding
                    anchors.top: parent.top
                    anchors.topMargin: prompt.theme.cardPadding
                    visible: contextRow.modelData.action !== ""
                    enabled: !contextRow.modelData.loading
                    text: contextRow.modelData.loading ? (contextRow.modelData.kind === "screen" ? "Capturing…" : "Reading…") : contextRow.modelData.action
                    onClicked: prompt.contextAction(contextRow.modelData.kind)
                  }
                }
              }
            }

            Column {
              width: parent.width
              visible: prompt.busy || prompt.answer !== "" || prompt.error !== ""
              spacing: prompt.theme.spaceSmall

              Shared.SectionRule {
                theme: prompt.theme
                width: parent.width
                label: "ANSWER"
                detail: prompt.busy ? "Working" : prompt.notice !== "" ? prompt.notice : prompt.answer !== "" ? "Enter copies · Ctrl+Enter inserts" : ""
                detailColor: prompt.error !== "" ? prompt.theme.yellow : prompt.theme.overlay
              }

              Rectangle {
                width: parent.width
                height: prompt.answer !== "" ? 190 : 86
                radius: prompt.theme.radius
                color: prompt.theme.cardColor
                border.width: 1
                border.color: prompt.error !== "" ? prompt.theme.alpha(prompt.theme.yellow, 0.3) : prompt.theme.cardBorder
                antialiasing: true

                Row {
                  anchors.centerIn: parent
                  spacing: prompt.theme.spaceMedium
                  visible: prompt.busy
                  Shared.RefreshGlyph {
                    theme: prompt.theme
                    anchors.verticalCenter: parent.verticalCenter
                    width: prompt.theme.textDisplay
                    height: width
                    spinning: true
                  }
                  Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Thinking with Codex"
                    color: prompt.theme.subtext
                    font.family: prompt.theme.fontFamily
                    font.pixelSize: prompt.theme.textBody
                  }
                }

                Text {
                  anchors.fill: parent
                  anchors.margins: prompt.theme.cardPadding
                  visible: !prompt.busy && prompt.answer === "" && prompt.error !== ""
                  text: prompt.error
                  textFormat: Text.PlainText
                  color: prompt.theme.yellow
                  font.family: prompt.theme.fontFamily
                  font.pixelSize: prompt.theme.textBody
                  wrapMode: Text.WordWrap
                  verticalAlignment: Text.AlignVCenter
                }

                Shared.SeeleFlickable {
                  theme: prompt.theme
                  anchors.fill: parent
                  anchors.margins: prompt.theme.cardPadding
                  visible: !prompt.busy && prompt.answer !== ""
                  clip: true
                  contentWidth: width
                  contentHeight: answerText.height
                  TextArea.flickable: TextArea {
                    id: answerText
                    text: prompt.answer
                    readOnly: true
                    textFormat: TextEdit.PlainText
                    color: prompt.theme.text
                    selectionColor: prompt.theme.selectedColor
                    selectedTextColor: prompt.theme.text
                    font.family: prompt.theme.fontFamily
                    font.pixelSize: prompt.theme.textBody
                    wrapMode: TextEdit.Wrap
                    selectByMouse: true
                    persistentSelection: true
                    background: Item {}
                    Keys.onPressed: event => prompt.key(event)
                  }
                  ScrollBar.vertical: Shared.SlimScrollBar { theme: prompt.theme; popupHovered: surface.hovered }
                }
              }

              Row {
                anchors.right: parent.right
                spacing: prompt.theme.spaceMedium
                visible: prompt.answer !== ""
                Shared.ActionButton {
                  theme: prompt.theme
                  text: "Copy · Enter"
                  onClicked: prompt.copyAnswer()
                }
                Shared.ActionButton {
                  theme: prompt.theme
                  text: "Insert · Ctrl+Enter"
                  enabled: prompt.canInsert
                  onClicked: prompt.insertAnswer()
                }
              }
            }
          }
        }
      }
    }
  }
}
