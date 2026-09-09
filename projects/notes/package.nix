{
  lib,
  pkgs,
  quickshellInput,
}:
let
  quickshell = quickshellInput.packages.${pkgs.stdenv.hostPlatform.system}.default;
  tools = import ../../packages/core/tools.nix { inherit pkgs; };
  markdown = import ../markdown/package.nix { inherit pkgs lib; };
  qmlPath = lib.concatStringsSep ":" [
    "${markdown}/lib/qt-6/qml"
    "${pkgs.qt6.qtmultimedia}/lib/qt-6/qml"
  ];
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "seele-notes";
  version = "2.0.0";
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
    for component in NotesStore MarkdownEditor AudioStrip SetupView MigrationPanel MemoPlayer; do
      install -m644 "${./.}/$component.qml" "$out/share/seele-notes/$component.qml"
      substituteInPlace "$out/share/seele-notes/$component.qml" \
        --replace-quiet 'import "../shared" as Shared' 'import "shared" as Shared'
    done
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
      --prefix QML_IMPORT_PATH : "${qmlPath}" \
      --prefix QML2_IMPORT_PATH : "${qmlPath}" \
      --prefix QT_PLUGIN_PATH : "${pkgs.qt6.qtmultimedia}/lib/qt-6/plugins"
    mkdir -p "$out/share/applications"
    cat > "$out/share/applications/dev.silas.SeeleNotes.desktop" <<DESKTOP
    [Desktop Entry]
    Type=Application
    Name=Seele Notes
    Comment=Quick capture into an Obsidian vault
    Exec=$out/bin/seele-notes
    Icon=accessories-text-editor
    Categories=Office;Audio;Recorder;
    Keywords=notes;markdown;obsidian;memo;voice;recording;
    Terminal=false
    DESKTOP
    runHook postInstall
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    grep -Fqx 'import "shared" as Shared' "$out/share/seele-notes/shell.qml"
    ! grep -Fqr 'import "../shared"' "$out/share/seele-notes/"
    # The live editor is the whole point of this application, so the module
    # that formats it has to be present and importable rather than optional.
    grep -Fq 'import Seele.Markdown' "$out/share/seele-notes/MarkdownEditor.qml"
    test -f "${markdown}/lib/qt-6/qml/Seele/Markdown/qmldir"
    qmllint -I ${quickshell}/lib/qt-6/qml \
      -I ${pkgs.qt6.qtmultimedia}/lib/qt-6/qml \
      -I ${markdown}/lib/qt-6/qml \
      "$out/share/seele-notes/"*.qml "$out/share/seele-notes/shared/"*.qml
    node ${../../tests/notes.js} "$out/share/seele-notes/notes.js"
    node ${../../tests/notes-store.js} "$out/share/seele-notes/NotesStore.qml" "$out/share/seele-notes/notes.js"
    python3 ${../../tests/notes.py} "$out/libexec/seele-notes-store"
    bash ${../../tests/notes-editor.sh} \
      "$out/share/seele-notes" \
      ${../../tests/tst_noteseditor.qml} \
      ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
      ${markdown}/lib/qt-6/qml
    # The editor fixture instantiates one component. This compiles the whole
    # window with the real runtime imports, which is what catches a property
    # that only exists in the version of a shared component this file expects.
    QML_IMPORT_PATH=${qmlPath} QML2_IMPORT_PATH=${qmlPath} \
      bash ${../../tests/shell-load.sh} \
        ${quickshell}/bin/quickshell \
        "$out/share/seele-notes" \
        ${pkgs.sway-unwrapped}/bin/sway
    runHook postInstallCheck
  '';

  meta = {
    description = "Quick Markdown capture and voice memos for an Obsidian vault";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    mainProgram = "seele-notes";
  };
}
