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

      flake.lib.mkNativePackage = import ./packages/core/native.nix;

      perSystem =
        { system, config, ... }:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            config.allowUnfree = true;
          };
          quickshell = import ./packages/core/quickshell.nix {
            inherit pkgs;
            upstream = inputs.quickshell.packages.${system}.default;
          };
          packageArgs = {
            inherit pkgs quickshell;
            lib = pkgs.lib;
          };
          tesseract = pkgs.tesseract5.override { enableLanguages = [ "eng" ]; };
          markdown = import ./projects/markdown/package.nix packageArgs;
          nativeQml = import ./projects/qml/package.nix packageArgs;
          fontConfig = pkgs.makeFontsConf {
            fontDirectories = [ pkgs.maple-mono.NF-CN ];
          };
        in
        {
          packages = {
            qml-core = import ./packages/core/native.nix {
              inherit pkgs;
              name = "qml-core";
            };
            qml = nativeQml;
            node-core = import ./projects/node/package.nix { inherit pkgs; };
            runtime = import ./packages/core/native.nix {
              inherit pkgs;
              name = "runtime";
            };
            tools = import ./packages/core/native.nix {
              inherit pkgs;
              name = "tools";
            };
            markdown-core = import ./packages/core/native.nix {
              inherit pkgs;
              name = "markdown-core";
            };
            codex-broker = import ./packages/core/native.nix {
              inherit pkgs;
              name = "broker";
            };
            maintenance = import ./packages/core/native.nix {
              inherit pkgs;
              name = "maintenance";
            };
            prompt = import ./packages/core/native.nix {
              inherit pkgs;
              name = "prompt";
            };
            integrations = import ./packages/core/native.nix {
              inherit pkgs;
              name = "integrations";
            };
            config-tools = import ./packages/core/native.nix {
              inherit pkgs;
              name = "config-tools";
            };
            failure-analysis = import ./packages/core/native.nix {
              inherit pkgs;
              name = "failure-analysis";
            };
            shell-ai = import ./packages/core/native.nix {
              inherit pkgs;
              name = "shell-ai";
            };
            desktop-tools = import ./packages/core/native.nix {
              inherit pkgs;
              name = "desktop-tools";
            };
            repo-tools = import ./packages/core/native.nix {
              inherit pkgs;
              name = "repo-tools";
            };
            default = import ./projects/shell/package.nix packageArgs;
            notes = import ./projects/notes/package.nix packageArgs;
            greeter = import ./projects/greeter/package.nix packageArgs;
            lock = import ./projects/lock/package.nix packageArgs;
            polkit = import ./projects/polkit/package.nix packageArgs;
          };

          # Each package owns its protocol and integration checks; flake check
          # now builds them instead of merely exposing unchecked outputs.
          checks = config.packages;

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
              quickshell
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
                command =
                  "nix build "
                  + pkgs.lib.concatMapStringsSep " " (name: ".#${name}") (builtins.attrNames config.packages);
              }
              {
                name = "test-shell";
                help = "Run the Rust, JavaScript, and QML tests";
                command = ''
                  set -e
                  export SEELE_QML_FUNCTIONS="$PWD/target/debug/seele-qml-functions"
                  export QML_IMPORT_PATH="${nativeQml}/lib/qt-6/qml"
                  export QML2_IMPORT_PATH="${nativeQml}/lib/qt-6/qml"
                  SEELE_TEST_CODEX=${pkgs.lib.getExe pkgs.codex} SEELE_TEST_FZF=${pkgs.lib.getExe pkgs.fzf} cargo test --workspace --all-features --locked
                  cargo build --workspace --locked
                  bash tests/shell-load.sh ${quickshell}/bin/quickshell projects/shell ${pkgs.sway-unwrapped}/bin/sway
                  node tests/feature-integrations.js projects/shell/shell.qml projects/shell/package.nix
                  node tests/ai-activity.js projects/shell/ai-activity.js projects/shell/AiActivityStore.qml
                  node tests/transfers.js projects/shell/TransfersStore.qml projects/shell/TransfersPanel.qml projects/shell/shell.qml
                  node tests/maintenance.js projects/shell/MaintenanceStore.qml
                  node tests/health.js projects/shell/health.js
                  node tests/bluetooth-pairing.js projects/shell/shell.qml
                  node tests/ai-prompt.js projects/shell/ai-prompt.js projects/shell/AiPrompt.qml
                  node tests/focus.js projects/shell/focus.js
                  bash tests/focus-timer.sh ${quickshell}/bin/quickshell projects/shell
                  node tests/panel-layouts.js projects/shell ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml
                  PYTHONDONTWRITEBYTECODE=1 ${
                    pkgs.python3.withPackages (ps: [ ps.aiohttp ])
                  }/bin/python3 projects/integrations/tests/home_assistant.py target/debug/seele-home-assistant
                  PYTHONDONTWRITEBYTECODE=1 ${pkgs.python3}/bin/python3 projects/integrations/tests/transfers.py target/debug/seele-transfers
                  bash tests/home-assistant-panel.sh ${quickshell}/bin/quickshell projects/shell ${pkgs.sway-unwrapped}/bin/sway tests/home-assistant-panel.qml
                  node tests/home-assistant-store.js projects/shell/HomeAssistantStore.qml
                  PYTHONDONTWRITEBYTECODE=1 ${pkgs.python3}/bin/python3 projects/runtime/tests/github.py target/debug/seele-github-status
                  node tests/github.js projects/shell/github.js projects/shell/GitHubStore.qml
                  node tests/network-addresses.js projects/shell/network.js
                  node tests/media.js projects/shell/media.js
                  bash tests/media-host.sh ${quickshell}/bin/quickshell projects/shell
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
                    ${markdown}/lib/qt-6/qml \
                    ${nativeQml}/lib/qt-6/qml
                  node tests/uri-picker.js projects/shell/uri-picker.js projects/shell/UriPicker.qml
                  node tests/status-patches.js projects/shell/shell.qml
                  node tests/shell-presentation.js projects/shell/shell.qml
                  node tests/palette.js projects/shared/Palette.js projects/shared/Theme.qml projects/lock/shell.qml projects/greeter/shell.qml projects/polkit/shell.qml
                  bash tests/palette.sh projects/shared tests/tst_palette.qml ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml
                  QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software qmltestrunner \
                    -import ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
                    -import ${nativeQml}/lib/qt-6/qml -input tests/tst_nativefunctions.qml
                  bash tests/system-state.sh \
                    projects/shell/SystemState.qml \
                    ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
                    tests/tst_systemstate.qml \
                    ${nativeQml}/lib/qt-6/qml
                  FONTCONFIG_FILE=${fontConfig} bash tests/centered-glyph.sh \
                    projects/shared/CenteredGlyph.qml \
                    ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
                    tests/tst_centeredglyph.qml
                  bash tests/plain-labels.sh projects/shared tests/tst_plainlabels.qml ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml
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
