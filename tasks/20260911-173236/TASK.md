# Cut the v2.8.0 release

- STATUS: OPEN
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
