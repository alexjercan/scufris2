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
    # The programs the agent runs, which the terminal launcher carries too:
    # there is one composition and it runs them from either. No speech
    # inference among them. The agent decides what is worth saying aloud and
    # the frontend synthesises it, so nothing in this process tree makes sound.
    runtimeInputs = import ./agent-runtime.nix {inherit pkgs den briefing;};
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
