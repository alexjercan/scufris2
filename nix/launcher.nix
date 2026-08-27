{
  pkgs,
  resources,
  piPackage,
  voice ? false,
  projectRoots ? ["~/personal" "~/work" "~/third-party"],
}: let
  extensionArgs = [
    "--extension"
    "${resources}/share/scufris/extensions/scufris/workflow/index.ts"
    "--skill"
    "${resources}/share/scufris/skills/workflow"
    "--extension"
    "${resources}/share/scufris/extensions/scufris/voice/index.ts"
    "--extension"
    "${resources}/share/scufris/extensions/scufris/calm.ts"
    "--extension"
    "${resources}/share/scufris/extensions/scufris/service/index.ts"
    "--extension"
    "${resources}/share/scufris/extensions/scufris/widgets/index.ts"
    "--skill"
    "${resources}/share/scufris/skills/widgets"
  ];
  renderedArgs = pkgs.lib.concatMapStringsSep " " pkgs.lib.escapeShellArg extensionArgs;
in
  pkgs.writeShellApplication {
    name = "scufris";
    # No Piper here. The agent decides what is worth saying aloud and the
    # frontend synthesises it, so nothing in this process tree makes sound.
    runtimeInputs = [
      pkgs.python3
      pkgs.tmux
    ];
    text = ''
      if [[ -z "''${SCUFRIS_PROJECT_ROOTS+x}" ]]; then
        export SCUFRIS_PROJECT_ROOTS=${pkgs.lib.escapeShellArg (builtins.toJSON projectRoots)}
      fi
      export SCUFRIS_ROLE=orchestrator
      ${pkgs.lib.optionalString voice ''
        export SCUFRIS_VOICE_AVAILABLE=1
      ''}

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
