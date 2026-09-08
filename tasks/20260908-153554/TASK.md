# Package scufris-jobs so a briefing source can call it by name

- STATUS: CLOSED
- PRIORITY: 75
- TAGS: workflow, packaging

## Goal

A briefing source can call `scufris-jobs` by name. Until it can, the machine
source that reports what Scufris did has to name a checkout path, which works
on one machine and nowhere else.

Origin: found while building task 20260908-145550. The listing that task added
is reachable from a terminal and not from the thing it was added for.

## Facts

- `scripts/scufris-jobs` is a resource file, not an installed program. Nothing
  puts it on `PATH`.
- The briefing unit's `PATH` is `scufris-briefing`, `scufris-ctl`, `pi`,
  `git`, and `python3`, then the user profile and the system path
  (`nix/briefing-unit.nix:23,32`).
- The launcher's `runtimeInputs` are `python3`, `tmux`, `scufris-den`, and
  `scufris-briefing` (`nix/launcher.nix:31-38`).
- The precedent is already there. A helper a model calls by name is packaged:
  `scufris-den` for the den skill and `scufris-briefing` for the briefing
  extension are both flake packages (`flake.nix:113-114`).

## Direction

- Package `scripts/scufris-jobs` as `scufris-jobs`, the way `scufris-den` and
  `scufris-briefing` are packaged.
- Put it on the briefing unit's `PATH`, so a machine source's guidance names a
  program rather than a path.
- The extension keeps calling `tools/jobs/scufris-jobs` by resource path.
  Nothing about the helper's own interface changes; this packages the
  user-facing script that reads it.

## Verification

- Test: a machine source whose guidance runs `scufris-jobs history` gets an
  answer, with no checkout path anywhere in its guidance.
- `nix flake check` and Python unit tests.
