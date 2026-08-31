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
  # BlueZ hands pairing decisions to a registered D-Bus agent, which
  # needs a real object on the bus rather than a shell pipeline.
  agentPython = pkgs.python3.withPackages (ps: [
    ps.dbus-python
    ps.pygobject3
  ]);
  runtimePath = lib.makeBinPath [
    pkgs.alsa-utils
    pkgs.bash
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

    mkdir -p "$out/share/seele-shell" "$out/share/vicinae/extensions/seele-shell/assets" "$out/libexec/seele-shell" "$out/bin"
    install -m644 ${./shell.qml} "$out/share/seele-shell/shell.qml"
    install -m644 ${../vicinae/seele.svg} "$out/share/seele-shell/seele.svg"
    install -m644 ${./claude-code.svg} "$out/share/seele-shell/claude-code.svg"
    node ${../../packages/core/grain.js} "$out/share/seele-shell/grain.png"
    install -m644 ${./media.js} "$out/share/seele-shell/media.js"
    install -m644 ${./time.js} "$out/share/seele-shell/time.js"
    install -m644 ${./CameraPreview.qml} "$out/share/seele-shell/CameraPreview.qml"
    install -m644 ${./opencode-status.ts} "$out/share/seele-shell/opencode-status.ts"
    install -m644 ${./pi-status.ts} "$out/share/seele-shell/pi-status.ts"

    cp ${../vicinae/package.json} "$out/share/vicinae/extensions/seele-shell/package.json"
    cp ${../vicinae/seele.svg} "$out/share/vicinae/extensions/seele-shell/assets/seele.svg"
    substitute ${../vicinae/seele.tsx} seele.tsx \
      --replace-fail '@SEELE_SHELLCTL@' "$out/bin/seele-shellctl"
    substitute ${../vicinae/keybindings.tsx} keybindings.tsx \
      --replace-fail '@HYPRCTL@' '${pkgs.hyprland}/bin/hyprctl'
    esbuild seele.tsx --bundle --platform=node --format=cjs --external:@raycast/api --external:react --external:react/jsx-runtime --outfile="$out/share/vicinae/extensions/seele-shell/seele.js"
    esbuild keybindings.tsx --bundle --platform=node --format=cjs --external:@raycast/api --external:react --external:react/jsx-runtime --outfile="$out/share/vicinae/extensions/seele-shell/keybindings.js"

    install -m755 ${../agents/agent-state.sh} "$out/libexec/seele-shell/agent-state"
    install -m755 ${../agents/agent-hook.sh} "$out/libexec/seele-shell/agent-hook"
    install -m755 ${../agents/agent-launch.sh} "$out/libexec/seele-shell/agent-launch"
    install -m755 ${../agents/agent-run.sh} "$out/libexec/seele-shell/agent-run"
    install -m755 ${../system/control.sh} "$out/libexec/seele-shell/control"
    install -m755 ${../bluetooth/bt-receiver.sh} "$out/libexec/seele-shell/bt-receiver"
    substitute ${../bluetooth/bt-agent.py} bt-agent \
      --replace-fail '#!/usr/bin/env python3' '#!${agentPython}/bin/python3'
    install -m755 bt-agent "$out/libexec/seele-shell/bt-agent"
    substitute ${../audio/mic-sync.py} mic-sync \
      --replace-fail '#!/usr/bin/env python3' '#!${agentPython}/bin/python3'
    install -m755 mic-sync "$out/libexec/seele-shell/mic-sync"
    install -m755 ${../system/os-session.sh} "$out/libexec/seele-shell/os-session"
    install -m755 ${../system/ctl.sh} "$out/libexec/seele-shell/ctl"
    install -m755 ${../system/clock.sh} "$out/libexec/seele-shell/clock"
    install -m755 ${../system/yubikey-watch.sh} "$out/libexec/seele-shell/yubikey-watch"
    patchShebangs "$out/libexec/seele-shell"

    makeWrapper ${quickshell}/bin/quickshell "$out/bin/seele-shell" \
      --add-flags "-n -p $out/share/seele-shell" \
      --prefix QML2_IMPORT_PATH : "${pkgs.qt6.qtmultimedia}/lib/qt-6/qml" \
      --prefix QT_PLUGIN_PATH : "${pkgs.qt6.qtmultimedia}/lib/qt-6/plugins" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/agent-state" "$out/bin/seele-agent-state" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/agent-launch" "$out/bin/seele-agent" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/agent-run" "$out/bin/seele-agent-run" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/agent-hook" "$out/bin/seele-agent-hook" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/control" "$out/bin/seele-control" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/bt-receiver" "$out/bin/seele-bt-receiver" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/mic-sync" "$out/bin/seele-mic-sync" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/bt-agent" "$out/bin/seele-bt-agent" \
      --prefix GI_TYPELIB_PATH : "${pkgs.glib}/lib/girepository-1.0" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/os-session" "$out/bin/seele-os-session" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/ctl" "$out/bin/seele-shellctl" \
      --set SEELE_SHELL_PATH "$out/share/seele-shell" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/clock" "$out/bin/seele-clock" \
      --set TZDIR "${pkgs.tzdata}/share/zoneinfo" \
      --prefix PATH : "$out/bin:${runtimePath}"
    makeWrapper "$out/libexec/seele-shell/yubikey-watch" "$out/bin/seele-yubikey-watch" \
      --prefix PATH : "$out/bin:${runtimePath}"

    runHook postInstall
  '';

  passthru.librepods = librepods;

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test -f "$out/share/seele-shell/shell.qml"
    test -f "$out/share/seele-shell/seele.svg"
    test -f "$out/share/seele-shell/claude-code.svg"
    test -s "$out/share/seele-shell/grain.png"
    head -c 8 "$out/share/seele-shell/grain.png" | od -An -tx1 | grep -q "89 50 4e 47"
    test -f "$out/share/seele-shell/media.js"
    test -f "$out/share/seele-shell/time.js"
    test -f "$out/share/seele-shell/CameraPreview.qml"
    test -f "$out/share/seele-shell/opencode-status.ts"
    test -f "$out/share/seele-shell/pi-status.ts"
    test -f "$out/share/vicinae/extensions/seele-shell/package.json"
    test -f "$out/share/vicinae/extensions/seele-shell/seele.js"
    test -f "$out/share/vicinae/extensions/seele-shell/keybindings.js"
    ${quickshell}/bin/quickshell --private-check-compat
    qmllint -I ${quickshell}/lib/qt-6/qml "$out/share/seele-shell/shell.qml"
    for command in seele-shell seele-agent-state seele-agent seele-agent-run seele-agent-hook seele-control seele-bt-receiver seele-bt-agent seele-mic-sync seele-os-session seele-shellctl seele-clock seele-yubikey-watch; do
      test -x "$out/bin/$command"
    done
    for script in "$out/libexec/seele-shell/"*; do
      case "$script" in
        *bt-agent | *mic-sync) ${agentPython}/bin/python3 -m py_compile "$script" ;;
        *) bash -n "$script" ;;
      esac
    done
    "$out/bin/seele-shellctl" --help >/dev/null
    bash ${../../tests/agent-state.sh} "$out/libexec/seele-shell/agent-state"
    bash ${../../tests/harness-status.sh} \
      "$out/share/seele-shell/pi-status.ts" \
      "$out/share/seele-shell/opencode-status.ts" \
      "$out/libexec/seele-shell/control" \
      "$out/libexec/seele-shell/agent-hook"
    node ${../../tests/media.js} "$out/share/seele-shell/media.js"
    node ${../../tests/time.js} "$out/share/seele-shell/time.js"
    bash ${../../tests/clock.sh} "$out/bin/seele-clock"
    PATH="${runtimePath}:$PATH" bash ${../../tests/network-vpn.sh} "$out/libexec/seele-shell/control"
    PATH="${runtimePath}:$PATH" bash ${../../tests/bluetooth-receiver.sh} \
      "$out/libexec/seele-shell/control" \
      "$out/libexec/seele-shell/bt-receiver" \
      "$out/libexec/seele-shell/bt-agent"
    bash ${../../tests/mic-sync.sh} "$out/libexec/seele-shell/mic-sync"

    runHook postInstallCheck
  '';

  meta = {
    description = "Seele-native Quickshell desktop shell";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "seele-shell";
  };
}
