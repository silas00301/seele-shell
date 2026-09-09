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
          markdown = import ./projects/markdown/package.nix packageArgs;
          fontConfig = pkgs.makeFontsConf {
            fontDirectories = [ pkgs.maple-mono.NF-CN ];
          };
        in
        {
          packages = {
            default = import ./projects/shell/package.nix packageArgs;
            notes = import ./projects/notes/package.nix packageArgs;
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
              zbar
              inputs.quickshell.packages.${system}.default
            ];

            env = [
              {
                name = "URI_PUBLIC_SUFFIX_LIST";
                value = "${pkgs.publicsuffix-list}/share/publicsuffix/public_suffix_list.dat";
              }
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
                eval = ''"-L${pkgs.lib.getLib tesseract}/lib -L${pkgs.lib.getLib pkgs.zbar}/lib ''${NIX_LDFLAGS_${pkgs.stdenv.cc.suffixSalt}:-}"'';
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
                command = "nix build .#default .#notes .#greeter .#lock .#polkit";
              }
              {
                name = "test-shell";
                help = "Run the Rust, JavaScript, and QML tests";
                command = ''
                  set -e
                  cargo test --manifest-path projects/tools/Cargo.toml
                  bash tests/shell-load.sh ${inputs.quickshell.packages.${system}.default}/bin/quickshell projects/shell ${pkgs.sway-unwrapped}/bin/sway
                  node tests/feature-integrations.js projects/shell/shell.qml projects/shell/package.nix
                  node tests/ai-prompt.js projects/shell/ai-prompt.js projects/shell/AiPrompt.qml
                  PYTHONDONTWRITEBYTECODE=1 ${pkgs.python3}/bin/python3 tests/ai-prompt.py projects/ai-prompt/worker.py
                  node tests/focus.js projects/shell/focus.js
                  bash tests/focus-timer.sh ${inputs.quickshell.packages.${system}.default}/bin/quickshell projects/shell
                  node tests/panel-layouts.js projects/shell ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml
                  PYTHONDONTWRITEBYTECODE=1 ${pkgs.python3}/bin/python3 tests/home-assistant.py projects/home-assistant/control.py
                  PYTHONDONTWRITEBYTECODE=1 ${pkgs.python3.withPackages (ps: [ ps.aiohttp ])}/bin/python3 tests/home-assistant-live.py projects/home-assistant/control.py
                  bash tests/home-assistant-panel.sh ${inputs.quickshell.packages.${system}.default}/bin/quickshell projects/shell ${pkgs.sway-unwrapped}/bin/sway tests/home-assistant-panel.qml
                  node tests/home-assistant-store.js projects/shell/HomeAssistantStore.qml
                  PYTHONDONTWRITEBYTECODE=1 ${pkgs.python3}/bin/python3 tests/github.py projects/github/status.py
                  node tests/github.js projects/shell/github.js projects/shell/GitHubStore.qml
                  node tests/network-addresses.js projects/shell/network.js
                  node tests/media.js projects/shell/media.js
                  node tests/player-volume.js projects/shell/player-volume.js
                  node tests/media-speed.js projects/shell/media-speed.js projects/shell/shell.qml
                  node tests/notifications.js projects/shell/notifications.js projects/shell/NotificationStore.qml
                  node tests/time.js projects/shell/time.js
                  node tests/notes.js projects/notes/notes.js
                  node tests/notes-store.js projects/notes/NotesStore.qml projects/notes/notes.js
                  bash tests/notes-editor.sh \
                    projects/notes \
                    tests/tst_noteseditor.qml \
                    ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
                    ${markdown}/lib/qt-6/qml
                  node tests/uri-picker.js projects/shell/uri-picker.js projects/shell/UriPicker.qml
                  node tests/status-patches.js projects/shell/shell.qml
                  bash tests/system-state.sh \
                    projects/shell/SystemState.qml \
                    ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
                    tests/tst_systemstate.qml
                  FONTCONFIG_FILE=${fontConfig} bash tests/centered-glyph.sh \
                    projects/shared/CenteredGlyph.qml \
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
