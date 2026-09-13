{ pkgs, ... }:
let
  core = import ../../packages/core/native.nix {
    inherit pkgs;
    name = "qml-core";
  };
in
pkgs.stdenv.mkDerivation {
  pname = "seele-core-qml";
  version = "1.0.0";
  src = ./.;
  nativeBuildInputs = [
    pkgs.cmake
    pkgs.ninja
    pkgs.qt6.qtdeclarative
    pkgs.qt6.wrapQtAppsHook
  ];
  buildInputs = [
    core
    pkgs.qt6.qtbase
    pkgs.qt6.qtdeclarative
  ];
  dontWrapQtApps = true;
  cmakeFlags = [ "-DSEELE_BOUNDARY_TEST=${../../tests/native-functions-bounds.cpp}" ];
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    ctest --output-on-failure
    runHook postCheck
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    test -f "$out/lib/qt-6/qml/Seele/Core/qmldir"
    test -s "$out/lib/qt-6/qml/Seele/Core/libseelecore.so"
    QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software qmltestrunner \
      -import ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
      -import "$out/lib/qt-6/qml" -input ${../../tests/tst_nativefunctions.qml}
    runHook postInstallCheck
  '';
  passthru = { inherit core; };
  meta = {
    description = "Qt binding for Seele's shared Rust UI policy";
    license = pkgs.lib.licenses.mit;
    platforms = pkgs.lib.platforms.linux;
  };
}
