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
    runtimeInputs = pkgs.lib.optional widgets dashboardctlPackage;
    text = ''
      pi=${pkgs.lib.escapeShellArg "${piPackage}/bin/pi"}
      if system_pi="$(type -P pi)"; then
        pi="$system_pi"
      fi

      exec "$pi" ${renderedArgs} "$@"
    '';
    meta = {
      description = "Pi launcher with configurable Scufris extensions";
      mainProgram = "scufris";
    };
  }
