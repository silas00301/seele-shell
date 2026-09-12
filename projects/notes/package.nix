{
  lib,
  pkgs,
  quickshell,
}:
let
  tools = import ../../packages/core/tools.nix { inherit pkgs; };
  nativeQml = import ../qml/package.nix { inherit pkgs; };
  tests = ../../tests;
  markdown = import ../markdown/package.nix { inherit pkgs lib; };
  qmlPath = lib.concatStringsSep ":" [
    "${nativeQml}/lib/qt-6/qml"
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
    pkgs.makeBinaryWrapper
    pkgs.qt6.qtdeclarative
    pkgs.nodejs
    pkgs.python3
  ];

  installPhase = ''
    runHook preInstall
    mkdir -p "$out/share/seele-notes/shared" "$out/bin" "$out/libexec"
    cp ${../shared}/*.qml ${../shared}/*.js "$out/share/seele-notes/shared/"
    ${tools}/bin/seele-grain "$out/share/seele-notes/shared/grain.png"
    install -m644 ${./shell.qml} "$out/share/seele-notes/shell.qml"
    substituteInPlace "$out/share/seele-notes/shell.qml" \
      --replace-fail 'import "../shared" as Shared' 'import "shared" as Shared'
    for component in NotesStore MarkdownEditor AudioStrip SetupView MigrationPanel MemoPlayer; do
      install -m644 "${./.}/$component.qml" "$out/share/seele-notes/$component.qml"
      substituteInPlace "$out/share/seele-notes/$component.qml" \
        --replace-quiet 'import "../shared" as Shared' 'import "shared" as Shared'
    done
    install -m644 ${./notes.js} "$out/share/seele-notes/notes.js"
    substituteInPlace "$out/share/seele-notes/notes.js" \
      --replace-quiet '../shared/Native.js' 'shared/Native.js'
    ln -s ${tools}/bin/seele-notes-store "$out/libexec/seele-notes-store"
    makeWrapper "$out/libexec/seele-notes-store" "$out/bin/seele-notes-store" \
      --prefix PATH : "${lib.makeBinPath [ pkgs.pulseaudio ]}"
    makeWrapper ${tools}/bin/seele-notes-run "$out/bin/seele-notes" \
      --set SEELE_QUICKSHELL "${quickshell}/bin/quickshell" \
      --set SEELE_CONFIG "$out/share/seele-notes" \
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

  env = {
    SEELE_QML_FUNCTIONS = "${nativeQml.core}/bin/seele-qml-functions";
    SEELE_QML_BRIDGE = "${../shared/Native.js}";
    QML_IMPORT_PATH = "${nativeQml}/lib/qt-6/qml";
    QML2_IMPORT_PATH = "${nativeQml}/lib/qt-6/qml";
  };

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    grep -Fqx 'import "shared" as Shared' "$out/share/seele-notes/shell.qml"
    ! grep -Fqr 'import "../shared"' "$out/share/seele-notes/"
    # The live editor is the whole point of this application, so the module
    # that formats it has to be present and importable rather than optional.
    grep -Fq 'import Seele.Markdown' "$out/share/seele-notes/MarkdownEditor.qml"
    test -f "${markdown}/lib/qt-6/qml/Seele/Markdown/qmldir"
    qmllint -I ${nativeQml}/lib/qt-6/qml -I ${quickshell}/lib/qt-6/qml \
      -I ${pkgs.qt6.qtmultimedia}/lib/qt-6/qml \
      -I ${markdown}/lib/qt-6/qml \
      "$out/share/seele-notes/"*.qml "$out/share/seele-notes/shared/"*.qml
    node ${tests}/notes.js "$out/share/seele-notes/notes.js"
    node ${tests}/notes-store.js "$out/share/seele-notes/NotesStore.qml" "$out/share/seele-notes/notes.js"
    python3 ${tests}/notes.py "$out/libexec/seele-notes-store"
    bash ${tests}/notes-editor.sh \
      "$out/share/seele-notes" \
      ${tests}/tst_noteseditor.qml \
      ${pkgs.qt6.qtdeclarative}/lib/qt-6/qml \
      ${markdown}/lib/qt-6/qml \
      ${nativeQml}/lib/qt-6/qml
    # The editor fixture instantiates one component. This compiles the whole
    # window with the real runtime imports, which is what catches a property
    # that only exists in the version of a shared component this file expects.
    QML_IMPORT_PATH=${qmlPath} QML2_IMPORT_PATH=${qmlPath} \
      bash ${tests}/shell-load.sh \
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
