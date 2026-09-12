.import "../shared/Native.js" as Bridge
function initial(host) { return Bridge.call("github.initial",Array.prototype.slice.call(arguments)) }
function receive(current,next) { return Bridge.call("github.receive",Array.prototype.slice.call(arguments)) }
function due(lastAttempt,now,state,manual) { return Bridge.call("github.due",Array.prototype.slice.call(arguments)) }
function safeUrl(url,host) { return Bridge.call("github.safeUrl",Array.prototype.slice.call(arguments)) }
function checksLabel(state) { return Bridge.call("github.checksLabel",Array.prototype.slice.call(arguments)) }
function reviewLabel(item) { return Bridge.call("github.reviewLabel",Array.prototype.slice.call(arguments)) }
