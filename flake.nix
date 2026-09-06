{
  description = "Seele's Quickshell desktop shell";

  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    devshell.url = "github:numtide/devshell";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    quickshell.url = "github:outfoxxed/quickshell";
    quickshell.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      imports = [
        inputs.devshell.flakeModule
      ];

      perSystem =
        { system, ... }:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            config.allowUnfree = true;
          };
          packageArgs = {
            inherit pkgs;
            lib = pkgs.lib;
            quickshellInput = inputs.quickshell;
          };
          fontConfig = pkgs.makeFontsConf {
            fontDirectories = [ pkgs.maple-mono.NF-CN ];
          };
        in
        {
          packages = {
            default = import ./projects/shell/package.nix packageArgs;
            greeter = import ./projects/greeter/package.nix packageArgs;
            lock = import ./projects/lock/package.nix packageArgs;
            polkit = import ./projects/polkit/package.nix packageArgs;
          };

          devshells.default = {
            name = "seele-shell";
            motd = "Seele Shell development environment";

            packages = with pkgs; [
              bash
              esbuild
              jq
              nodejs
              qt6.qtdeclarative
              cargo
              dbus.dev
              pkg-config
              stdenv.cc
              clippy
              rust-analyzer
              rustc
              rustfmt
              inputs.quickshell.packages.${system}.default
            ];

            env = [
              {
                name = "RUST_SRC_PATH";
                value = pkgs.rustPlatform.rustLibSrc;
              }
              {
                name = "PKG_CONFIG_PATH";
                value = "${pkgs.dbus.dev}/lib/pkgconfig";
              }
            ];

            commands = [
              {
                name = "check";
                help = "Evaluate the flake and run package checks";
                command = "nix flake check --no-build";
              }
              {
                name = "build-all";
                help = "Build every Seele package";
                command = "nix build .#default .#greeter .#lock .#polkit";
              }
              {
                name = "test-shell";
                help = "Run the Rust and JavaScript tests";
                command = ''
                  set -e
                  cargo test --manifest-path projects/tools/Cargo.toml
                  node tests/media.js projects/shell/media.js
                  node tests/time.js projects/shell/time.js
                  FONTCONFIG_FILE=${fontConfig} bash tests/centered-glyph.sh \
                    projects/shell/CenteredGlyph.qml \
                    ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
                    tests/tst_centeredglyph.qml
                '';
              }
            ];
          };
        };
    };
}
