{
  lib,
  pkgs,
  quickshell,
}:
let
  librepods = pkgs.librepods.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ../../packages/core/patches/librepods-status.patch ];
  });
  notes = import ../notes/package.nix { inherit lib pkgs quickshell; };
  tools = import ../../packages/core/tools.nix { inherit pkgs; };
  nativeQml = import ../qml/package.nix { inherit pkgs; };
  tests = ../../tests;
  runtime = import ../../packages/core/runtime.nix { inherit pkgs; };
  integrations = import ../../packages/core/native.nix {
    inherit pkgs;
    name = "integrations";
  };
  prompt = import ../../packages/core/native.nix {
    inherit pkgs;
    name = "prompt";
  };
  fontConfig = pkgs.makeFontsConf {
    fontDirectories = [ pkgs.maple-mono.NF-CN ];
  };
  generationSwitch = import ../../packages/core/native.nix {
    inherit pkgs;
    name = "repo-tools";
  };
  runtimePath = lib.makeBinPath [
    notes
    pkgs.alsa-utils
    pkgs.bluez
    pkgs.cameractrls-gtk4
    pkgs.coreutils
    pkgs.findutils
    pkgs.gawk
    pkgs.ghostty
    pkgs.gh
    pkgs.glib
    pkgs.git
    pkgs.grim
    pkgs.hyprland
    pkgs.iproute2
    pkgs.jq
    pkgs.jujutsu
    librepods
    pkgs.libnotify
    pkgs.networkmanager
    pkgs.networkmanagerapplet
    pkgs.nvd
    pkgs.ookla-speedtest
    pkgs.openlogi
    pkgs.pipewire
    pkgs.playerctl
    pkgs.procps
    pkgs.proton-vpn
    pkgs.proton-vpn-cli
    pkgs.pulseaudio
    pkgs.socat
    pkgs.systemd
    pkgs.tailscale
    pkgs.util-linux
    pkgs.uwsm
    pkgs.v4l-utils
    pkgs.vicinae
    pkgs.voxtype-vulkan
    pkgs.wireplumber
    pkgs.wl-clipboard
    pkgs.wtype
    pkgs.xdg-utils
    quickshell
  ];
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "seele-shell";
  version = "1.0.0";

  dontUnpack = true;
  dontWrapQtApps = true;
  nativeBuildInputs = [
    pkgs.dbus
    pkgs.esbuild
    pkgs.glib
    pkgs.imagemagick
    pkgs.jq
    pkgs.makeBinaryWrapper
    pkgs.nodejs
    pkgs.python3
    pkgs.qt6.qtdeclarative
    (pkgs.zint-qt.override { withGUI = false; })
  ];

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/share/seele-shell" "$out/share/vicinae/extensions/seele-shell/assets" "$out/share/licenses/seele-shell" "$out/libexec/seele-shell" "$out/bin"
    install -m644 ${./shell.qml} "$out/share/seele-shell/shell.qml"
    install -m644 ${./HeadphonesIcon.qml} "$out/share/seele-shell/HeadphonesIcon.qml"
    install -m644 ${./NotificationStore.qml} "$out/share/seele-shell/NotificationStore.qml"
    install -m644 ${./TransfersStore.qml} "$out/share/seele-shell/TransfersStore.qml"
    install -m644 ${./TransfersPanel.qml} "$out/share/seele-shell/TransfersPanel.qml"
    substituteInPlace "$out/share/seele-shell/TransfersPanel.qml" --replace-fail 'import "../shared" as Shared' 'import "shared" as Shared'
    install -m644 ${./HomeAssistantPanel.qml} "$out/share/seele-shell/HomeAssistantPanel.qml"
    substituteInPlace "$out/share/seele-shell/HomeAssistantPanel.qml" --replace-fail 'import "../shared" as Shared' 'import "shared" as Shared'
    install -m644 ${./AiActivityStore.qml} "$out/share/seele-shell/AiActivityStore.qml"
    install -m644 ${./AiActivityPanel.qml} "$out/share/seele-shell/AiActivityPanel.qml"
    install -m644 ${./ai-activity.js} "$out/share/seele-shell/ai-activity.js"
    substituteInPlace "$out/share/seele-shell/AiActivityPanel.qml" --replace-fail 'import "../shared" as Shared' 'import "shared" as Shared'
    install -m644 ${./AiPrompt.qml} "$out/share/seele-shell/AiPrompt.qml"
    substituteInPlace "$out/share/seele-shell/AiPrompt.qml" --replace-fail 'import "../shared" as Shared' 'import "shared" as Shared'
    install -m644 ${./ai-prompt.js} "$out/share/seele-shell/ai-prompt.js"
    install -m644 ${./HomeAssistantStore.qml} "$out/share/seele-shell/HomeAssistantStore.qml"
    install -m644 ${./health.js} "$out/share/seele-shell/health.js"
    install -m644 ${./IntegrationHealthStore.qml} "$out/share/seele-shell/IntegrationHealthStore.qml"
    install -m644 ${./MaintenanceStore.qml} "$out/share/seele-shell/MaintenanceStore.qml"
    install -m644 ${./MaintenancePanel.qml} "$out/share/seele-shell/MaintenancePanel.qml"
    substituteInPlace "$out/share/seele-shell/MaintenancePanel.qml" --replace-fail '"../shared"' '"shared"'
    install -m644 ${./SystemHealthPanel.qml} "$out/share/seele-shell/SystemHealthPanel.qml"
    substituteInPlace "$out/share/seele-shell/SystemHealthPanel.qml" --replace-fail '"../shared"' '"shared"'
    install -m644 ${./GitHubStore.qml} "$out/share/seele-shell/GitHubStore.qml"
    install -m644 ${./FocusTimer.qml} "$out/share/seele-shell/FocusTimer.qml"
    mkdir -p "$out/share/seele-shell/shared"
    cp ${../shared}/*.qml ${../shared}/*.js "$out/share/seele-shell/shared/"
    ${tools}/bin/seele-grain "$out/share/seele-shell/shared/grain.png"
    substituteInPlace "$out/share/seele-shell/shell.qml" \
      --replace-fail 'import "../shared" as Shared' 'import "shared" as Shared'
    install -m644 ${./DictationState.qml} "$out/share/seele-shell/DictationState.qml"
    install -m644 ${./SystemState.qml} "$out/share/seele-shell/SystemState.qml"
    install -m644 ${./UriPicker.qml} "$out/share/seele-shell/UriPicker.qml"
    install -m644 ${./uri-picker.js} "$out/share/seele-shell/uri-picker.js"
    install -m644 ${../vicinae/seele.svg} "$out/share/seele-shell/seele.svg"
    install -m644 ${./claude.svg} "$out/share/seele-shell/claude.svg"
    install -m644 ${./openai.svg} "$out/share/seele-shell/openai.svg"
    install -m644 ${./opencode.svg} "$out/share/seele-shell/opencode.svg"
    install -m644 ${./pi.svg} "$out/share/seele-shell/pi.svg"
    install -m644 ${./media.js} "$out/share/seele-shell/media.js"
    install -m644 ${./media-speed.js} "$out/share/seele-shell/media-speed.js"
    install -m644 ${./player-volume.js} "$out/share/seele-shell/player-volume.js"
    install -m644 ${./network.js} "$out/share/seele-shell/network.js"
    install -m644 ${./notifications.js} "$out/share/seele-shell/notifications.js"
    install -m644 ${./focus.js} "$out/share/seele-shell/focus.js"
    install -m644 ${./github.js} "$out/share/seele-shell/github.js"
    install -m644 ${./time.js} "$out/share/seele-shell/time.js"
    install -m644 ${./CameraPreview.qml} "$out/share/seele-shell/CameraPreview.qml"
    esbuild ${./.}/opencode-status.ts --bundle --platform=node --format=esm \
      --external:@opencode-ai/plugin --define:SEELE_AGENT_HOOK=\"$out/bin/seele-agent-hook\" \
      --outfile="$out/share/seele-shell/opencode-status.ts"
    esbuild ${./.}/pi-status.ts --bundle --platform=node --format=esm \
      --external:@earendil-works/pi-coding-agent --define:SEELE_AGENT_HOOK=\"$out/bin/seele-agent-hook\" \
      --outfile="$out/share/seele-shell/pi-status.ts"
    install -m644 ${../tools/LICENSES/Something-X.txt} "$out/share/licenses/seele-shell/Something-X.txt"

    cp ${../vicinae/package.json} "$out/share/vicinae/extensions/seele-shell/package.json"
    cp ${../vicinae/seele.svg} "$out/share/vicinae/extensions/seele-shell/assets/seele.svg"
    cp -r ${../vicinae} vicinae
    chmod -R u+w vicinae
    substituteInPlace vicinae/runtime.ts \
      --replace-fail '@SEELE_SHELLCTL@' "$out/bin/seele-shellctl" \
      --replace-fail '@SEELE_CONTROL@' "$out/bin/seele-control" \
      --replace-fail '@HYPRCTL@' '${pkgs.hyprland}/bin/hyprctl' \
      --replace-fail '@WTYPE@' '${pkgs.wtype}/bin/wtype' \
      --replace-fail '@NVD@' '${pkgs.nvd}/bin/nvd' \
      --replace-fail '@SWITCH_GENERATION@' '${generationSwitch}/bin/seele-switch-generation'
    for command in $(jq -r '.commands[].name' vicinae/package.json); do
      esbuild "vicinae/$command.tsx" --bundle --platform=node --format=cjs --external:@raycast/api --external:react --external:react/jsx-runtime --outfile="$out/share/vicinae/extensions/seele-shell/$command.js"
    done
    esbuild vicinae/desktop.ts --bundle --platform=node --format=cjs --external:./runtime --outfile=desktop.cjs
    node ${tests}/vicinae.cjs "$PWD/desktop.cjs"
    cp vicinae/keybindings.tsx vicinae/keybindings-fixture.tsx
    printf '\nexport { loadBindings, executeBinding };\n' >> vicinae/keybindings-fixture.tsx
    esbuild vicinae/keybindings-fixture.tsx --bundle --platform=node --format=cjs \
      --external:./runtime --external:react --external:@raycast/api --outfile=keybindings.cjs
    node ${tests}/vicinae-keybindings.cjs "$PWD/keybindings.cjs"
    esbuild vicinae/runtime.ts --bundle --platform=node --format=cjs --external:@raycast/api --external:react --outfile=runtime.cjs
    esbuild vicinae/status.ts --bundle --platform=node --format=cjs --external:./runtime --external:react --outfile=status.cjs
    esbuild vicinae/generations.tsx --bundle --platform=node --format=cjs \
      --external:./runtime --external:react --external:@raycast/api --outfile=generation-review.cjs
    node ${tests}/vicinae-generation-review.cjs "$PWD/generation-review.cjs"
    node ${tests}/vicinae-runtime.cjs "$PWD/runtime.cjs" "$PWD/status.cjs"
    node ${tests}/vicinae-generations.mjs \
      "$PWD/vicinae/generation-data.mjs" \
      "$PWD/vicinae/generations.tsx" \
      ${./package.nix}

    makeWrapper ${quickshell}/bin/quickshell "$out/bin/seele-shell" \
      --prefix QML_IMPORT_PATH : "${nativeQml}/lib/qt-6/qml" \
      --prefix QML2_IMPORT_PATH : "${nativeQml}/lib/qt-6/qml" \
      --add-flags "-n -p $out/share/seele-shell" \
      --prefix QML2_IMPORT_PATH : "${pkgs.qt6.qtmultimedia}/lib/qt-6/qml" \
      --prefix QT_PLUGIN_PATH : "${pkgs.qt6.qtmultimedia}/lib/qt-6/plugins" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper ${integrations}/bin/seele-transfers "$out/bin/seele-transfers" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper ${integrations}/bin/seele-home-assistant "$out/bin/seele-home-assistant" \
      --prefix PATH : "${lib.makeBinPath [ pkgs.libsecret ]}"
    makeWrapper ${runtime}/bin/seele-github-status "$out/bin/seele-github-status" \
      --prefix PATH : "${lib.makeBinPath [ pkgs.gh ]}"
    makeWrapper ${prompt}/bin/seele-ai-prompt-worker "$out/bin/seele-ai-prompt-worker" \
      --set-default SEELE_SHELL_CODEX "${lib.getExe pkgs.codex}" \
      --set-default SEELE_SHELL_GRIM "${lib.getExe pkgs.grim}" \
      --set-default SEELE_SHELL_HYPRCTL "${pkgs.hyprland}/bin/hyprctl" \
      --set-default SEELE_SHELL_WL_PASTE "${pkgs.wl-clipboard}/bin/wl-paste" \
      --set-default SEELE_SHELL_WL_COPY "${pkgs.wl-clipboard}/bin/wl-copy" \
      --set-default SEELE_SHELL_WTYPE "${lib.getExe pkgs.wtype}"
    install -m755 ${tools}/bin/seele-tools "$out/libexec/seele-shell/seele-tools"
    for name in seele-agent-state seele-agent seele-agent-run seele-agent-hook seele-control seele-bt-receiver seele-mic-sync seele-nothing-headphones seele-bt-agent seele-os-session seele-shellctl seele-clock seele-yubikey-watch; do
      install -m755 "${tools}/bin/$name" "$out/libexec/seele-shell/$name"
    done
    makeTool() {
      local name=$1
      shift
      makeWrapper "$out/libexec/seele-shell/$name" "$out/bin/$name" \
        --prefix PATH : "$out/bin:${runtimePath}" \
        "$@"
    }
    makeTool seele-agent-state
    makeTool seele-agent
    makeTool seele-agent-run
    makeTool seele-agent-hook
    makeTool seele-control
    makeTool seele-bt-receiver
    makeTool seele-mic-sync
    makeTool seele-nothing-headphones
    makeTool seele-bt-agent
    makeTool seele-os-session
    makeTool seele-shellctl --set SEELE_SHELL_PATH "$out/share/seele-shell"
    makeTool seele-clock --set TZDIR "${pkgs.tzdata}/share/zoneinfo"
    makeTool seele-yubikey-watch
    makeWrapper ${tools}/bin/seele-dictation-levels "$out/bin/seele-dictation-levels"
    makeWrapper ${tools}/bin/seele-uri-worker "$out/bin/seele-uri-worker" \
      --prefix PATH : "${lib.makeBinPath [ pkgs.grim ]}" \
      --set TESSDATA_PREFIX "${tools.tesseract}/share/tessdata" \
      --set OMP_THREAD_LIMIT 1 \
      --set OMP_NUM_THREADS 1

    substituteInPlace "$out/share/seele-shell/"*.js "$out/share/seele-shell/"*.qml \
      --replace-quiet '../shared/Native.js' 'shared/Native.js' \
      --replace-quiet '../shared/ListModels.js' 'shared/ListModels.js'

    runHook postInstall
  '';

  passthru = {
    inherit librepods;
  };

  env = {
    SEELE_QML_FUNCTIONS = "${nativeQml.core}/bin/seele-qml-functions";
    SEELE_QML_BRIDGE = "${../shared/Native.js}";
    QML_IMPORT_PATH = "${nativeQml}/lib/qt-6/qml";
    QML2_IMPORT_PATH = "${nativeQml}/lib/qt-6/qml";
  };

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    export SEELE_QML_BRIDGE="$out/share/seele-shell/shared/Native.js"
    export SEELE_QML_LIST_MODELS="$out/share/seele-shell/shared/ListModels.js"

    test -f "$out/share/seele-shell/shell.qml"
    test -f "$out/share/seele-shell/shared/CenteredGlyph.qml"
    grep -Fqx 'import "shared" as Shared' "$out/share/seele-shell/shell.qml"
    ! grep -Fq 'import "../shared"' "$out/share/seele-shell/shell.qml"
    test -f "$out/share/seele-shell/SystemState.qml"
    test -f "$out/share/seele-shell/seele.svg"
    for mark in claude openai opencode pi; do
      test -f "$out/share/seele-shell/$mark.svg"
    done
    test -s "$out/share/seele-shell/shared/grain.png"
    head -c 8 "$out/share/seele-shell/shared/grain.png" | od -An -tx1 | grep -q "89 50 4e 47"
    for source in MaintenanceStore.qml MaintenancePanel.qml TransfersStore.qml TransfersPanel.qml AiActivityStore.qml AiActivityPanel.qml ai-activity.js health.js IntegrationHealthStore.qml SystemHealthPanel.qml AiPrompt.qml ai-prompt.js FocusTimer.qml focus.js HomeAssistantStore.qml GitHubStore.qml github.js network.js player-volume.js media-speed.js; do
      test -f "$out/share/seele-shell/$source"
    done
    test -f "$out/share/seele-shell/media.js"
    test -f "$out/share/seele-shell/time.js"
    test -f "$out/share/seele-shell/CameraPreview.qml"
    test -f "$out/share/seele-shell/opencode-status.ts"
    test -f "$out/share/seele-shell/pi-status.ts"
    test -f "$out/share/licenses/seele-shell/Something-X.txt"
    test -f "$out/share/vicinae/extensions/seele-shell/package.json"
    test -f "$out/share/vicinae/extensions/seele-shell/seele.js"
    for command in $(jq -r '.commands[].name' "$out/share/vicinae/extensions/seele-shell/package.json"); do
      test -s "$out/share/vicinae/extensions/seele-shell/$command.js"
    done
    test -f "$out/share/seele-shell/shared/Palette.js"
    node ${tests}/palette.js ${../shared/Palette.js} ${../shared/Theme.qml} \
      ${../lock/shell.qml} ${../greeter/shell.qml} ${../polkit/shell.qml}
    bash ${tests}/palette.sh "$out/share/seele-shell/shared" ${tests}/tst_palette.qml \
      ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml
    ${quickshell}/bin/quickshell --private-check-compat
    bash ${tests}/shell-load.sh ${quickshell}/bin/quickshell \
      "$out/share/seele-shell" ${pkgs.sway-unwrapped}/bin/sway
    bash ${tests}/home-assistant-panel.sh ${quickshell}/bin/quickshell \
      "$out/share/seele-shell" ${pkgs.sway-unwrapped}/bin/sway ${tests}/home-assistant-panel.qml
    qmllint -I ${nativeQml}/lib/qt-6/qml -I ${quickshell}/lib/qt-6/qml "$out/share/seele-shell/TransfersStore.qml" "$out/share/seele-shell/TransfersPanel.qml" "$out/share/seele-shell/AiActivityStore.qml" "$out/share/seele-shell/AiActivityPanel.qml" "$out/share/seele-shell/IntegrationHealthStore.qml" "$out/share/seele-shell/SystemHealthPanel.qml" "$out/share/seele-shell/MaintenanceStore.qml" "$out/share/seele-shell/MaintenancePanel.qml" "$out/share/seele-shell/DictationState.qml" "$out/share/seele-shell/shared/"*.qml "$out/share/seele-shell/shell.qml" "$out/share/seele-shell/shared/CenteredGlyph.qml" "$out/share/seele-shell/SystemState.qml" "$out/share/seele-shell/UriPicker.qml" "$out/share/seele-shell/HeadphonesIcon.qml" "$out/share/seele-shell/NotificationStore.qml" "$out/share/seele-shell/HomeAssistantStore.qml" "$out/share/seele-shell/HomeAssistantPanel.qml" "$out/share/seele-shell/GitHubStore.qml" "$out/share/seele-shell/FocusTimer.qml" "$out/share/seele-shell/AiPrompt.qml"
    bash ${tests}/headphones-icon.sh \
      "$out/share/seele-shell/HeadphonesIcon.qml" \
      ${tests}/tst_headphones.qml \
      ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml
    bash ${tests}/system-state.sh \
      "$out/share/seele-shell/SystemState.qml" \
      ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
      ${tests}/tst_systemstate.qml \
      ${nativeQml}/lib/qt-6/qml
    bash ${tests}/plain-labels.sh \
      "$out/share/seele-shell/shared" \
      ${tests}/tst_plainlabels.qml \
      ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml
    FONTCONFIG_FILE=${fontConfig} bash ${tests}/centered-glyph.sh \
      "$out/share/seele-shell/shared/CenteredGlyph.qml" \
      ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
      ${tests}/tst_centeredglyph.qml
    for command in seele-transfers seele-ai-prompt-worker seele-uri-worker seele-shell seele-home-assistant seele-github-status seele-agent-state seele-agent seele-agent-run seele-agent-hook seele-control seele-bt-receiver seele-bt-agent seele-mic-sync seele-nothing-headphones seele-os-session seele-shellctl seele-clock seele-yubikey-watch; do
      test -x "$out/bin/$command"
    done
    "$out/bin/seele-shellctl" --help >/dev/null
    bash ${tests}/agent-state.sh "$out/libexec/seele-shell/seele-agent-state"
    bash ${tests}/harness-status.sh \
      "$out/share/seele-shell/pi-status.ts" \
      "$out/share/seele-shell/opencode-status.ts" \
      "$out/libexec/seele-shell/seele-control" \
      "$out/libexec/seele-shell/seele-agent-hook"
    node ${tests}/feature-integrations.js "$out/share/seele-shell/shell.qml" ${./package.nix}
    node ${tests}/ai-activity.js "$out/share/seele-shell/ai-activity.js" "$out/share/seele-shell/AiActivityStore.qml"
    node ${tests}/transfers.js "$out/share/seele-shell/TransfersStore.qml" "$out/share/seele-shell/TransfersPanel.qml" "$out/share/seele-shell/shell.qml"
    node ${tests}/ai-prompt.js "$out/share/seele-shell/ai-prompt.js" "$out/share/seele-shell/AiPrompt.qml"
    node ${tests}/focus.js "$out/share/seele-shell/focus.js"
    bash ${tests}/focus-timer.sh ${quickshell}/bin/quickshell "$out/share/seele-shell"
    node ${tests}/panel-layouts.js "$out/share/seele-shell" ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml
    node ${tests}/home-assistant-store.js "$out/share/seele-shell/HomeAssistantStore.qml"
    node ${tests}/maintenance.js "$out/share/seele-shell/MaintenanceStore.qml"
    node ${tests}/health.js "$out/share/seele-shell/health.js"
    node ${tests}/github.js "$out/share/seele-shell/github.js" "$out/share/seele-shell/GitHubStore.qml"
    node ${tests}/network-addresses.js "$out/share/seele-shell/network.js"
    node ${tests}/media.js "$out/share/seele-shell/media.js"
    bash ${tests}/media-host.sh ${quickshell}/bin/quickshell "$out/share/seele-shell"
    node ${tests}/player-volume.js "$out/share/seele-shell/player-volume.js"
    node ${tests}/media-speed.js "$out/share/seele-shell/media-speed.js" "$out/share/seele-shell/shell.qml"
    node ${tests}/notifications.js "$out/share/seele-shell/notifications.js" "$out/share/seele-shell/NotificationStore.qml"
    bash ${tests}/notification-server.sh ${quickshell}/bin/quickshell \
      "$out/libexec/seele-shell/seele-shellctl" "$out/share/seele-shell"
    node ${tests}/time.js "$out/share/seele-shell/time.js"
    node ${tests}/uri-picker.js "$out/share/seele-shell/uri-picker.js" "$out/share/seele-shell/UriPicker.qml"
    TESSDATA_PREFIX=${tools.tesseract}/share/tessdata bash ${tests}/uri-picker.sh \
      ${tools}/bin/seele-uri-worker ${pkgs.dejavu_fonts}/share/fonts/truetype/DejaVuSans.ttf
    node ${tests}/status-patches.js "$out/share/seele-shell/shell.qml"
    node ${tests}/shell-presentation.js "$out/share/seele-shell/shell.qml"
    python3 ${tests}/dictation.py "$out/bin/seele-dictation-levels"
    bash ${tests}/clock.sh "$out/bin/seele-clock"
    PATH="${runtimePath}:$PATH" bash ${tests}/audio-routing.sh "$out/libexec/seele-shell/seele-control"
    PATH="${runtimePath}:$PATH" bash ${tests}/network-vpn.sh "$out/libexec/seele-shell/seele-control"
    PATH="${runtimePath}:$PATH" bash ${tests}/bluetooth-receiver-routing.sh "$out/libexec/seele-shell/seele-control"
    PATH="${runtimePath}:$PATH" bash ${tests}/bluetooth-receiver.sh \
      "$out/libexec/seele-shell/seele-control" \
      "$out/libexec/seele-shell/seele-bt-receiver" \
      "$out/libexec/seele-shell/seele-bt-agent"
    python3 ${tests}/bluetooth-pairing.py "$out/libexec/seele-shell/seele-control"
    node ${tests}/bluetooth-pairing.js "$out/share/seele-shell/shell.qml"
    bash ${tests}/control-actions.sh "$out/libexec/seele-shell/seele-control"
    bash ${tests}/mic-sync.sh "$out/libexec/seele-shell/seele-mic-sync" "$out/libexec/seele-shell/seele-shellctl"

    runHook postInstallCheck
  '';

  meta = {
    description = "Seele-native Quickshell desktop shell";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "seele-shell";
  };
}
