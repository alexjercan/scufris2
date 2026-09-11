# Put scufris-den on scheduled briefing PATH

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, briefing, nix

## Goal

A scheduled briefing source can invoke `scufris-den` by name, with no login-profile dependency.

## Cause confirmed

- `nix/home-manager.nix` passes briefing, jobs, ctl, and Pi into `briefing-unit.nix`, but not `defaults.denPackage`.
- `nix/briefing-unit.nix` builds the scheduled runner PATH from only those packages plus coreutils, Git, and Python.
- The deployed morning unit starts `/nix/store/mr9wwg7f1fax5p67qs45qppzndblz2gz-scufris-briefing-morning/bin/scufris-briefing-morning`.
- That wrapper's exported PATH has scufris-briefing, scufris-jobs, scufris-ctl, Pi, coreutils, Git, and Python, but no scufris-den store path.
- `~/.nix-profile/bin/scufris-den` is absent. The foreground can find `/nix/store/9y44lj8x0axfxkiv7bx9ya0gcqjqwwk4-scufris-den/bin/scufris-den` only because the interactive launcher includes the den package.

## Plan

Pass `defaults.denPackage` to the scheduled runner, include it in `briefing-unit.nix`'s explicit PATH, and assert the rendered scheduled runner contains `scufris-den`. Run the focused Nix check, project checks, and `nix flake check`. Do not land or release.

## Implementation

- `nix/home-manager.nix` passes `defaults.denPackage` into each scheduled briefing runner.
- `nix/briefing-unit.nix` includes that package in the explicit `lib.makeBinPath` list.
- `nix/checks/briefing.nix` checks that the rendered scheduled runner's `export PATH` contains the exact `${scufris.den}/bin` package path. It also tightens the existing jobs assertion to its exact package path.

## Verification

- `nix fmt -- nix/home-manager.nix nix/briefing-unit.nix nix/checks/briefing.nix`: passed.
- `nix build .#checks.x86_64-linux.briefing-timers -L --no-link`: passed. Its rendered PATH contains `/nix/store/...-scufris-den/bin`.
- `npm ci`: installed the locked development dependencies with no vulnerabilities.
- `nix develop -c env -u PI_PACKAGE_DIR npm run check`: passed the version check, TypeScript, 125 Node tests, and Prettier.
- `nix flake check -L`: passed all checks, including 394 Python helper tests with 3 skipped.

The first direct JavaScript check attempts did not have the lockfile dependencies, then inherited the delegated worker's `PI_PACKAGE_DIR` override. That override pointed the locked Pi 0.84.2 test package at unrelated installed Pi 0.85 resources. Installing with `npm ci` and unsetting that harness-only override reproduced the repository CI environment and passed. No product source change was needed for this environment issue.

## Approved landing follow-up

The user approved landing after syncing this Sprout with current `master` and re-running the focused regression and affected required checks.

- `sprout sync briefing-den-path` merged `master` at `4275fa6` into the feature without conflicts as merge `50f0003`.
- The incoming master commit adds only `tasks/20260910-230024/TASK.md`; it does not change the fix paths or intent.
- `nix build .#checks.x86_64-linux.briefing-timers -L --no-link`: passed after sync.
- `nix develop -c env -u PI_PACKAGE_DIR npm run check`: passed after sync, including all 125 Node tests.
- `nix flake check -L`: passed after sync, including all 394 Python helper tests with 3 skipped.

The synced Sprout is cleanly integrated and verified for the approved landing.
