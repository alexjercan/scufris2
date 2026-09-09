{pkgs}:
# The morning briefing on the command line. The agent drives the same program
# through its tools; this is the same thing for whoever is not the agent, and
# it is what the checks run.
let
  # markdown-it-py is the single CommonMark and GFM table implementation used
  # by collection, publish, and archived re-renders.
  python = pkgs.python3.withPackages (pythonPackages: [pythonPackages.markdown-it-py]);
  # The briefing asks the jobs helper which projects declare a briefing, so
  # that one reader stays the only answer to what a project is. Both live in
  # the store under their own names or the relative path between them breaks.
  source = pkgs.lib.fileset.toSource {
    root = ../tools;
    fileset = pkgs.lib.fileset.unions [
      ../tools/briefing
      ../tools/jobs
    ];
  };
in
  pkgs.writeShellApplication {
    name = "scufris-briefing";
    runtimeInputs = [python];
    text = ''
      exec ${python}/bin/python3 ${source}/briefing/cli.py "$@"
    '';
    meta = {
      description = "Collect and render the Scufris morning briefing";
      mainProgram = "scufris-briefing";
    };
  }
