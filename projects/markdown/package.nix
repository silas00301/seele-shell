{
  lib,
  pkgs,
  ...
}:
pkgs.stdenv.mkDerivation {
  pname = "seele-markdown-qml";
  version = "1.0.0";
  src = ./.;

  nativeBuildInputs = [
    pkgs.cmake
    pkgs.ninja
    pkgs.qt6.qtdeclarative
    pkgs.qt6.wrapQtAppsHook
  ];
  buildInputs = [
    pkgs.qt6.qtbase
    pkgs.qt6.qtdeclarative
  ];
  dontWrapQtApps = true;

  # The engine resolves `import Seele.Markdown` from this tree, so the module
  # directory has to arrive intact: plugin, qmldir and type description.
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    test -f "$out/lib/qt-6/qml/Seele/Markdown/qmldir"
    test -n "$(find "$out/lib/qt-6/qml/Seele/Markdown" -name '*.so' -print -quit)"
    runHook postInstallCheck
  '';

  meta = {
    description = "Live Markdown formatting for Seele Notes' editor";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
  };
}
