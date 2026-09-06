{
  lib,
  pkgs,
  quickshellInput,
}:
let
  quickshell = quickshellInput.packages.${pkgs.stdenv.hostPlatform.system}.default;
  librepods = pkgs.librepods.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ../../packages/core/patches/librepods-status.patch ];
  });
  tools = import ../../packages/core/tools.nix { inherit pkgs; };
  fontConfig = pkgs.makeFontsConf {
    fontDirectories = [ pkgs.maple-mono.NF-CN ];
  };
  runtimePath = lib.makeBinPath [
    pkgs.alsa-utils
    pkgs.bluez
    pkgs.cameractrls-gtk4
    pkgs.coreutils
    pkgs.findutils
    pkgs.gawk
    pkgs.ghostty
    pkgs.git
    pkgs.hyprland
    pkgs.iproute2
    pkgs.jq
    pkgs.jujutsu
    librepods
    pkgs.mako
    pkgs.networkmanager
    pkgs.networkmanagerapplet
    pkgs.ookla-speedtest
    pkgs.openlogi
    pkgs.pipewire
    pkgs.playerctl
    pkgs.procps
    pkgs.proton-vpn
    pkgs.proton-vpn-cli
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
    pkgs.esbuild
    pkgs.jq
    pkgs.makeWrapper
    pkgs.nodejs
    pkgs.qt6.qtdeclarative
  ];

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/share/seele-shell" "$out/share/vicinae/extensions/seele-shell/assets" "$out/share/licenses/seele-shell" "$out/libexec/seele-shell" "$out/bin"
    install -m644 ${./shell.qml} "$out/share/seele-shell/shell.qml"
    install -m644 ${./CenteredGlyph.qml} "$out/share/seele-shell/CenteredGlyph.qml"
    install -m644 ${../vicinae/seele.svg} "$out/share/seele-shell/seele.svg"
    install -m644 ${./claude-code.svg} "$out/share/seele-shell/claude-code.svg"
    ${tools}/bin/seele-tools grain "$out/share/seele-shell/grain.png"
    install -m644 ${./media.js} "$out/share/seele-shell/media.js"
    install -m644 ${./time.js} "$out/share/seele-shell/time.js"
    install -m644 ${./CameraPreview.qml} "$out/share/seele-shell/CameraPreview.qml"
    install -m644 ${./opencode-status.ts} "$out/share/seele-shell/opencode-status.ts"
    install -m644 ${./pi-status.ts} "$out/share/seele-shell/pi-status.ts"
    install -m644 ${../tools/LICENSES/Something-X.txt} "$out/share/licenses/seele-shell/Something-X.txt"

    cp ${../vicinae/package.json} "$out/share/vicinae/extensions/seele-shell/package.json"
    cp ${../vicinae/seele.svg} "$out/share/vicinae/extensions/seele-shell/assets/seele.svg"
    substitute ${../vicinae/seele.tsx} seele.tsx \
      --replace-fail '@SEELE_SHELLCTL@' "$out/bin/seele-shellctl"
    substitute ${../vicinae/keybindings.tsx} keybindings.tsx \
      --replace-fail '@HYPRCTL@' '${pkgs.hyprland}/bin/hyprctl'
    esbuild seele.tsx --bundle --platform=node --format=cjs --external:@raycast/api --external:react --external:react/jsx-runtime --outfile="$out/share/vicinae/extensions/seele-shell/seele.js"
    esbuild keybindings.tsx --bundle --platform=node --format=cjs --external:@raycast/api --external:react --external:react/jsx-runtime --outfile="$out/share/vicinae/extensions/seele-shell/keybindings.js"

    makeWrapper ${quickshell}/bin/quickshell "$out/bin/seele-shell" \
      --add-flags "-n -p $out/share/seele-shell" \
      --prefix QML2_IMPORT_PATH : "${pkgs.qt6.qtmultimedia}/lib/qt-6/qml" \
      --prefix QT_PLUGIN_PATH : "${pkgs.qt6.qtmultimedia}/lib/qt-6/plugins" \
      --prefix PATH : "$out/bin:${runtimePath}"
    install -m755 ${tools}/bin/seele-tools "$out/libexec/seele-shell/seele-tools"
    for name in seele-agent-state seele-agent seele-agent-run seele-agent-hook seele-control seele-bt-receiver seele-mic-sync seele-nothing-headphones seele-bt-agent seele-os-session seele-shellctl seele-clock seele-yubikey-watch; do
      ln -s seele-tools "$out/libexec/seele-shell/$name"
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

    runHook postInstall
  '';

  passthru = {
    inherit librepods;
  };

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test -f "$out/share/seele-shell/shell.qml"
    test -f "$out/share/seele-shell/CenteredGlyph.qml"
    test -f "$out/share/seele-shell/seele.svg"
    test -f "$out/share/seele-shell/claude-code.svg"
    test -s "$out/share/seele-shell/grain.png"
    head -c 8 "$out/share/seele-shell/grain.png" | od -An -tx1 | grep -q "89 50 4e 47"
    test -f "$out/share/seele-shell/media.js"
    test -f "$out/share/seele-shell/time.js"
    test -f "$out/share/seele-shell/CameraPreview.qml"
    test -f "$out/share/seele-shell/opencode-status.ts"
    test -f "$out/share/seele-shell/pi-status.ts"
    test -f "$out/share/licenses/seele-shell/Something-X.txt"
    test -f "$out/share/vicinae/extensions/seele-shell/package.json"
    test -f "$out/share/vicinae/extensions/seele-shell/seele.js"
    test -f "$out/share/vicinae/extensions/seele-shell/keybindings.js"
    ${quickshell}/bin/quickshell --private-check-compat
    qmllint -I ${quickshell}/lib/qt-6/qml "$out/share/seele-shell/shell.qml" "$out/share/seele-shell/CenteredGlyph.qml"
    FONTCONFIG_FILE=${fontConfig} bash ${../../tests/centered-glyph.sh} \
      "$out/share/seele-shell/CenteredGlyph.qml" \
      ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
      ${../../tests/tst_centeredglyph.qml}
    for command in seele-shell seele-agent-state seele-agent seele-agent-run seele-agent-hook seele-control seele-bt-receiver seele-bt-agent seele-mic-sync seele-nothing-headphones seele-os-session seele-shellctl seele-clock seele-yubikey-watch; do
      test -x "$out/bin/$command"
    done
    "$out/bin/seele-shellctl" --help >/dev/null
    bash ${../../tests/agent-state.sh} "$out/libexec/seele-shell/seele-agent-state"
    bash ${../../tests/harness-status.sh} \
      "$out/share/seele-shell/pi-status.ts" \
      "$out/share/seele-shell/opencode-status.ts" \
      "$out/libexec/seele-shell/seele-control" \
      "$out/libexec/seele-shell/seele-agent-hook"
    node ${../../tests/media.js} "$out/share/seele-shell/media.js"
    node ${../../tests/time.js} "$out/share/seele-shell/time.js"
    bash ${../../tests/clock.sh} "$out/bin/seele-clock"
    PATH="${runtimePath}:$PATH" bash ${../../tests/network-vpn.sh} "$out/libexec/seele-shell/seele-control"
    PATH="${runtimePath}:$PATH" bash ${../../tests/bluetooth-receiver.sh} \
      "$out/libexec/seele-shell/seele-control" \
      "$out/libexec/seele-shell/seele-bt-receiver" \
      "$out/libexec/seele-shell/seele-bt-agent"
    bash ${../../tests/control-actions.sh} "$out/libexec/seele-shell/seele-control"
    bash ${../../tests/mic-sync.sh} "$out/libexec/seele-shell/seele-mic-sync" "$out/libexec/seele-shell/seele-shellctl"

    runHook postInstallCheck
  '';

  meta = {
    description = "Seele-native Quickshell desktop shell";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "seele-shell";
  };
}
