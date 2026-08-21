{
  pkgs,
  resources,
  piPackage,
  dashboardctlPackage,
  delegation ? true,
  widgets ? true,
  projectRoots ? ["~/personal" "~/work" "~/third-party"],
}: let
  extensionArgs =
    [
      "--extension"
      "${resources}/share/scufris/extensions/scufris/identity.ts"
      "--extension"
      "${resources}/share/scufris/extensions/scufris/calm.ts"
      "--extension"
      "${resources}/share/scufris/extensions/scufris/speech.ts"
    ]
    ++ pkgs.lib.optionals delegation [
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
      [
        pkgs.piper-tts
        pkgs.pipewire
      ]
      ++ pkgs.lib.optional widgets dashboardctlPackage;
    text = ''
      if [[ -z "''${SCUFRIS_PROJECT_ROOTS+x}" ]]; then
        export SCUFRIS_PROJECT_ROOTS=${pkgs.lib.escapeShellArg (builtins.toJSON projectRoots)}
      fi
      export SCUFRIS_FOREGROUND=1

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
