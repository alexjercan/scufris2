{
  pkgs,
  resources,
  piPackage,
  den,
  briefing,
  projectRoots ? ["~/personal" "~/work" "~/third-party"],
  briefingTime ? "08:00",
}: let
  extensionArgs = [
    "--extension"
    "${resources}/share/scufris/extensions/scufris/workflow/index.ts"
    "--skill"
    "${resources}/share/scufris/skills/workflow"
    "--extension"
    "${resources}/share/scufris/extensions/scufris/briefing/index.ts"
    "--extension"
    "${resources}/share/scufris/extensions/scufris/response.ts"
    "--extension"
    "${resources}/share/scufris/extensions/scufris/calm.ts"
    "--extension"
    "${resources}/share/scufris/extensions/scufris/service/index.ts"
    "--skill"
    "${resources}/share/scufris/skills/den"
  ];
  renderedArgs = pkgs.lib.concatMapStringsSep " " pkgs.lib.escapeShellArg extensionArgs;
in
  pkgs.writeShellApplication {
    name = "scufris";
    # No speech inference here. The agent decides what is worth saying aloud and the
    # frontend synthesises it, so nothing in this process tree makes sound.
    runtimeInputs = [
      pkgs.python3
      pkgs.tmux
      # The journal, which the den skill runs by name.
      den
      # The morning briefing, which the briefing extension runs by name.
      briefing
    ];
    text = ''
      if [[ -z "''${SCUFRIS_PROJECT_ROOTS+x}" ]]; then
        export SCUFRIS_PROJECT_ROOTS=${pkgs.lib.escapeShellArg (builtins.toJSON projectRoots)}
      fi
      # The morning the briefing extension arms its one timer for. `off` is a
      # deployment that wants no unprompted briefing at all.
      if [[ -z "''${SCUFRIS_BRIEFING_TIME+x}" ]]; then
        export SCUFRIS_BRIEFING_TIME=${pkgs.lib.escapeShellArg briefingTime}
      fi
      export SCUFRIS_ROLE=orchestrator

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
