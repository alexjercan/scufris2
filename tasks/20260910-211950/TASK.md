# Cut the v2.7.0 release

- STATUS: OPEN
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

- `017dcbe`. The full release Clippy gate found two warnings in the landed safeguards commit: a test-only `BriefingStore::delivery_failed` method was compiled into the shipped service, and the final match arm in `agent_request` had an unnecessary `return`. The method is now test-only and the needless return is removed. `nix develop -c cargo clippy --all-targets -- -D warnings` passes.
