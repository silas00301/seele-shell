.import "Native.js" as Bridge

// The Qt host retains real delegates and role values. Rust plans only ID moves;
// payloads cross neither the ABI nor a second state store for reconciliation.
function reconcile(model, values, role, key, keyRole) {
  var previous = [], next = []
  for (var i = 0; i < model.count; i++) previous.push(keyRole ? model.get(i)[keyRole] : key(model.get(i)[role]))
  for (var j = 0; j < values.length; j++) next.push(key(values[j]))
  var inserted = {}
  var operations = Bridge.call("models.plan", [previous, next])
  for (var n = 0; n < operations.length; n++) {
    var operation = operations[n]
    if (operation.insert !== undefined) {
      var row = {}, index = operation.insert
      row[role] = values[index]
      if (keyRole) row[keyRole] = next[index]
      model.insert(index, row)
      inserted[index] = true
    } else model.move(operation.move, operation.to, 1)
  }
  for (var k = 0; k < values.length; k++) {
    if (!inserted[k] && JSON.stringify(model.get(k)[role]) !== JSON.stringify(values[k])) model.setProperty(k, role, values[k])
  }
  if (model.count > values.length) model.remove(values.length, model.count - values.length)
}
