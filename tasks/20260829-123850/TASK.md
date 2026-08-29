# Release and prepare deployment of Scufris v0.5.0

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: release, deployment

## Goal

Publish the reviewed `master` as the immutable Scufris `v0.5.0` source release and prepare the canonical dotfiles deployment to consume it.

## Release decision

- Use `v0.5.0`. The release adds the background service, native widgets, the conversation window, staging, journal panels, and desktop controls.
- This is a minor pre-1.0 release because it also replaces control protocol v2, removes the popup and Dashboardd interfaces, and requires the service for the desktop companion.
- Publish a source-only GitHub Release through the tag workflow.
- Do not alter the dirty `nix.dotfiles` worktree. It contains an unrelated user-authored Tailscale and SSH change. Prepare deployment by recording the required input update and checks; apply it after that worktree is clean.

## Scope

- Repair the formatting failure on current `master` without changing the content of the user-authored spike task.
- Update JavaScript and Rust package versions to `0.5.0`.
- Move the Unreleased changelog into the dated `0.5.0` release and update release links.
- Update installation examples and the release checklist.
- Run every applicable check in `RELEASE.md`.
- Commit and push the release preparation, tag `v0.5.0`, and verify publication.
- Record the canonical deployment change and verification commands.

## Deployment plan

When `/home/alex/personal/nix.dotfiles` is clean, update `inputs.scufris.url` in `flake.nix` from `github:alexjercan/scufris2/v0.4.0` to `github:alexjercan/scufris2/v0.5.0`, update `flake.lock`, then run:

```bash
nix flake check -L
nix build .#homeConfigurations.alex.activationPackage --no-link
git diff --check
```

Review the evaluated service, desktop, voice, and journal widget configuration before activation. The v0.5.0 module removes popup options and requires `programs.scufris.service.enable` when the desktop is enabled.

## Verification evidence

Preparation:

- Base revision: `eef22fb`, synchronized with `origin/master`.
- `npm version 0.5.0 --no-git-tag-version` updated the JavaScript package and lockfile. The Rust workspace and lockfile now use `0.5.0` too.
- The changelog entries since `v0.4.0` are dated as `0.5.0`; installation examples and comparison links name `v0.5.0`.
- Prettier repaired the formatting failure reported by GitHub Actions in `tasks/20260828-232631/TASK.md`. It changed Markdown list spacing only.
- The release checklist's obsolete `desktop` Rust path now names `native`.
- Clippy found a test-only `tray_state` helper in production builds. `#[cfg(test)]` now limits it to the tests that use it.
- Ruff found two lint defects in the journal backend and old formatting in its test. The backend now reads a timezone-aware local date and passes `check=False` explicitly without changing subprocess result handling. The focused 39-test suite passed after the correction.

Checks:

- `npm ci`: passed, 228 packages audited with no vulnerabilities.
- Ordinary `npm run check`, with inherited Pi package and Scufris voice variables unset: passed typecheck, all 92 TypeScript tests, and Prettier.
- `nix develop -c npm run check`, with the normal voice environment and only the harness-specific `PI_PACKAGE_DIR` unset: passed the same checks.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: passed all 91 tests.
- `ruff check .`: passed.
- `ruff format --check .`: passed for 178 files.
- `shellcheck scripts/scufris-dev`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed for the native workspace.
- `cargo test`: passed 26 control, 314 desktop, and 49 service tests plus doc tests.
- `nix fmt -- --check .`: passed for 23 files.
- `nix flake check -L`: passed on `x86_64-linux`, including native package tests, helper tests, closure checks, and Home Manager checks. Nix omitted configured incompatible systems.
- `git diff --check`: passed.

The first ordinary JavaScript run inherited Pi's Nix package location and failed because that installed package has no theme JSON in its source layout. Unsetting `PI_PACKAGE_DIR` reproduced the ordinary CI environment and passed. Initial Ruff checks found and led to the bounded backend corrections above.

Publication:

- Release commit `eec186c` was pushed to `master`.
- Annotated tag `v0.5.0` points to `eec186c` and was pushed without replacement.
- GitHub Actions release run `33246286165` passed the reusable repository checks, verified the package version, and created the release.
- [Scufris v0.5.0](https://github.com/alexjercan/scufris2/releases/tag/v0.5.0) is published as a stable, source-only release with no assets.
- The separate `master` check and documentation workflows passed at the release revision.
- Deployment remains prepared but unapplied. The canonical dotfiles still pin `v0.4.0`; its worktree contains active user-authored NixOS restructuring, so this task did not mix the release input update into it.
