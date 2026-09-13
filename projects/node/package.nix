{ pkgs, ... }:
let
  core = import ../../packages/core/native.nix {
    inherit pkgs;
    name = "qml-core";
  };
in
pkgs.stdenv.mkDerivation {
  pname = "seele-node-core";
  version = "1.0.0";
  src = ./.;
  nativeBuildInputs = [ pkgs.nodejs ];
  buildInputs = [ core ];
  dontConfigure = true;
  buildPhase = ''
    runHook preBuild
    $CXX -std=c++17 -O2 -fPIC -shared -DNAPI_VERSION=8 \
      -I${pkgs.nodejs}/include/node -I${core}/include addon.cpp \
      ${core}/lib/libseele_qml_core.a \
      ${
        if pkgs.stdenv.hostPlatform.isDarwin then
          "-Wl,-undefined,dynamic_lookup"
        else
          "-ldl -lpthread -lm -Wl,--exclude-libs,ALL"
      } \
      -o seele-core.node
    runHook postBuild
  '';
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    node --expose-gc test.cjs "$PWD/seele-core.node"
    runHook postCheck
  '';
  installPhase = ''
    runHook preInstall
    install -Dm755 seele-core.node "$out/lib/seele-core.node"
    runHook postInstall
  '';
  meta = {
    description = "Stable Node-API binding for Seele's shared Rust UI policy";
    license = pkgs.lib.licenses.mit;
    platforms = pkgs.lib.platforms.unix;
  };
}
