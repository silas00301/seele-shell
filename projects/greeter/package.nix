{
  lib,
  pkgs,
  quickshell,
}:
let
  tools = import ../../packages/core/tools.nix { inherit pkgs; };
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "seele-greeter";
  version = "1.0.0";

  dontUnpack = true;
  dontWrapQtApps = true;
  nativeBuildInputs = [
    pkgs.makeBinaryWrapper
    pkgs.qt6.qtdeclarative
  ];

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/bin" "$out/share/seele-greeter/shared"
    substitute ${./shell.qml} "$out/share/seele-greeter/shell.qml" \
      --replace-fail '@SYSTEMCTL@' '${pkgs.systemd}/bin/systemctl'
    install -m644 ${../shared/Palette.js} "$out/share/seele-greeter/shared/Palette.js"
    install -m644 ${../shared/Motion.js} "$out/share/seele-greeter/shared/Motion.js"
    install -m644 ${../shared/Shapes.js} "$out/share/seele-greeter/shared/Shapes.js"
    install -m644 ${../shared/LoadingIndicator.qml} "$out/share/seele-greeter/shared/LoadingIndicator.qml"
    substituteInPlace "$out/share/seele-greeter/shell.qml" \
      --replace-fail 'import "../shared/Palette.js" as Palette' 'import "shared/Palette.js" as Palette' \
      --replace-fail 'import "../shared/Motion.js" as Motion' 'import "shared/Motion.js" as Motion' \
      --replace-fail 'import "../shared/Shapes.js" as Shapes' 'import "shared/Shapes.js" as Shapes' \
      --replace-fail 'import "../shared" as Shared' 'import "shared" as Shared'
    makeWrapper ${tools}/bin/seele-greeter-run "$out/bin/seele-greeter" \
      --set SEELE_QUICKSHELL '${quickshell}/bin/quickshell' \
      --set SEELE_HYPRCTL '${pkgs.hyprland}/bin/hyprctl' \
      --set SEELE_CONFIG "$out/share/seele-greeter"

    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test -f "$out/share/seele-greeter/shell.qml"
    test -f "$out/share/seele-greeter/shared/Palette.js"
    test -f "$out/share/seele-greeter/shared/LoadingIndicator.qml"
    test -x "$out/bin/seele-greeter"
    "$out/bin/seele-greeter" --help >/dev/null
    ${quickshell}/bin/quickshell --private-check-compat
    qmllint -I ${quickshell}/lib/qt-6/qml "$out/share/seele-greeter/shell.qml"
    grep -q 'model: Quickshell.screens' "$out/share/seele-greeter/shell.qml"
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
