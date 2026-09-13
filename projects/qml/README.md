# Seele.Core Qt binding

This plugin adapts the shared Rust C ABI to QML. It owns no desktop capability or
business policy. `Functions.call(operation, arguments)` invokes the corresponding
`qml-core` function in the same process. The shared `seele-core.h` header defines
both bindings' ABI and exports the common message-size limit.

Only bounded JSON-compatible Qt values are admitted. The boundary rejects
QObject pointers and arbitrary QVariant conversions before serializing input.
Adapters project enum singletons into numeric fields and dynamic maps into
ordinary objects with own data properties; a null-prototype object remains an
opaque Qt value and must not be passed into the ABI.
Rust validates the envelope, argument count, operation and domain policy.
Returned bytes are checked before conversion and always released through the
matching Rust allocator, including error paths.

The plugin captures the QML engine's built-in JSON parser on first use, decodes
the bounded response once and returns `QJSValue`. This is intentional: QVariant
lists become Qt sequence wrappers, which do not preserve `Array.isArray()` or
array methods used by the existing UI. The JSON decoder creates ordinary arrays
and own object properties, including `__proto__` and `constructor`, without
executing generated source. Native failures become QML exceptions rather than
empty successful objects.

`Functions.notificationState(now)` creates a JavaScript-owned QObject containing
one opaque Rust state. There is no global handle table. Calls return compact
effects after mutation; actual notification QObject actions remain with the host
adapter. Qt garbage collection or engine teardown releases the Rust object and
all retained text.

`tests/native-functions-bounds.cpp` exercises the pre-serialization boundary;
`tests/tst_nativefunctions.qml` tests the actual plugin, nested array/object
semantics, scalars/null, safe keys, errors, isolated notification objects and
reload snapshots. `tests/system-state.sh` tests both source and installed layouts,
including unchanged delegates and rendered pixels. `tests/media-host.sh` uses
real Quickshell enum singletons and player QObjects to guard repeat controls. The
package runs the C++
fixture through CTest and the QML fixture as an install check.

Build with the Qt version supplied by the pinned package set. The production
CMake requirement remains Qt 6.5 or newer. Standalone compilation and fixtures do
not replace the Nix build, complete Quickshell load or target-host visual checks.
