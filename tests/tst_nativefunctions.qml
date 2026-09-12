import QtQuick
import QtTest
import Seele.Core

TestCase {
  name: "NativeFunctions"
  function test_variant_and_unicode_round_trip() {
    compare(Functions.call("fixture.echo", ["🦀"]), "🦀")
    var value = Functions.call("fixture.echo", [{name: "text", number: 4, values: [true, null, "😀"]}])
    compare(value.name, "text")
    compare(value.number, 4)
    compare(value.values[0], true)
    compare(value.values[1], null)
    compare(value.values[2], "😀")
  }
  function test_native_results_have_json_object_and_array_semantics() {
    var input = JSON.parse('{"__proto__":{"inherited":true},"constructor":"data","nested":[{"values":[1,2,3]},[]]}')
    var value = Functions.call("fixture.echo", [input])
    verify(Object.prototype.hasOwnProperty.call(value, "__proto__"))
    verify(Object.prototype.hasOwnProperty.call(value, "constructor"))
    compare(value.constructor, "data")
    compare(value.inherited, undefined)
    verify(Object.getPrototypeOf(value) === Object.prototype)
    verify(Array.isArray(value.nested))
    verify(Array.isArray(value.nested[0].values))
    verify(Array.isArray(value.nested[1]))
    compare(value.nested[0].values.filter(function(x) { return x > 1 }).map(function(x) { return x * 2 }).join(","), "4,6")
    value.nested[1].push("owned")
    compare(value.nested[1].length, 1)
    compare(input.nested[1].length, 0)
    compare(Functions.call("fixture.echo", [null]), null)
    compare(Functions.call("fixture.echo", [false]), false)
    compare(Functions.call("fixture.echo", [0]), 0)
    compare(Functions.call("fixture.echo", [""]), "")
    compare(Functions.call("fixture.echo", []), null)
    // Optional/missing arguments retain their JSON protocol semantics; fields
    // absent from an ordinary object remain undefined in the QML engine.
    compare(value.missing, undefined)
    verify(Array.isArray(Functions.call("fixture.echo", [[]])))
    var advertised = {}
    Object.defineProperty(advertised, "__proto__", {value:"Open", enumerable:true, writable:true, configurable:true})
    Object.defineProperty(advertised, "constructor", {value:"Reply", enumerable:true, writable:true, configurable:true})
    var returned = Functions.call("fixture.echo", [{actions:advertised}]).actions
    verify(Object.prototype.hasOwnProperty.call(returned, "__proto__"))
    compare(returned.__proto__, "Open")
    compare(returned.constructor, "Reply")
    var parse = JSON.parse
    try {
      JSON.parse = function() { throw new Error("mutated global parser") }
      compare(Functions.call("fixture.echo", ["captured parser"]), "captured parser")
    } finally { JSON.parse = parse }
  }
  function test_errors_do_not_expose_arguments() {
    var rejected = false
    try { Functions.call("missing.operation", ["private fixture text"]) }
    catch (error) { rejected = true; verify(String(error).indexOf("private fixture text") < 0) }
    verify(rejected)
  }
  function test_bounds_reject_before_json_expansion() {
    function rejects(value, label) {
      var rejected = false
      try { Functions.call("fixture.echo", [value]) }
      catch (error) { rejected = true; verify(String(error).indexOf("limit") >= 0) }
      verify(rejected, label)
    }
    var nested = "leaf"
    for (var i = 0; i < 66; i++) nested = [nested]
    rejects(nested, "depth")
    rejects(new Array(3 * 1024 * 1024 + 1).join("x"), "bytes")
    rejects(this, "QObject")
    var cyclic = {}; cyclic.self = cyclic
    var normalized = Functions.call("fixture.echo", [cyclic])
    verify(JSON.stringify(normalized).length < 64, "Qt normalizes cycles before the QVariant boundary")
    compare(Functions.call("fixture.echo", [[1, "safe", null]])[1], "safe")
  }
  function test_resident_notification_state_isolation_and_restore() {
    var first=Functions.notificationState(1000), second=Functions.notificationState(1000)
    var entry={id:7,summary:"🦀",body:"local",actions:{},timeout:-1,urgency:1,transient:false,time:1000,pinned:false}
    var update=first.call("receive",[entry,1000,false])
    verify(Array.isArray(update.effects))
    compare(update.effects.filter(function(effect) { return effect.operation === "arrived" }).length,1)
    compare(update.effects[0].operation,"arrived")
    compare(first.call("view",[]).count,1)
    verify(Array.isArray(first.call("view",[]).items))
    verify(Array.isArray(first.call("view",[]).history))
    var nativeItems=first.call("view",[]).items
    compare(nativeItems.map(function(item) { return item.id }).join(","),"7")
    nativeItems[0].summary="caller-only mutation"
    compare(first.call("view",[]).items[0].summary,"🦀")
    compare(second.call("view",[]).count,0)
    first.call("pin",[7])
    var saved=first.call("save",[])
    compare(saved.metadata["7"].pinned,true)
    second.call("restore",[saved])
    second.call("receive",[entry,1005,true])
    compare(second.call("view",[]).items[0].pinned,true)
    first.call("closed",[7,2])
    compare(first.call("view",[]).count,0)
    compare(second.call("view",[]).count,1)
    var rejected=false
    try { first.call("unknown",["private fixture text"]) }
    catch(error) {rejected=true;verify(String(error).indexOf("private fixture text")<0)}
    verify(rejected)
  }
}
