# Keep one patched Qt host for every application and development fixture.
{ pkgs, upstream }:
let
  unwrapped = upstream.unwrapped.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ./patches/quickshell-network-mode.patch ];
    postPatch = (old.postPatch or "") + ''
      ${pkgs.python3}/bin/python3 ${../../tests/quickshell-network-mode.py} .
    '';
  });
  # Upstream exports a wrapper whose installPhase closes over its unwrapped
  # derivation. Override that copy explicitly so patches reach the executable.
  wrapped = upstream.overrideAttrs (old: {
    installPhase = ''
      mkdir -p "$out"
      cp -r ${unwrapped}/* "$out"
    '';
    passthru = (old.passthru or { }) // {
      inherit unwrapped;
      withModules =
        modules:
        wrapped.overrideAttrs (previous: {
          buildInputs = previous.buildInputs ++ modules;
        });
    };
  });
in
wrapped
