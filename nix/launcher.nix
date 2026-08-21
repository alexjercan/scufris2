{
  pkgs,
  resources,
  piPackage,
  dashboardctlPackage,
  delegation ? true,
  widgets ? true,
}: let
  extensionArgs =
    pkgs.lib.optionals delegation [
      "--extension"
      "${resources}/share/scufris/extensions/scufris/agents.ts"
      "--skill"
      "${resources}/share/scufris/skills/delegation"
    ]
    ++ pkgs.lib.optionals widgets [
      "--extension"
      "${resources}/share/scufris/extensions/scufris/widgets.ts"
      "--skill"
      "${resources}/share/scufris/skills/widgets"
    ];
  renderedArgs = pkgs.lib.concatMapStringsSep " " pkgs.lib.escapeShellArg extensionArgs;
in
  pkgs.writeShellApplication {
    name = "scufris";
    runtimeInputs =
      [piPackage]
      ++ pkgs.lib.optional widgets dashboardctlPackage;
    text = ''
      exec pi ${renderedArgs} "$@"
    '';
    meta = {
      description = "Pi launcher with configurable Scufris extensions";
      mainProgram = "scufris";
    };
  }
