{pkgs}:
# Stored jobs on the command line, read and never changed. The orchestrator
# drives the helper through its tools; this is the same records for whoever is
# not the orchestrator, and it is what a briefing source reads.
let
  # The script runs the helper beside it, by the relative path between the two.
  # Both go into the store under their own names or that path breaks.
  source = pkgs.lib.fileset.toSource {
    root = ../.;
    fileset = pkgs.lib.fileset.unions [
      ../scripts/scufris-jobs
      ../tools/jobs
    ];
  };
in
  pkgs.writeShellApplication {
    name = "scufris-jobs";
    runtimeInputs = [pkgs.python3];
    text = ''
      exec python3 ${source}/scripts/scufris-jobs "$@"
    '';
    meta = {
      description = "Read stored Scufris jobs, their receipts, and their history";
      mainProgram = "scufris-jobs";
    };
  }
