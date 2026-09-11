{
  pkgs,
  resources,
  piPackage,
  den,
  briefing,
  projectRoots ? ["~/personal" "~/work" "~/third-party"],
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
      # The briefing extension runs `tools/briefing` by resource path, so the
      # `python3` this puts on PATH is the one that has to be able to import
      # what those scripts import.
      (import ./python.nix {inherit pkgs;})
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
      export SCUFRIS_ROLE=orchestrator
      # Jobs this conversation delegates belong to the conversation, not to
      # the session that happened to start them. The session id changes on
      # every handoff; this token does not.
      export SCUFRIS_JOB_OWNER=foreground

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
