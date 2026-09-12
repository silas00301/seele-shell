{
  lib,
  pkgs,
  quickshell,
}:
let
  tools = import ../../packages/core/tools.nix { inherit pkgs; };
in
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

    mkdir -p "$out/bin" "$out/share/seele-polkit"
    install -m644 ${./shell.qml} "$out/share/seele-polkit/shell.qml"

    # Same seeded generator seele-shell uses, so the grain film is the
    # identical texture rather than a second one that almost matches.
    install -m644 ${../shared/Palette.js} "$out/share/seele-polkit/Palette.js"
    substituteInPlace "$out/share/seele-polkit/shell.qml" \
      --replace-fail 'import "../shared/Palette.js" as Palette' 'import "Palette.js" as Palette'
    ${tools}/bin/seele-grain "$out/share/seele-polkit/grain.png"
    makeWrapper ${quickshell}/bin/quickshell "$out/bin/seele-polkit" \
      --add-flags "-p $out/share/seele-polkit"

    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test -f "$out/share/seele-polkit/shell.qml"
    test -f "$out/share/seele-polkit/Palette.js"
    test -s "$out/share/seele-polkit/grain.png"
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
