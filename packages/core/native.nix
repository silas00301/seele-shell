# One build policy for the native workspace. Keep wrappers at integration
# boundaries so fixture executables cannot reach real accounts or services.
{ pkgs, name }:
let
  lib = pkgs.lib;
  crates = (builtins.fromTOML (builtins.readFile ../../Cargo.toml)).workspace.members;
  source = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions (
      [
        ../../Cargo.toml
        ../../Cargo.lock
      ]
      ++ lib.concatMap (
        crate:
        map (part: lib.fileset.maybeMissing (../.. + "/${crate}/${part}")) [
          "Cargo.toml"
          "src"
          "include"
          "tests"
          "benches"
          "LICENSES"
        ]
      ) crates
    );
  };
  tesseract = pkgs.tesseract5.override { enableLanguages = [ "eng" ]; };
  tools = name == "tools";
  markdown = name == "markdown-core";
  python =
    if name == "integrations" then pkgs.python3.withPackages (ps: [ ps.aiohttp ]) else pkgs.python3;
  runtimeDependencies = {
    broker = [ pkgs.codex ];
    maintenance = [
      pkgs.systemd
      pkgs.openssl
      pkgs.ghostty
      pkgs.libnotify
    ];
    shell-ai = [ pkgs.fzf ];
  };
  wrapped = builtins.hasAttr name runtimeDependencies;
  rawBin = if wrapped then "$out/libexec/seele-${name}" else "$out/bin";
  mainPrograms = {
    broker = "seele-codex";
    maintenance = "seele-maintenance";
    prompt = "seele-ai-prompt-worker";
    config-tools = "seele-inputs";
    failure-analysis = "seele-failure-report";
    runtime = "seele-github-status";
    integrations = "seele-home-assistant";
    desktop-tools = "seele-screenshot";
    repo-tools = "seele-check";
    qml-core = "seele-qml-functions";
  };
  fixtures = {
    runtime = ''python3 projects/runtime/tests/github.py "$out/bin/seele-github-status"'';
    integrations = ''
      python3 projects/integrations/tests/home_assistant.py "$out/bin/seele-home-assistant"
      python3 projects/integrations/tests/transfers.py "$out/bin/seele-transfers"
    '';
    config-tools = ''
      python3 projects/config-tools/tests/materialize.py "$out/bin/seele-portable-config"
      python3 projects/config-tools/tests/catalog.py "$out/bin/seele-portable-apps"
      python3 projects/config-tools/tests/project_text.py "$out/bin/seele-project-text"
      python3 projects/config-tools/tests/inputs.py "$out/bin/seele-inputs"
    '';
    failure-analysis = ''python3 projects/failure-analysis/tests/protocol.py "$out/bin/seele-failure-report"'';
    desktop-tools = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''python3 projects/desktop-tools/tests/screenshot.py "$out/bin/seele-screenshot"'';
    repo-tools = ''
      python3 projects/repo-tools/tests/check.py "$out/bin/seele-check"
      python3 projects/repo-tools/tests/protocol.py "$out/bin"
      python3 projects/repo-tools/tests/submodule.py "$out/bin/update-submodule"
    '';
    broker = ''
      python3 projects/broker/tests/protocol.py "${rawBin}/seele-codex"
      SEELE_BROKER_CODEX=${lib.getExe pkgs.codex} python3 projects/broker/tests/codex_loopback.py "${rawBin}/seele-codex"
    '';
  };
in
pkgs.rustPlatform.buildRustPackage {
  pname = "seele-${name}";
  version = "1.0.0";
  src = source;
  cargoLock.lockFile = ../../Cargo.lock;
  cargoBuildFlags = [
    "-p"
    "seele-${name}"
  ];
  cargoTestFlags = [
    "-p"
    "seele-${name}"
    "--all-features"
  ];
  nativeBuildInputs =
    lib.optionals (tools || markdown) [ pkgs.pkg-config ]
    ++ lib.optionals wrapped [ pkgs.makeBinaryWrapper ];
  buildInputs =
    lib.optionals tools [
      pkgs.dbus
      pkgs.zbar
      tesseract
    ]
    ++ lib.optionals markdown [ pkgs.pcre2 ];
  nativeCheckInputs = [
    pkgs.bash
    pkgs.coreutils
    pkgs.python3
  ]
  ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.procps ]
  ++ lib.optionals tools [ pkgs.dbus ]
  ++ lib.optionals (name == "prompt") [ pkgs.codex ]
  ++ lib.optionals (name == "shell-ai") [ pkgs.fzf ];
  preCheck =
    lib.optionalString (name == "prompt") ''
      export SEELE_TEST_CODEX=${lib.getExe pkgs.codex}
    ''
    + lib.optionalString (name == "shell-ai") ''
      export SEELE_TEST_FZF=${lib.getExe pkgs.fzf}
    '';
  nativeInstallCheckInputs = [
    python
    pkgs.ripgrep
    pkgs.bash
    pkgs.coreutils
  ]
  ++ lib.optionals (name == "repo-tools") [
    pkgs.git
    pkgs.jujutsu
  ];
  postInstall =
    lib.optionalString (name == "qml-core") ''
      install -Dm644 projects/qml-core/include/seele-core.h "$out/include/seele-core.h"
    ''
    + lib.optionalString wrapped ''
      mkdir -p "${rawBin}"
      for binary in "$out/bin/"*; do
        executable="$(basename "$binary")"
        mv "$binary" "${rawBin}/$executable"
        makeWrapper "${rawBin}/$executable" "$binary" \
          --prefix PATH : "${lib.makeBinPath (runtimeDependencies.${name})}" \
          --suffix PATH : /run/current-system/sw/bin \
          ${lib.optionalString (
            name == "broker"
          ) ''--set-default SEELE_BROKER_CODEX "${lib.getExe pkgs.codex}"''}
      done
    '';
  doInstallCheck = builtins.hasAttr name fixtures;
  installCheckPhase = ''
    runHook preInstallCheck
    export PYTHONDONTWRITEBYTECODE=1
    ${fixtures.${name} or ""}
    runHook postInstallCheck
  '';
  env = lib.optionalAttrs tools {
    URI_PUBLIC_SUFFIX_LIST = "${pkgs.publicsuffix-list}/share/publicsuffix/public_suffix_list.dat";
  };
  passthru = lib.optionalAttrs tools { inherit tesseract; };
  meta = {
    description = "Native Seele ${name} services and helpers";
    license = lib.licenses.mit;
    platforms =
      if
        builtins.elem name [
          "runtime"
          "qml-core"
          "config-tools"
          "repo-tools"
          "desktop-tools"
        ]
      then
        lib.platforms.unix
      else
        lib.platforms.linux;
  }
  // lib.optionalAttrs (!markdown) { mainProgram = mainPrograms.${name} or "seele-${name}"; };
}
