import QtQuick
import Quickshell
import Quickshell.Io
import "notes.js" as Notes

Scope {
  id: store
  property var notes: []
  property string selected: ""
  property string error: ""
  property string warning: ""
  property bool ready: false
  property int serial: 0
  property var pending: ({})
  property var pendingTrash: null
  property string recordingNote: ""
  property bool stopping: false
  property bool memoSaved: false
  property real recordingDuration: 0
  property string recordingError: ""
  readonly property bool recording: recordingNote !== ""
  readonly property var current: find(selected)
  readonly property bool dirty: notes.some(function(note) { return !!note._dirty })
  signal level(real value)
  signal created()

  function find(id) {
    for (var i = 0; i < notes.length; i++) if (notes[i].id === id) return notes[i]
    return null
  }
  function send(message) {
    if (!ready) return false
    message.request = ++serial
    var next = Object.assign({}, pending)
    next[message.request] = message
    pending = next
    worker.write(JSON.stringify(message) + "\n")
    return true
  }
  function apply(note, version) {
    var previous = find(note.id)
    var next = notes.filter(function(item) { return item.id !== note.id })
    next.push(Notes.merge(note, previous, version))
    next.sort(function(a, b) { return b.updated - a.updated || b.id.localeCompare(a.id) })
    notes = next
  }
  function receive(line) {
    try {
      var data = JSON.parse(line)
      if (data.ready) ready = true
      var request = pending[data.request] || {}
      var nextPending = Object.assign({}, pending)
      delete nextPending[data.request]
      pending = nextPending
      if (data.ok === false) { pendingTrash = null; error = data.error || "Could not save changes"; return }
      if (data.notes) {
        var next = data.notes.map(function(note) { return Notes.merge(note, find(note.id), -1) })
        // A failed save is retained even if the disk listing is incomplete.
        notes.forEach(function(note) {
          if (note._dirty && !next.some(function(item) { return item.id === note.id })) next.push(note)
        })
        notes = next
        warning = data.unreadable ? data.unreadable + " unreadable note(s) were left untouched." : ""
        if (!selected) { var first = notes.find(function(note) { return !note.trashed }); if (first) selected = first.id }
      }
      if (data.note) {
        apply(data.note, request.version)
        if (request.action === "create") { selected = data.note.id; created() }
      }
      if (data.ready || request.action === "save") flush()
      if (pendingTrash) {
        var deleting = find(pendingTrash.id)
        if (deleting && !deleting._dirty) { send(pendingTrash); pendingTrash = null }
      }
    } catch (_) { error = "Could not read the Notes service response" }
  }
  function edit(field, value) {
    var note = current
    if (!note || note.trashed || note[field] === value) return
    var updated = Object.assign({}, note)
    updated[field] = value
    updated._version = (note._version || 0) + 1
    updated._dirty = true
    notes = notes.map(function(item) { return item.id === updated.id ? updated : item })
    autosave.restart()
  }
  function flush() {
    autosave.stop()
    notes.forEach(function(note) {
      if (!note._dirty) return
      for (var key in pending) {
        var request = pending[key]
        if (request.action === "save" && request.id === note.id && request.version === note._version) return
      }
      send({action:"save",id:note.id,title:note.title,body:note.body,version:note._version})
    })
  }
  function retry() {
    error = ""
    if (!worker.running) worker.running = true
    else { flush(); send({action:"list"}) }
  }
  function create() { flush(); send({action:"create"}) }
  function select(id) { flush(); selected = id }
  function trash(restore) {
    if (!current || (recording && selected === recordingNote)) return
    // A failed save must never be followed by trashing an older disk copy.
    if (current._dirty) {
      pendingTrash = {action:restore ? "restore" : "trash",id:selected}
      flush()
    } else send({action:restore ? "restore" : "trash",id:selected})
  }
  function startRecording() {
    if (!current || current.trashed || recording || !ready) return
    flush()
    recordingError = ""
    recordingDuration = 0
    memoSaved = false
    stopping = false
    recordingNote = selected
    recorder.command = ["seele-notes-store", "record", selected]
    recorder.running = true
  }
  function stopRecording() {
    if (!recording || stopping) return
    stopping = true
    recorder.write("stop\n")
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
      if (!store.error) store.error = "Notes service disconnected. Your unsaved edits are still here."
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
          if (data.duration !== undefined) store.recordingDuration = data.duration
          if (data.saved) store.memoSaved = true
        } catch (_) { store.recordingError = "Could not read recording progress" }
      }
    }
    stderr: StdioCollector { onStreamFinished: if (text.trim()) store.recordingError = text.trim() }
    onExited: code => {
      if (code !== 0 || !store.memoSaved) store.error = store.recordingError || "Voice memo could not be saved"
      store.recordingNote = ""
      store.stopping = false
      store.level(0)
      store.send({action:"list"})
    }
  }
}
