# Retro: Move voice runtime and popup ownership into Scufris

- TASK: 20260822-003928
- BRANCH: scufris-voice-feature

## What went well

- The landed cross-project voice task supplied exact hashes, Piper defects, popup identity lessons, and real fixture expectations.
- Separate resource derivations made disabled closure evidence direct. The normal output contains neither speech code nor speech runtime.
- Home Manager evaluation covered the primary Pi `finalPackage` composition without activating a user generation.
- A real Piper fixture reused the production helper and caught stream regressions beyond fake process tests.
- The accepted scope update synchronized cleanly through a temporary stash, and exact development tests covered the new role and runtime boundary.
- Extracting check construction reduced `flake.nix` from about 490 lines to 172 without changing check coverage.

## What went wrong

- The first npm check ran before this worktree had dependencies and could not find `tsc`.
- The first Nix real fixture executed the helper directly. Nix build sandboxes have no `/usr/bin/env`, so the unchanged portable shebang could not resolve Python there.
- Read-only Home Manager options cannot combine a default value with a later module assignment. The first popup launcher interface used both and failed evaluation.
- Home Manager normalizes systemd `ExecStart` to a one-item list. The first interface assertion compared it with a string.
- npm prepends the repository Node binary directory to scripts. A naive `type -P pi` would have selected the development dependency instead of system Pi.
- Initial review found that the intended cross-repository outputs were marked internal, README omitted required user guidance, development did not set project-root defaults or resume, and check fixtures dominated the flake.

## What to improve next time

- Install JavaScript dependencies before the first repository check in a new Sprout worktree.
- Invoke shebang-based Python helpers through the explicit Nix Python interpreter in sandbox fixtures.
- Model computed read-only Home Manager values as one assignment with no default.
- Inspect normalized Home Manager service values before writing exact evaluation assertions.
- Test executable provenance when package managers rewrite `PATH`; do not treat the first same-named command as the system tool.
- Review public option visibility, documentation acceptance criteria, and top-level file size before the first review-ready revision.
