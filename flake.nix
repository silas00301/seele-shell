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
          tesseract = pkgs.tesseract5.override { enableLanguages = [ "eng" ]; };
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
              dbus
              dbus.dev
              pkg-config
              stdenv.cc
              clippy
              rust-analyzer
              rustc
              rustfmt
              tesseract
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
              {
                name = "NIX_LDFLAGS_${pkgs.stdenv.cc.suffixSalt}";
                eval = ''"-L${pkgs.lib.getLib tesseract}/lib ''${NIX_LDFLAGS_${pkgs.stdenv.cc.suffixSalt}:-}"'';
              }
              {
                name = "TESSDATA_PREFIX";
                value = "${tesseract}/share/tessdata";
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
                help = "Run the Rust, JavaScript, and QML tests";
                command = ''
                  set -e
                  cargo test --manifest-path projects/tools/Cargo.toml
                  node tests/media.js projects/shell/media.js
                  node tests/time.js projects/shell/time.js
                  node tests/uri-picker.js projects/shell/uri-picker.js projects/shell/UriPicker.qml
                  node tests/status-patches.js projects/shell/shell.qml
                  bash tests/system-state.sh \
                    projects/shell/SystemState.qml \
                    ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
                    tests/tst_systemstate.qml
                  FONTCONFIG_FILE=${fontConfig} bash tests/centered-glyph.sh \
                    projects/shell/CenteredGlyph.qml \
                    ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
                    tests/tst_centeredglyph.qml
                  QT_QPA_PLATFORM=offscreen qmltestrunner \
                    -import ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
                    -input tests/tst_cardhover.qml
                '';
              }
            ];
          };
        };
    };
}
