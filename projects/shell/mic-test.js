.import "../shared/Native.js" as Bridge

function devices(list, chosen, status) { return Bridge.call("mic_test.devices", [list, chosen, status]) }
function view(status, meter) { return Bridge.call("mic_test.view", [status, meter]) }
function users(names, detection) { return Bridge.call("mic_test.users", [names, detection]) }
function start(action, input, output, names, detection, acknowledged) {
  return Bridge.call("mic_test.start", [action, input, output, names, detection, acknowledged])
}
