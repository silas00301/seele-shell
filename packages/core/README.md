# Core

This package contains files shared by the Seele projects. It has no desktop
entrypoint. Projects should use these files instead of keeping copies.

It currently contains:

- `tools.nix`, which builds the shared Rust runtime, texture generator, and separate URI OCR worker.
  Only the OCR worker links to nixpkgs Tesseract; the auth clients retain their
  existing runtime libraries.
- `patches/`, which contains local patches for third-party inputs.
