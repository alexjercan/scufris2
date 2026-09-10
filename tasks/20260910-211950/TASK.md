# Cut the v2.7.0 release

- STATUS: CLOSED
- PRIORITY: 0
- TAGS: release

## Purpose

Cut the stable Scufris release that contains the landed briefing-loop safeguards on current master.

## Scope and decision

- `v2.6.0..master` contains the v2.6.0 task close and `3dbe769`, the briefing-loop safeguards.
- The safeguards move `SERVICE_VERSION` from 9 to 10 across the Rust, TypeScript, and Swift surfaces. The protocol is strict and coordinated, so project convention requires a minor release: `v2.7.0`.
- The existing `CHANGELOG.md` `Unreleased` section covers the protocol change, circuit breaker, event correlation, fixture isolation, idempotent publication and delivery, and structured evidence.

## Release plan

Follow `RELEASE.md`: prepare coordinated product versions and changelog, run every local release check, land the release commit on master, push master and the annotated tag in that order, verify all triggered GitHub workflows and the source-only release, and dispatch and verify TestFlight because `SERVICE_VERSION` changed.

## Evidence

### Scope review

- Local master was clean at `3dbe769`; a fresh fetch measured `origin/master` at `fa46300`, with only the landed safeguards commit local and pending push.
- `git log v2.6.0..master` contains the v2.6.0 task close and the safeguards commit. The latter changes the host, agent, desktop, gateway, control client, and iPhone together and moves strict surface protocol 9 to 10.
- The coordinated protocol bump and user-facing failure state follow the project's recent minor-release convention, so the next version is 2.7.0.
- No local or remote `v2.7.0` tag existed when preparation started.

### Fix committed before preparation

- `017dcbe`. The full release Clippy gate found two warnings in the landed safeguards commit: a test-only `BriefingStore::delivery_failed` method was compiled into the shipped service, and the final match arm in `agent_request` had an unnecessary `return`. The method is now test-only and the needless return is removed.
- `e902b65`. The Ruff release gate found one unformatted assertion and a recovery test closure that captured two loop variables. The test now binds those values explicitly and combines its context managers. The focused recovery and fixture-isolation tests pass.

### Verification

- `npm ci`: installed the locked dependencies; audit found zero vulnerabilities.
- `nix develop -c npm run check` with Pi and voice development variables unset: product 2.7.0 and protocol 10 are consistent; strict TypeScript, 125 Node tests, and Prettier pass.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 394 tests pass.
- `ruff check .`: clean. `ruff format --check .`: 260 files formatted.
- `shellcheck scripts/scufris-agent scripts/scufris-dev scripts/scufris-staging`: clean.
- `cargo clippy --all-targets -- -D warnings`: clean. `cargo test`: 26 control, 336 desktop, 72 service, and 9 gateway tests pass.
- `nix fmt -- --check .`: 27 files comply.
- `nix flake check -L`: all compatible-system checks pass. Nix omitted the incompatible `aarch64-darwin` and `aarch64-linux` systems.
- `git diff --check`: clean.
- The master `iOS` workflow ran the native Xcode build and Swift tests successfully on macOS.

### Released

- Release commit: `424e1781f912bf4bfe94aa8d7aa2ee7d193b3fdd` (`Scufris 2.7.0`).
- Annotated tag: `v2.7.0`; tag object `2b509c8ce31adb61d18498fa05872afc3fc24c38`, peeled to the release commit. Master was pushed first, then only the new tag.
- Release run 34515039030 passed its reusable repository checks, tag/version verification, and publication job: https://github.com/alexjercan/scufris2/actions/runs/34515039030
- Master check 34515023949, Documentation 34515024011, and iOS 34515024131 all passed on the release commit.
- The stable source-only GitHub Release has no assets: https://github.com/alexjercan/scufris2/releases/tag/v2.7.0
- Protocol 10 required the iOS surface to move with the host. TestFlight run 34516582386 completed its signed archive and App Store Connect upload successfully: https://github.com/alexjercan/scufris2/actions/runs/34516582386
- Deployment was not part of this release request and was not performed.
