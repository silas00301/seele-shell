.import "../shared/Native.js" as Bridge

function addresses(entries, device) { return Bridge.call("network.addresses", [entries, device]) }
