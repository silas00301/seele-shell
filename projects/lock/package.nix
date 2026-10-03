{
  lib,
  pkgs,
  quickshell,
}:
let
  navigation = import ../navigation/package.nix { inherit pkgs; };
  tools = import ../../packages/core/tools.nix { inherit pkgs; };
  runtimePath = lib.makeBinPath [
    pkgs.coreutils
    pkgs.glibc.bin
    pkgs.hyprland
    pkgs.systemd
  ];
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "seele-lock";
  version = "1.0.0";

  dontUnpack = true;
  dontWrapQtApps = true;
  nativeBuildInputs = [
    pkgs.makeBinaryWrapper
    pkgs.qt6.qtdeclarative
  ];

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/bin" "$out/share/seele-lock"
    install -m644 ${./shell.qml} "$out/share/seele-lock/shell.qml"
    mkdir -p "$out/share/seele-lock/shared"
    for component in ActionArea FocusRing KeyboardNavigation; do
      install -m644 "${../shared}/$component.qml" "$out/share/seele-lock/shared/$component.qml"
    done
    substituteInPlace "$out/share/seele-lock/shell.qml" \
      --replace-fail 'import "../shared" as Shared' 'import "shared" as Shared'
    install -m644 ${../shared/Palette.js} "$out/share/seele-lock/Palette.js"
    substituteInPlace "$out/share/seele-lock/shell.qml" \
      --replace-fail 'import "../shared/Palette.js" as Palette' 'import "Palette.js" as Palette'
    ${tools}/bin/seele-grain "$out/share/seele-lock/grain.png"
    makeWrapper ${tools}/bin/seele-lock-run "$out/bin/seele-lock" \
      --set SEELE_QUICKSHELL '${quickshell}/bin/quickshell' \
      --set SEELE_CONFIG "$out/share/seele-lock" \
      --prefix PATH : "${runtimePath}" \
      --prefix QML_IMPORT_PATH : "${navigation}/lib/qt-6/qml" \
      --prefix QML2_IMPORT_PATH : "${navigation}/lib/qt-6/qml"

    runHook postInstall
  '';

  env.QML_IMPORT_PATH = "${navigation}/lib/qt-6/qml";
  env.QML2_IMPORT_PATH = "${navigation}/lib/qt-6/qml";
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test -f "$out/share/seele-lock/shell.qml"
    test -f "$out/share/seele-lock/Palette.js"
    test -s "$out/share/seele-lock/grain.png"
    head -c 8 "$out/share/seele-lock/grain.png" | od -An -tx1 | grep -q "89 50 4e 47"
    test -x "$out/bin/seele-lock"
    "$out/bin/seele-lock" --help >/dev/null
    ${quickshell}/bin/quickshell --private-check-compat
    qmllint -I ${navigation}/lib/qt-6/qml -I ${quickshell}/lib/qt-6/qml "$out/share/seele-lock/shell.qml"
    bash ${../../tests/shell-load.sh} ${quickshell}/bin/quickshell \
      "$out/share/seele-lock" ${pkgs.sway-unwrapped}/bin/sway

    runHook postInstallCheck
  '';

  meta = {
    description = "Seele-native Quickshell session lock";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "seele-lock";
  };
}
