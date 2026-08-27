{
  pkgs,
  piperPackage,
  piperModel,
  piperConfig,
}: let
  inherit (pkgs) lib;
in
  pkgs.writeShellApplication {
    name = "scufris-speak";
    # Piper and PipeWire are the helper's own tools, not the agent's. The
    # speaker belongs to whoever is sitting in front of the machine, so what
    # runs it is the companion's process tree and this is where the paths are
    # bound.
    runtimeInputs = [pkgs.python3 piperPackage pkgs.pipewire];
    text = ''
      export SCUFRIS_PIPER_MODEL=${lib.escapeShellArg (toString piperModel)}
      export SCUFRIS_PIPER_CONFIG=${lib.escapeShellArg (toString piperConfig)}
      exec python3 ${../tools/voice/scufris-speak} "$@"
    '';
    meta = {
      description = "Speaks one paragraph read from standard input with the pinned Piper voice";
      mainProgram = "scufris-speak";
    };
  }
