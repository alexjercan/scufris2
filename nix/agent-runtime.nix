{
  pkgs,
  den,
  briefing,
}:
# Every program the Scufris agent runs, whichever launcher started it.
#
# The composition is the same in a deployment and in a checkout, so the two
# launchers put the same programs on PATH: `tools/briefing` and
# `tools/jobs/scufris-jobs` are scripts with a `python3` shebang, the job
# helper drives tmux, and the den skill and the briefing sources call their
# programs by name. Written once, because a launcher that leaves one of them
# out builds, installs, and then fails at the first briefing and nowhere
# earlier.
[
  # The interpreter the helpers import from, not a plain `python3`.
  (import ./python.nix {inherit pkgs;})
  pkgs.tmux
  # The journal, which the den skill runs by name.
  den
  # The morning briefing, which a source runs by name.
  briefing
]
