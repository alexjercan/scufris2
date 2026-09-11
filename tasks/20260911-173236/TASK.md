# Cut the v2.8.0 release

- STATUS: CLOSED
- PRIORITY: 0
- TAGS: release

## Purpose

Release current master with the landed terminal handoff through the documented stable release, GitHub publication, and coordinated TestFlight path.

## Scope and semantic-version decision

- `v2.7.0..master` contains the prior release-task close, the scheduled briefing PATH fix, corrected landing receipts, the resolved nightly briefing review, restarted-job visibility, and the terminal handoff.
- The terminal handoff adds a new user-facing terminal surface and moves strict `SERVICE_VERSION` from 10 to 11 across host, agent, desktop, gateway, control client, and iPhone. Recent coordinated protocol bumps have each taken the next minor version. The required version is therefore `2.8.0`, not a patch and not an incompatible public API major bump.
- The current `Unreleased` section covers terminal handoff and landing receipts. Release preparation must also describe the shipped briefing recovery, scheduled den PATH, visible widget-capacity refusal, and generation-scoped job filing fixes.
- `SERVICE_VERSION` changed, so `RELEASE.md` requires the iOS/TestFlight workflow after the tag release.
- The explicit release request prohibits any edit, rebuild, switch, or update of `personal/nix.dotfiles` or the installed Home Manager generation. Source publication and TestFlight proceed; local deployment step 11 is intentionally not performed.

## Plan

Follow `RELEASE.md` in order: complete the changelog and coordinated versions, run every prescribed local check in the required environment, sync and land the release Sprout, push `master` and then only `v2.8.0`, verify every triggered workflow and the source-only GitHub Release, dispatch and verify TestFlight, record exact evidence here, close this task, and remove the release Sprout when its recorded work is landed.

## Evidence

### Scope review

- Release Sprout started clean at `ee84f869a7c604695e689c243feb9b38d22eda7d`, exactly matching local `master`; fetched `origin/master` was `a81ad7323ddc2902edd1f9b7603a17848fcdc35e`, so the landed terminal handoff was the one clean local commit pending publication.
- No local or remote `v2.8.0` tag and no GitHub `v2.8.0` release existed before preparation.
- All newly completed task records and user-facing documentation changed since `v2.7.0` were reviewed. The terminal implementation task was the only completed landed task still marked open; release housekeeping closes it before version preparation.

### Version preparation

- `npm version 2.8.0 --no-git-tag-version` updated `package.json` and both root values in `package-lock.json`.
- Workspace `Cargo.toml`, the three workspace package entries in `Cargo.lock`, and iOS `MARKETING_VERSION` are 2.8.0. `nix develop -c cargo check --workspace` refreshed and checked the lock because the host profile does not expose Cargo.
- `CHANGELOG.md` moves the complete `Unreleased` scope under `2.8.0` dated 2026-09-11 and advances both comparison links.
- `python3 tools/release/check_versions.py --tag v2.8.0` reports `product 2.8.0; surface protocol 11`.
- Sprout preparation commits are `b819d524e5ee72b6b0947c860d549256c79483c5` and `b55194d567a1d952571524e5c7910dd04edd9a6e`. Neither commit has an AI, co-author, or session trailer. `sprout sync release-terminal-handoff` was already current before landing, so the verified release tip did not change.

### Local verification

- `npm ci`: 235 locked packages installed, 236 audited, 0 vulnerabilities.
- `nix develop -c npm run check` with the worker-only Pi override and all Scufris speech/TTS variables unset: product 2.8.0 and protocol 11 agree; strict TypeScript, all 157 Node tests, and Prettier pass.
- The voice-affected normal-environment repeat also passes all 157 Node tests. The normal environment had no Scufris speech/TTS override; only the worker-only `PI_PACKAGE_DIR` was removed so tests used the locked Pi package.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 411 tests pass.
- `ruff check .`: clean. `ruff format --check .`: 280 files formatted. Ruff ran in `nix develop` because the host profile does not expose it.
- `shellcheck scripts/scufris-agent scripts/scufris-dev scripts/scufris-staging`: clean in `nix develop`, where ShellCheck is available.
- `cargo clippy --all-targets -- -D warnings`: clean. The chained `cargo test` passes 475 tests: 29 control, 340 desktop, 2 control-client, 92 service, 9 gateway, and 3 lease integration tests. Both ran in `nix develop`, where Cargo is available.
- `nix fmt -- --check .`: all 28 Nix files comply.
- `nix flake check -L`: all compatible checks pass, including 411 helper tests with 4 expected skips. Nix reports only the incompatible `aarch64-darwin` and `aarch64-linux` systems as omitted.
- `git diff --check`: clean. The exact tested Sprout was clean before landing.

### Local, remote, and tag evidence

- `sprout land release-terminal-handoff -m 'Scufris 2.8.0'` created clean master release commit `f6a637997eb9bda1465c7fdf4640a639e1855794`. Its commit message contains no trailers.
- Annotated tag `v2.8.0` has immutable tag object `4d881288dff678510626480a8ccbf81aee0fba46`, message `Scufris v2.8.0`, and peels to the release commit.
- `master` was pushed first. The remote master became `f6a637997eb9bda1465c7fdf4640a639e1855794` while remote `v2.8.0` was still absent. Only then was `refs/tags/v2.8.0` pushed. Remote refs now match the local master, tag object, and peeled commit exactly.

### CI and GitHub Release evidence

All workflows started by the release push passed on exact head SHA `f6a637997eb9bda1465c7fdf4640a639e1855794`:

- Master check run `34611855634`: <https://github.com/alexjercan/scufris2/actions/runs/34611855634>
- Documentation build and Pages deployment run `34611855632`: <https://github.com/alexjercan/scufris2/actions/runs/34611855632>
- Native unsigned iOS simulator build and Swift test run `34611855694`: <https://github.com/alexjercan/scufris2/actions/runs/34611855694>
- Tag release run `34611872863`: <https://github.com/alexjercan/scufris2/actions/runs/34611872863>. Its reusable repository checks, tag/package verification, and source publication job all passed.

GitHub Release `387126075` is published, stable, and not a draft or prerelease: <https://github.com/alexjercan/scufris2/releases/tag/v2.8.0>. It is named `Scufris v2.8.0`, has generated comparison notes for `v2.7.0...v2.8.0`, and has zero uploaded assets as the source-only policy requires. Both generated source artifacts resolve and contain root package version 2.8.0, with 765 entries each:

- tar.gz: 3,388,988 bytes, SHA-256 `6e07f8a804f41da7312590c4b9ece248814a1221a68cab4b864ee7fbe4c6c583`.
- zip: 3,722,248 bytes, SHA-256 `b94eb266232796d87f815174d2e6ea9041efb5a2a1a40a471c01c44e1a6d8047`.

### TestFlight evidence

- Protocol 11 required the coordinated iOS path. `gh workflow run testflight.yml --ref v2.8.0` created run `34613651681` on the exact tag commit: <https://github.com/alexjercan/scufris2/actions/runs/34613651681>.
- Run number 20 makes this iOS version 2.8.0 build 20. The protected-environment job installed signing material, created the signed archive, exported `Scufris.ipa`, and uploaded it to App Store Connect. Every step passed.
- Apple's uploader reported `UPLOAD SUCCEEDED with no errors`, transferred 893,458 bytes, and returned delivery UUID `7e393d49-75dd-442d-954f-d964e309037a`. Signing material cleanup also passed.
- `RELEASE.md` step 10 and `surfaces/ios/README.md` define completion as the signed App Store Connect upload. They do not require waiting for Apple's later TestFlight processing or installation. No local App Store Connect client or credentials are exposed outside the protected GitHub environment, so this record makes no unsupported claim that post-upload processing or phone installation occurred.

### Deployment and cleanup

- Deployment step 11 was intentionally not run because this release request explicitly forbids edits, rebuilds, switches, or other updates to `personal/nix.dotfiles` and the installed Home Manager generation. Neither was modified.
- After this evidence commit lands and is pushed, the release Sprout can be removed because all of its content will be on `master`.
