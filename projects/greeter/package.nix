{
  lib,
  pkgs,
  quickshellInput,
}:
let
  quickshell = quickshellInput.packages.${pkgs.stdenv.hostPlatform.system}.default;
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "seele-greeter";
  version = "1.0.0";

  dontUnpack = true;
  dontWrapQtApps = true;
  nativeBuildInputs = [
    pkgs.nodejs
    pkgs.qt6.qtdeclarative
  ];

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/bin" "$out/share/seele-greeter"
    substitute ${./shell.qml} "$out/share/seele-greeter/shell.qml" \
      --replace-fail '@SYSTEMCTL@' '${pkgs.systemd}/bin/systemctl'
    node ${../../packages/core/grain.js} "$out/share/seele-greeter/grain.png"
    substitute ${./greeter.sh} "$out/bin/seele-greeter" \
      --replace-fail '@QUICKSHELL@' '${quickshell}/bin/quickshell' \
      --replace-fail '@HYPRCTL@' '${pkgs.hyprland}/bin/hyprctl' \
      --replace-fail '@CONFIG@' "$out/share/seele-greeter"
    chmod 755 "$out/bin/seele-greeter"

    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test -f "$out/share/seele-greeter/shell.qml"
    test -s "$out/share/seele-greeter/grain.png"
    head -c 8 "$out/share/seele-greeter/grain.png" | od -An -tx1 | grep -q "89 50 4e 47"
    test -x "$out/bin/seele-greeter"
    bash -n "$out/bin/seele-greeter"
    "$out/bin/seele-greeter" --help >/dev/null
    ${quickshell}/bin/quickshell --private-check-compat
    qmllint -I ${quickshell}/lib/qt-6/qml "$out/share/seele-greeter/shell.qml"
    grep -q 'model: Quickshell.screens' "$out/share/seele-greeter/shell.qml"
    grep -q 'source: root.grain' "$out/share/seele-greeter/shell.qml"
    grep -q 'onInputReadyChanged: if (inputReady) focusDelay.restart()' "$out/share/seele-greeter/shell.qml"

    runHook postInstallCheck
  '';

  meta = {
    description = "Seele-native Quickshell greetd frontend";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "seele-greeter";
  };
}
