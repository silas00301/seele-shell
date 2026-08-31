{
  lib,
  pkgs,
  quickshellInput,
}:
let
  quickshell = quickshellInput.packages.${pkgs.stdenv.hostPlatform.system}.default;
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
    pkgs.makeWrapper
    pkgs.qt6.qtdeclarative
  ];

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/bin" "$out/share/seele-lock"
    install -m644 ${./shell.qml} "$out/share/seele-lock/shell.qml"
    ${tools}/bin/seele-tools grain "$out/share/seele-lock/grain.png"
    makeWrapper ${tools}/bin/seele-tools "$out/bin/seele-lock" \
      --add-flags lock-run \
      --set SEELE_QUICKSHELL '${quickshell}/bin/quickshell' \
      --set SEELE_CONFIG "$out/share/seele-lock" \
      --prefix PATH : "${runtimePath}"

    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck

    test -f "$out/share/seele-lock/shell.qml"
    test -s "$out/share/seele-lock/grain.png"
    head -c 8 "$out/share/seele-lock/grain.png" | od -An -tx1 | grep -q "89 50 4e 47"
    test -x "$out/bin/seele-lock"
    "$out/bin/seele-lock" --help >/dev/null
    ${quickshell}/bin/quickshell --private-check-compat
    qmllint -I ${quickshell}/lib/qt-6/qml "$out/share/seele-lock/shell.qml"

    runHook postInstallCheck
  '';

  meta = {
    description = "Seele-native Quickshell session lock";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "seele-lock";
  };
}
