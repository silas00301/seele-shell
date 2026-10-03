{
  lib,
  pkgs,
  quickshell,
}:
let
  navigation = import ../navigation/package.nix { inherit pkgs; };
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

    mkdir -p "$out/bin" "$out/share/seele-greeter"
    substitute ${./shell.qml} "$out/share/seele-greeter/shell.qml" \
      --replace-fail '@SYSTEMCTL@' '${pkgs.systemd}/bin/systemctl'
    mkdir -p "$out/share/seele-greeter/shared"
    for component in ActionArea FocusRing KeyboardNavigation; do
      install -m644 "${../shared}/$component.qml" "$out/share/seele-greeter/shared/$component.qml"
    done
    substituteInPlace "$out/share/seele-greeter/shell.qml" \
      --replace-fail 'import "../shared" as Shared' 'import "shared" as Shared'
    install -m644 ${../shared/Palette.js} "$out/share/seele-greeter/Palette.js"
    substituteInPlace "$out/share/seele-greeter/shell.qml" \
      --replace-fail 'import "../shared/Palette.js" as Palette' 'import "Palette.js" as Palette'
    ${tools}/bin/seele-grain "$out/share/seele-greeter/grain.png"
    makeWrapper ${tools}/bin/seele-greeter-run "$out/bin/seele-greeter" \
      --set SEELE_QUICKSHELL '${quickshell}/bin/quickshell' \
      --set SEELE_HYPRCTL '${pkgs.hyprland}/bin/hyprctl' \
      --set SEELE_CONFIG "$out/share/seele-greeter" \
      --prefix QML_IMPORT_PATH : "${navigation}/lib/qt-6/qml" \
      --prefix QML2_IMPORT_PATH : "${navigation}/lib/qt-6/qml"

    runHook postInstall
  '';

  env.QML_IMPORT_PATH = "${navigation}/lib/qt-6/qml";
  env.QML2_IMPORT_PATH = "${navigation}/lib/qt-6/qml";
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test -f "$out/share/seele-greeter/shell.qml"
    test -f "$out/share/seele-greeter/Palette.js"
    test -s "$out/share/seele-greeter/grain.png"
    head -c 8 "$out/share/seele-greeter/grain.png" | od -An -tx1 | grep -q "89 50 4e 47"
    test -x "$out/bin/seele-greeter"
    "$out/bin/seele-greeter" --help >/dev/null
    ${quickshell}/bin/quickshell --private-check-compat
    qmllint -I ${navigation}/lib/qt-6/qml -I ${quickshell}/lib/qt-6/qml "$out/share/seele-greeter/shell.qml"
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
