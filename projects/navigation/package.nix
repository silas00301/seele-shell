{ pkgs, ... }:
pkgs.stdenv.mkDerivation {
  pname = "seele-navigation-qml";
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
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    bash ${../../tests/keyboard-navigation.sh} ${../shared} \
      ${../../tests/tst_keyboardnavigation.qml} "$out/lib/qt-6/qml"
    qmllint -I "$out/lib/qt-6/qml" \
      ${../shared/KeyboardNavigation.qml} ${../shared/ActionArea.qml}
    runHook postInstallCheck
  '';
  meta = {
    description = "Keyboard navigation for Seele's Qt interfaces";
    license = pkgs.lib.licenses.mit;
    platforms = pkgs.lib.platforms.linux;
  };
}
