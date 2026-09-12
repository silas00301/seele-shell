.import "../shared/Native.js" as Bridge

function actions(state) { return Bridge.call("ai_activity.actions", [state]) }
function rows(values, now) { return Bridge.call("ai_activity.rows", [values, now]) }
function indicator(jobs) { return Bridge.call("ai_activity.indicator", [jobs]) }
