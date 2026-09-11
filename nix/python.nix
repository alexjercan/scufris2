{pkgs}:
# The interpreter every Scufris helper script runs under.
#
# markdown-it-py is the single CommonMark and GFM table implementation used by
# collection, publish, and archived re-renders, so a plain `python3` cannot run
# `tools/briefing`. Every path that reaches that code has to name this one: the
# packaged `scufris-briefing`, the two launchers through `nix/agent-runtime.nix`,
# the staging run, and the development shell. Written once, because a path that
# forgets it fails at the first import and only when a briefing runs.
pkgs.python3.withPackages (pythonPackages: [pythonPackages.markdown-it-py])
