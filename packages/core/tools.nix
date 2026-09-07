{ pkgs }:
let
  tesseract = pkgs.tesseract5.override { enableLanguages = [ "eng" ]; };
in
pkgs.rustPlatform.buildRustPackage {
  pname = "seele-tools";
  version = "1.0.0";

  src = ../../projects/tools;
  cargoLock.lockFile = ../../projects/tools/Cargo.lock;

  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [
    pkgs.dbus
    tesseract
  ];
  nativeCheckInputs = [ pkgs.dbus ];

  passthru = { inherit tesseract; };

  meta = {
    description = "Runtime helpers for Seele Shell";
    license = pkgs.lib.licenses.mit;
    platforms = pkgs.lib.platforms.linux;
  };
}
