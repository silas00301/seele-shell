# Native configuration tools

`seele-portable-config` owns refresh of generated config links. Directory
descriptors pin its private lock and manifest publication, malformed metadata
does not erase links, missing sources retain the previous generation, and user
files or replaced links are preserved. Traversal, retained metadata and lock
waiting are bounded. The materializer never changes existing directory modes.

`seele-launch MANIFEST [ARGUMENT ...]` combines that materialization with direct
application execution. Its version-1 JSON manifest declares an absolute program,
fixed arguments, additional PATH entries, environment values, and an optional
configuration source/destination. All declarations validate before configuration
publication. Compiled Nix wrappers pin the manifest. There
is no runtime shell evaluation or additional child between the launcher and the
application: `exec` retains the PID, signals, terminal and exit status.

Environment values and config destinations support `$NAME`, `${NAME}`,
`${NAME:-default}`, `${NAME-default}`, `${NAME:+alternate}` and
`${NAME+alternate}`, including nested defaults. Values read from the environment
are data: spaces, quotes, wildcard characters and command-looking text are never
reinterpreted. Undefined bare variables, command substitution, arithmetic,
assignment and unsupported shell operators fail closed. Backslash escapes for
the double-quoted shell characters remain available. Unlike a shell fragment,
literal quote characters are ordinary data and need no balancing.

The manifest is bounded to 1 MiB, with at most 256 arguments and PATH components
and 4096 exports. Each expansion and the aggregate declared environment have a
1 MiB limit; nesting is limited to 16 levels. Variable substitution preserves
non-UTF-8 Unix environment bytes. PATH is prefixed before exports, and exports
are evaluated in sorted name order, retaining the original Home Manager wrapper
ordering. Declare data substitutions in `home.sessionVariables`; command logic
belongs in a native executable rather than an environment string.

`seele-home-backup` delegates numbered backup semantics to the packaged native
GNU `mv`, using a fixed argument vector, an exact destination, and `--` before
the original filename. Existing directories and directory symlinks cannot redirect
the backup inside another directory.
`seele-portable-apps`, `seele-inputs`, and `seele-project-text` provide the
catalog, offline input projection, and safe project-text selection paths. They
share runtime file/process bounds and canonical text handling.

Run `cargo test -p seele-config-tools --locked` for native boundary and real exec
fixtures. `tests/{materialize,catalog,inputs,project_text}.py` retain compatibility
assertions through the native executables; Python is a development dependency
only. The launcher fixture uses a private temporary home and a synthetic target,
checks exact arguments/environment/PID and exit status, and never launches a
configured application or touches the user's home.
