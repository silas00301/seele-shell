# Core

This package contains files shared by the Seele projects. It has no desktop
entrypoint. Projects should use these files instead of keeping copies.

It currently contains:

- `tools.nix`, which builds the shared Rust runtime and texture generator.
- `patches/`, which contains local patches for third-party inputs.
