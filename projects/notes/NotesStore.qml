import QtQuick
import Quickshell
import Quickshell.Io
import "notes.js" as Notes

// The vault directory is the library. This store owns one open document at a
// time, the digest it was loaded from, and the states the file can be in that
// the editor cannot see for itself.
Scope {
  id: store

  property var config: ({ configured: false, legacy: 0 })
  property var ui: ({ sidebar: 0, collapsed: false })
  property var notes: []
  property var trash: []
  property var drafts: []
  property var vaults: []
  property var browse: null
  property var migration: null
  property int unreadable: 0
  property bool ready: false
  property bool started: false
  property int serial: 0
  property var pending: ({})

  // The open document. `path` is empty for a capture that has not earned a
  // filename yet, and `draft` is what the editor currently holds.
  property string path: ""
  // The path the editor is actually showing, which lags `path` between asking
  // for a note and being handed it.
  property string openPath: ""
  property string draft: ""
  property string baseline: ""
  property var audio: []
  property bool trashView: false
  property string trashId: ""

  property string saveState: "idle"
  property var conflict: null
  property bool pendingTrash: false
  property string error: ""
  property string warning: ""

  // Recording is not owned by a note: audio is a vault file, and the note
  // merely embeds it.
  property bool recording: false
  property bool stopping: false
  property real recordingDuration: 0
  property string recordingError: ""

  readonly property bool dirty: saveState === "dirty" || saveState === "saving"
  readonly property bool writable: !!config.writable
  readonly property var note: find(path)

  signal level(real value)
  // The one channel that may replace the editor's text. Autosave, listing
  // refreshes and search updates never travel this way, so the caret, the
  // selection and the undo stack survive all of them.
  signal loaded(string text, bool keepCaret)
  signal recorded(string name)

  function find(value) {
    for (var i = 0; i < notes.length; i++) if (notes[i].path === value) return notes[i]
    return null
  }

  function send(message) {
    if (!ready) return 0
    message.request = ++serial
    var next = Object.assign({}, pending)
    next[message.request] = message
    pending = next
    worker.write(JSON.stringify(message) + "\n")
    return message.request
  }

  function configure(vault, directory) {
    error = ""
    send({ action: "configure", vault: vault, directory: directory })
  }
  function listDirectory(at) { send({ action: "browse", path: at || "" }) }
  function findVaults() { send({ action: "vaults" }) }
  function previewMigration() { send({ action: "migrate", mode: "preview" }) }
  function runMigration() { send({ action: "migrate", mode: "run" }) }
  function remember(width, collapsed) { send({ action: "ui", sidebar: width, collapsed: collapsed }) }

  // A new capture is a document, not a file. Nothing is written until there is
  // something to write, so an abandoned empty draft leaves no file behind.
  function create() {
    flush()
    path = ""
    openPath = ""
    trashId = ""
    baseline = ""
    draft = ""
    audio = []
    conflict = null
    saveState = "idle"
    trashView = false
    loaded("", false)
  }

  function select(value) {
    if (value === path) return
    flush()
    conflict = null
    trashId = ""
    path = value
    send({ action: "read", path: value })
  }

  function edit(text) {
    if (draft === text) return
    draft = text
    if (saveState !== "conflict" && saveState !== "gone") saveState = "dirty"
    autosave.restart()
  }

  function flush() {
    autosave.stop()
    if (saveState !== "dirty" && saveState !== "failed") return
    if (!ready || !config.configured) return
    if (path === "" && draft.trim() === "") { saveState = "idle"; return }
    saveState = "saving"
    error = ""
    send({ action: "save", path: path, text: draft, baseline: baseline })
  }

  function resolve(mode) {
    if (!conflict) return
    send({ action: "resolve", path: conflict.path, mode: mode, text: draft })
  }

  function retry() {
    error = ""
    if (!worker.running) { worker.running = true; return }
    if (saveState === "failed" || saveState === "dirty") flush()
    else send({ action: "list" })
  }

  function trashNote() {
    if (!path || recording) return
    // A failed save must never be followed by discarding an older disk copy,
    // so the move waits for the write to be acknowledged first.
    if (saveState === "dirty" || saveState === "saving" || saveState === "failed") {
      pendingTrash = true
      flush()
      return
    }
    send({ action: "trash", path: path })
  }

  function restoreNote(id) { if (id) send({ action: "restore", id: id }) }

  // The trash is a second view of the same folder, not a second document. It
  // reads what it selects and hands the editor back to the open note on the
  // way out, so switching views never disturbs unsaved text.
  function setTrashView(on) {
    if (trashView === on) return
    trashView = on
    if (on) selectTrash(trash.length ? trash[0].id : "")
    else { trashId = ""; loaded(draft, false) }
  }

  function selectTrash(id) {
    trashId = id
    if (id) send({ action: "read", id: id })
    else loaded("", false)
  }
  // A recovery draft is for text the vault could not take. A note that saved
  // cleanly has nothing to recover, and keeping one anyway would greet the
  // next launch with a warning about work that is already on disk.
  function keepDraft() {
    if (!path || draft === "") return
    if (saveState !== "failed" && saveState !== "conflict" && saveState !== "gone") return
    send({ action: "draft", path: path, text: draft })
  }
  function dropDraft(value) { send({ action: "discard", path: value }) }

  function startRecording() {
    if (recording || !config.configured) return
    recordingError = ""
    recordingDuration = 0
    stopping = false
    recording = true
    recorder.command = ["seele-notes-store", "record"]
    recorder.running = true
  }
  function stopRecording() {
    if (!recording || stopping) return
    stopping = true
    recorder.write("stop\n")
  }

  function receive(line) {
    var data
    try { data = JSON.parse(line) } catch (_) { error = "Could not read the Notes service response"; return }
    if (data.ready) ready = true
    var request = pending[data.request] || {}
    if (data.request !== undefined) {
      var nextPending = Object.assign({}, pending)
      delete nextPending[data.request]
      pending = nextPending
    }
    // Choosing a folder answers whatever the unconfigured worker refused.
    if (data.config) {
      if (data.config.configured && !config.configured) error = ""
      config = data.config
    }
    if (data.ui) ui = data.ui
    if (data.drafts) drafts = data.drafts
    if (data.vaults) vaults = data.vaults
    if (data.browse) browse = data.browse
    if (data.migration) migration = data.migration
    if (data.ok === false) { applyFailure(request, data.error); return }
    if (data.notes) applyListing(data)
    if (data.audio !== undefined && data.text === undefined) audio = data.audio
    if (data.preview) { loaded(data.text, false); return }
    if (data.conflict) {
      conflict = data.conflict
      saveState = "conflict"
      return
    }
    if (data.gone) { saveState = "gone"; return }
    if (data.text !== undefined && (request.action === "read" || data.reload)) {
      applyDocument(data)
      return
    }
    if (data.note && (request.action === "save" || request.action === "resolve")) applySaved(request, data)
    if (data.trashed) afterTrash()
    if (data.restored) {
      conflict = null
      trashView = false
      trashId = ""
      openPath = ""
      select(data.restored)
    }
    // A launch is a capture, so the window opens on an empty document with the
    // caret already in it. A reconnect is not a launch: the draft that was in
    // the editor when the worker went away stays exactly where it was.
    if (data.ready && !started) {
      started = true
      if (config.configured) create()
    }
  }

  function applyListing(data) {
    notes = data.notes
    trash = data.trash || []
    unreadable = data.unreadable || 0
    warning = unreadable ? unreadable + " file(s) in this directory could not be read." : ""
    if (data.writable !== undefined && config.writable !== data.writable)
      config = Object.assign({}, config, { writable: data.writable })
    if (!data.changed) return
    // An external edit refreshes a clean note in place and never takes the
    // keyboard away from whoever is typing in another one.
    if (!path) return
    var current = find(path)
    if (!current) {
      if (saveState === "dirty" || saveState === "saving" || saveState === "conflict") saveState = "gone"
      else { path = ""; draft = ""; baseline = ""; audio = []; loaded("", false) }
      return
    }
    if (current.hash !== baseline && saveState !== "dirty" && saveState !== "saving" && saveState !== "conflict")
      send({ action: "read", path: path })
  }

  function applyDocument(data) {
    // Re-reading the note already on screen keeps the caret where the reader
    // left it; arriving at a different note starts at the top.
    var keep = openPath === data.note.path
    path = data.note.path
    openPath = path
    baseline = data.hash
    draft = data.text
    audio = data.audio || []
    conflict = null
    saveState = "idle"
    loaded(data.text, keep)
  }

  function applySaved(request, data) {
    path = data.note.path
    openPath = path
    baseline = data.hash
    conflict = null
    // Text typed while the save was in flight is still unsaved: acknowledge
    // only the exact text the worker wrote.
    if (request.text !== undefined && request.text !== draft) { saveState = "dirty"; autosave.restart() }
    else saveState = "saved"
    if (data.switched) loaded(draft, true)
    if (data.note.audio !== audio.length) send({ action: "audio", path: path })
    if (pendingTrash) { pendingTrash = false; trashNote() }
  }

  function applyFailure(request, message) {
    error = message || "Could not save changes"
    pendingTrash = false
    if (request.action === "save" || request.action === "resolve") saveState = "failed"
  }

  function afterTrash() {
    conflict = null
    saveState = "idle"
    var next = notes.find(function(item) { return item.path !== path })
    path = ""
    openPath = ""
    baseline = ""
    audio = []
    if (next) select(next.path)
    else create()
  }

  Timer { id: autosave; interval: 350; onTriggered: store.flush() }

  Process {
    id: worker
    command: ["seele-notes-store", "watch"]
    stdinEnabled: true
    running: true
    stdout: SplitParser { onRead: line => store.receive(line) }
    stderr: StdioCollector { onStreamFinished: if (text.trim()) store.error = text.trim() }
    onExited: {
      store.ready = false
      store.pending = ({})
      if (store.saveState === "saving") store.saveState = "dirty"
      if (!store.error) store.error = "The Notes service stopped. Your unsaved text is still here."
    }
  }

  Process {
    id: recorder
    stdinEnabled: true
    stdout: SplitParser {
      onRead: line => {
        try {
          var data = JSON.parse(line)
          if (data.level !== undefined) store.level(data.level)
          if (data.duration !== undefined && !data.saved) store.recordingDuration = data.duration
          if (data.saved) store.recorded(data.saved)
          if (data.salvaged) {
            store.recorded(data.salvaged)
            store.recordingError = "The microphone stopped early. The audio captured so far was kept."
          }
        } catch (_) { store.recordingError = "Could not read recording progress" }
      }
    }
    stderr: StdioCollector { onStreamFinished: if (text.trim()) store.recordingError = text.trim() }
    onExited: code => {
      store.recording = false
      store.stopping = false
      store.level(0)
      if (code !== 0) store.error = store.recordingError || "The voice memo could not be saved"
      else if (store.recordingError) store.warning = store.recordingError
    }
  }
}
