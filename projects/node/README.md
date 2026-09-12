# Native Node boundary

`seele-core.node` exposes `evaluate(jsonString) -> jsonString` through the stable
[Node-API C interface](https://nodejs.org/api/n-api.html). It owns no global
handles or retained state, uses no V8 internals, and launches no processes. The
same Rust library and C ABI serve Qt and Node; policy stays in `qml-core`.

The boundary accepts exactly one string, checks its UTF-8 byte length before
allocating, and preserves explicit lengths including NUL. Rust validates the
strict bounded JSON envelope and returns an owned bounded envelope. The shim
copies that result into a Node string and releases it exactly once, including
Node conversion failures. Synchronous errors never expose input content.

The Unix package uses Node headers only at build time and leaves stable API
symbols for the host to resolve. Linux uses a shared object and Darwin uses
Apple's dynamic-lookup linker mode. Darwin execution must be validated on Darwin.
Run `node --expose-gc projects/node/test.cjs /absolute/path/seele-core.node` for
real addon bounds, Unicode, ownership, repeated allocation and worker-isolation
checks. No credentials or desktop services are needed.
