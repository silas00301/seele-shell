# Theme folder registration end to end, as the flake builds it. Every folder
# in themes/default.nix and the example fixture register; the example then
# lists beside a preset through the real helper; and each invalid fixture
# fails its own build, naming the folder and what it broke.
{ pkgs, configTools }:
let
  lib = pkgs.lib;
  themes = import ./themes.nix { inherit pkgs configTools; };
  fixtures = ../../tests/theme-folders;
  registered = themes.entries (import ../../themes);
  example = themes.entry (fixtures + "/example");
  preset = {
    id = "catppuccin-mocha";
    name = "Catppuccin Mocha";
    mode = "dark";
    palette = example.palette;
    vicinaeTheme = launcher;
  };
  launcher = pkgs.writeText "seele-theme-folders-vicinae.toml" ''
    [meta]
    name = "Fixture"
    variant = "dark"
  '';
  entry = theme: {
    inherit (theme)
      id
      name
      mode
      palette
      document
      ;
    vicinaeTheme = launcher;
  };
  catalog = pkgs.writeText "seele-theme-folders-catalog.json" (
    builtins.toJSON {
      version = 2;
      default = "catppuccin-mocha";
      fontFamily = "Maple Mono NF CN";
      wallpaper = "/etc/wallpaper/wallpaper.jpg";
      commands = { };
      themes = [ preset ] ++ map entry ([ example ] ++ registered);
    }
  );
  # What each invalid fixture's build has to say.
  refusals = {
    incomplete = "missing required slot accent";
    misnamed = ''id: "example" is not the folder's name'';
    unknown-slots = "sparkle: not a contract slot";
    escaping-file = ''soundNotification: "../misnamed/theme.json" is not a path inside the theme folder'';
  };
  failures = lib.mapAttrs (
    name: _: pkgs.testers.testBuildFailure (themes.register (fixtures + "/invalid/${name}"))
  ) refusals;
in
pkgs.runCommand "seele-theme-folders-check" { nativeBuildInputs = [ pkgs.jq ]; } ''
  export HOME="$TMPDIR" XDG_CONFIG_HOME="$TMPDIR/config" XDG_STATE_HOME="$TMPDIR/state"
  mkdir -p "$XDG_CONFIG_HOME/seele-theme"
  cp ${catalog} "$XDG_CONFIG_HOME/seele-theme/catalog.json"
  ${configTools}/bin/seele-theme list > listed.json
  test "$(jq -r '[.themes[].id] | join(" ")' listed.json)" = ${
    lib.escapeShellArg (
      lib.concatStringsSep " " (
        [ "catppuccin-mocha" ] ++ map (theme: theme.id) ([ example ] ++ registered)
      )
    )
  }
  test "$(jq -r '.themes[1].radius' listed.json)" = 14
  ${lib.concatStrings (
    lib.mapAttrsToList (name: line: ''
      grep -qxF ${lib.escapeShellArg "Seele Themes: Invalid theme folder ${name}:"} ${failures.${name}}/testBuildFailure.log
      grep -qxF ${lib.escapeShellArg "  ${line}"} ${failures.${name}}/testBuildFailure.log
    '') refusals
  )}
  touch "$out"
''
