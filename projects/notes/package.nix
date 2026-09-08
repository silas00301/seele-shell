{
  lib,
  pkgs,
  quickshellInput,
}:
let
  quickshell = quickshellInput.packages.${pkgs.stdenv.hostPlatform.system}.default;
  tools = import ../../packages/core/tools.nix { inherit pkgs; };
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "seele-notes";
  version = "1.0.0";
  dontUnpack = true;
  dontWrapQtApps = true;
  nativeBuildInputs = [
    pkgs.makeWrapper
    pkgs.qt6.qtdeclarative
    pkgs.nodejs
    pkgs.python3
  ];

  installPhase = ''
    runHook preInstall
    mkdir -p "$out/share/seele-notes/shared" "$out/bin" "$out/libexec"
    cp ${../shared}/*.qml "$out/share/seele-notes/shared/"
    ${tools}/bin/seele-tools grain "$out/share/seele-notes/shared/grain.png"
    install -m644 ${./shell.qml} "$out/share/seele-notes/shell.qml"
    substituteInPlace "$out/share/seele-notes/shell.qml" \
      --replace-fail 'import "../shared" as Shared' 'import "shared" as Shared'
    install -m644 ${./NotesStore.qml} "$out/share/seele-notes/NotesStore.qml"
    install -m644 ${./MemoPlayer.qml} "$out/share/seele-notes/MemoPlayer.qml"
    install -m644 ${./notes.js} "$out/share/seele-notes/notes.js"
    ln -s ${tools}/bin/seele-tools "$out/libexec/seele-notes-store"
    makeWrapper "$out/libexec/seele-notes-store" "$out/bin/seele-notes-store" \
      --prefix PATH : "${lib.makeBinPath [ pkgs.pulseaudio ]}"
    cat > "$out/libexec/launch-notes" <<SCRIPT
    #!${pkgs.runtimeShell}
    if ${quickshell}/bin/quickshell ipc -n -p "$out/share/seele-notes" call -- seele-notes open >/dev/null 2>&1; then
      exit 0
    fi
    exec ${quickshell}/bin/quickshell -n -p "$out/share/seele-notes"
    SCRIPT
    chmod +x "$out/libexec/launch-notes"
    makeWrapper "$out/libexec/launch-notes" "$out/bin/seele-notes" \
      --prefix PATH : "$out/bin" \
      --prefix QML2_IMPORT_PATH : "${pkgs.qt6.qtmultimedia}/lib/qt-6/qml" \
      --prefix QT_PLUGIN_PATH : "${pkgs.qt6.qtmultimedia}/lib/qt-6/plugins"
    mkdir -p "$out/share/applications"
    cat > "$out/share/applications/dev.silas.SeeleNotes.desktop" <<DESKTOP
    [Desktop Entry]
    Type=Application
    Name=Seele Notes
    Comment=Notes and voice memos
    Exec=$out/bin/seele-notes
    Icon=accessories-text-editor
    Categories=Office;Audio;Recorder;
    Keywords=notes;memo;voice;recording;
    Terminal=false
    DESKTOP
    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    grep -Fqx 'import "shared" as Shared' "$out/share/seele-notes/shell.qml"
    ! grep -Fq 'import "../shared"' "$out/share/seele-notes/shell.qml"
    qmllint -I ${quickshell}/lib/qt-6/qml -I ${pkgs.qt6.qtmultimedia}/lib/qt-6/qml \
      "$out/share/seele-notes/"*.qml "$out/share/seele-notes/shared/"*.qml
    node ${../../tests/notes.js} "$out/share/seele-notes/notes.js"
    node ${../../tests/notes-store.js} "$out/share/seele-notes/NotesStore.qml" "$out/share/seele-notes/notes.js"
    python3 ${../../tests/notes.py} "$out/libexec/seele-notes-store"
    runHook postInstallCheck
  '';

  meta = {
    description = "Local notes and voice memos in Seele's desktop style";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "seele-notes";
  };
}
