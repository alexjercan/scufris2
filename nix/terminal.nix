{
  pkgs,
  piPackage,
  ctl,
}:
# Normal interactive Pi, started as the holder of the Scufris conversation.
# The launcher is one script in the store rather than a copy of its text here,
# so the working tree and the deployment answer the same way.
let
  source = pkgs.lib.fileset.toSource {
    root = ../.;
    fileset = pkgs.lib.fileset.unions [
      ../scripts/scufris-terminal
    ];
  };
in
  pkgs.writeShellApplication {
    name = "scufris-terminal";
    # `scufris-ctl` says where the sessions are, and Pi is what this becomes.
    # A `pi` already on PATH wins, the way the agent launcher lets it win.
    runtimeInputs = [ctl piPackage];
    text = ''
      exec bash ${source}/scripts/scufris-terminal "$@"
    '';
    meta = {
      description = "Hold the Scufris conversation in a terminal";
      mainProgram = "scufris-terminal";
    };
  }
