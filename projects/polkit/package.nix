{
  lib,
  pkgs,
  quickshell,
}:
pkgs.stdenvNoCC.mkDerivation {
  pname = "seele-polkit";
  version = "1.0.0";

  dontUnpack = true;
  dontWrapQtApps = true;
  nativeBuildInputs = [
    pkgs.makeBinaryWrapper
    pkgs.qt6.qtdeclarative
  ];

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/bin" "$out/share/seele-polkit/shared"
    install -m644 ${./shell.qml} "$out/share/seele-polkit/shell.qml"
    install -m644 ${../shared/Palette.js} "$out/share/seele-polkit/shared/Palette.js"
    install -m644 ${../shared/Shapes.js} "$out/share/seele-polkit/shared/Shapes.js"
    substituteInPlace "$out/share/seele-polkit/shell.qml" \
      --replace-fail 'import "../shared/Palette.js" as Palette' 'import "shared/Palette.js" as Palette' \
      --replace-fail 'import "../shared/Shapes.js" as Shapes' 'import "shared/Shapes.js" as Shapes'
    makeWrapper ${quickshell}/bin/quickshell "$out/bin/seele-polkit" \
      --add-flags "-p $out/share/seele-polkit"

    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test -f "$out/share/seele-polkit/shell.qml"
    test -f "$out/share/seele-polkit/shared/Palette.js"
    test -f "$out/share/seele-polkit/shared/Shapes.js"
    test -x "$out/bin/seele-polkit"
    ${quickshell}/bin/quickshell --private-check-compat
    qmllint -I ${quickshell}/lib/qt-6/qml "$out/share/seele-polkit/shell.qml"

    runHook postInstallCheck
  '';

  meta = {
    description = "Seele-native Quickshell PolicyKit authentication agent";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "seele-polkit";
  };
}
