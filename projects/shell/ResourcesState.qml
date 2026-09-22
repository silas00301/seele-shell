import QtQuick
import "../shared/ListModels.js" as Models

// Qt owns model identity and focus; the worker owns filtering, sorting, samples
// and process identity. This state is independently usable by the Qt fixture.
QtObject {
  id: state
  property ListModel model: ListModel { dynamicRoles: true }
  property var snapshot: ({ rows: [], cpuHistory: [], memoryHistory: [], memory: null, cpu: null, total: 0, matched: 0, cores: 0, cadenceSeconds: 1, historyCapacity: 60 })
  property string query: ""
  property string sort: "cpu"
  property string selected: ""
  property string error: ""
  signal request(var value)
  function accept(value) {
    if (!value || value.version !== 1 || value.type !== "snapshot") return
    snapshot = value
    error = ""
    Models.reconcile(model, value.rows || [], "entry", function(row) { return row.id })
  }
  function reset() {
    model.clear()
    snapshot = ({ rows: [], cpuHistory: [], memoryHistory: [], memory: null, cpu: null, total: 0, matched: 0, cores: 0, cadenceSeconds: 1, historyCapacity: 60 })
    query = ""
    sort = "cpu"
    selected = ""
    error = ""
  }
  function select(id) {
    selected = selected === id ? "" : id
    request({op: "select", id: selected})
  }
  onQueryChanged: request({op: "query", text: query})
  onSortChanged: request({op: "sort", value: sort})
}
