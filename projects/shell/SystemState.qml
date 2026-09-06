import QtQml

QtObject {
  id: state

  // Each field owns its notify signal. A volume update must not invalidate
  // notification, device, or agent models elsewhere in the shell.
  property int volume: 0
  property bool muted: false
  property int microphoneVolume: 0
  property bool microphoneMuted: false
  property bool microphoneActive: false
  property string connection: "Disconnected"
  property string connectionType: ""
  property string connectivity: "unknown"
  property bool wifiEnabled: false
  property bool wifiAvailable: false
  property string ipAddress: ""
  property string gateway: ""
  property var tailscale: ({ available: false, backend: "Unavailable", connected: false, needsLogin: false, name: "", ip: "", tailnet: "", peers: 0, onlinePeers: 0 })
  property var protonVpn: ({ available: false, connected: false, connection: "" })
  property var sshServer: ({ available: false, mode: "off", tailscaleAvailable: false, sshAvailable: false })
  property bool bluetoothAvailable: false
  property bool bluetoothPowered: false
  property int bluetoothConnected: 0
  property bool bluetoothScanning: false
  property bool bluetoothReceiver: false
  property bool bluetoothDiscoverable: false
  property var bluetoothDevices: []
  property var headphones: ({ connected: false, name: "", kind: "", battery: null, controls: false, noiseMode: "" })
  property string voxtypeStatus: "unavailable"
  property var cameraDevices: []
  property string cameraDevice: ""
  property bool cameraActive: false
  property bool screenRecording: false
  property var audioDevices: []
  property var batteries: []
  property var trayHidden: []
  property var barModules: ({})
  property bool airpodsEarDetection: true
  property var agentStates: ({})
  property var notifications: ({ count: 0, items: [], history: [] })
  property bool dnd: false

  // Preserve unchanged JSON branches, including current notifications when
  // only history changes. Object key order is irrelevant; array order isn't.
  function retain(previous, next) {
    if (previous === next) return previous
    if (previous === null || next === null
        || typeof previous !== "object" || typeof next !== "object"
        || Array.isArray(previous) !== Array.isArray(next)) return next

    var keys = Object.keys(next)
    var unchanged = keys.length === Object.keys(previous).length
    var result = Array.isArray(next) ? [] : {}
    for (var i = 0; i < keys.length; i++) {
      var key = keys[i]
      var value = retain(previous[key], next[key])
      // JSON can carry this as an ordinary key, not a prototype assignment.
      if (key === "__proto__") Object.defineProperty(result, key, { value: value, enumerable: true, writable: true, configurable: true })
      else result[key] = value
      if (!Object.prototype.hasOwnProperty.call(previous, key) || value !== previous[key]) unchanged = false
    }
    return unchanged ? previous : result
  }

  function apply(patch) {
    for (var key in patch) {
      var previous = state[key]
      if (previous === undefined || typeof previous === "function") continue
      var next = retain(previous, patch[key])
      if (next !== previous) state[key] = next
    }
  }
}
