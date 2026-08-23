{
  pkgs,
  resources,
  piPackage,
  dashboardctlPackage,
  dashboard ? true,
  voice ? false,
  piperPackage ? null,
  piperModel ? null,
  piperConfig ? null,
  projectRoots ? ["~/personal" "~/work" "~/third-party"],
}:
assert !voice || (piperPackage != null && piperModel != null && piperConfig != null); let
  extensionArgs =
    [
      "--extension"
      "${resources}/share/scufris/extensions/scufris/workflow/index.ts"
      "--skill"
      "${resources}/share/scufris/skills/workflow"
      "--extension"
      "${resources}/share/scufris/extensions/scufris/voice/index.ts"
      "--extension"
      "${resources}/share/scufris/extensions/scufris/calm.ts"
    ]
    ++ pkgs.lib.optionals dashboard [
      "--extension"
      "${resources}/share/scufris/extensions/scufris/dashboard/index.ts"
      "--skill"
      "${resources}/share/scufris/skills/dashboard"
    ];
  renderedArgs = pkgs.lib.concatMapStringsSep " " pkgs.lib.escapeShellArg extensionArgs;
in
  pkgs.writeShellApplication {
    name = "scufris";
    runtimeInputs =
      [
        pkgs.coreutils
        pkgs.python3
        pkgs.tmux
      ]
      ++ pkgs.lib.optionals voice [
        piperPackage
        pkgs.pipewire
      ]
      ++ pkgs.lib.optional dashboard dashboardctlPackage;
    text = ''
      if [[ -z "''${SCUFRIS_PROJECT_ROOTS+x}" ]]; then
        export SCUFRIS_PROJECT_ROOTS=${pkgs.lib.escapeShellArg (builtins.toJSON projectRoots)}
      fi
      if [[ -z "''${SCUFRIS_TMUX_SOCKET+x}" ]]; then
        if [[ -n "''${XDG_RUNTIME_DIR:-}" ]]; then
          scufris_runtime_root="$XDG_RUNTIME_DIR/scufris"
        elif [[ -n "''${XDG_STATE_HOME:-}" ]]; then
          scufris_runtime_root="$XDG_STATE_HOME/scufris/run"
        elif [[ "''${HOME:-}" != /homeless-shelter && -n "''${HOME:-}" ]]; then
          scufris_runtime_root="$HOME/.local/state/scufris/run"
        else
          scufris_runtime_root="''${TMPDIR:-/tmp}/scufris-runtime-$UID"
        fi
        mkdir -p -- "$scufris_runtime_root"
        chmod 0700 "$scufris_runtime_root"
        scufris_runtime_root="$(unset CDPATH; cd -- "$scufris_runtime_root" && pwd -P)"
        export SCUFRIS_TMUX_SOCKET="$scufris_runtime_root/workflows.tmux"
      fi
      export SCUFRIS_ROLE=orchestrator
      ${pkgs.lib.optionalString voice ''
        export SCUFRIS_VOICE_AVAILABLE=1
        export SCUFRIS_PIPER_MODEL=${pkgs.lib.escapeShellArg (toString piperModel)}
        export SCUFRIS_PIPER_CONFIG=${pkgs.lib.escapeShellArg (toString piperConfig)}
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
