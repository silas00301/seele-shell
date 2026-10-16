# Drop-in theme folders, from a folder to an entry of the theme catalog.
#
# `register` runs `seele-theme register` on a folder at build time: it holds
# the folder to the theme contract, resolves the files it names into the
# registered copy and writes that copy, or fails the build naming every rule
# the folder broke. `entries` gives the catalog what it needs of each folder:
# the registered document, and the identity and Base16 palette the flake
# renders the desktop's other applications from while it evaluates. Those few
# facts are checked here, before anything reads them; everything else is the
# native check's, so the rules live in one place.
{
  pkgs,
  configTools ? import ./native.nix {
    inherit pkgs;
    name = "config-tools";
  },
}:
let
  lib = pkgs.lib;
  base16 = map (index: "base0${index}") (lib.stringToCharacters "0123456789ABCDEF");
  identifier = id: builtins.match "[a-z0-9]+(-[a-z0-9]+)*" id != null;
  hex = value: builtins.isString value && builtins.match "#[0-9a-f]{6}" value != null;
  idOf = folder: baseNameOf (toString folder);
in
rec {
  register =
    folder:
    let
      id = idOf folder;
    in
    if !identifier id then
      throw "Seele theme folder ${toString folder}: its name is the theme's id, lowercase letters and digits joined by hyphens"
    else
      pkgs.runCommand "seele-theme-${id}"
        {
          preferLocalBuild = true;
          allowSubstitutes = false;
        }
        ''
          ${configTools}/bin/seele-theme register ${lib.escapeShellArg id} ${folder} "$out"
        '';

  entry =
    folder:
    let
      id = idOf folder;
      file = folder + "/theme.json";
      document = if builtins.pathExists file then lib.importJSON file else { };
      palette = document.palette or null;
      problems =
        lib.optional (!builtins.pathExists file) "no theme.json"
        ++ lib.optional (!builtins.isString (document.name or null)) "missing name"
        ++ lib.optional (
          !lib.elem (document.mode or null) [
            "light"
            "dark"
          ]
        ) "mode: light or dark"
        ++ (
          if builtins.isAttrs palette then
            map (key: "palette.${key}: not a lowercase #rrggbb") (
              lib.filter (key: !hex (palette.${key} or null)) base16
            )
          else
            [ "missing palette, the 16 Base16 colours the desktop's other applications are themed from" ]
        );
      registered = register folder;
    in
    if problems != [ ] then
      throw "Invalid theme folder ${id}:\n  ${lib.concatStringsSep "\n  " problems}\nThe build checks the rest of the theme contract once these are in place."
    else
      {
        inherit id registered;
        inherit (document) name mode;
        palette = lib.genAttrs base16 (key: palette.${key});
        document = "${registered}/theme.json";
      };

  # The registered folders (themes/default.nix), as catalog entries.
  entries = folders: map entry folders;
}
