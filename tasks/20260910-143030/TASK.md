# Add the compact briefing drawer

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: briefing, service, desktop, ios

## Purpose

Implement the agreed compact BRIEF drawer across the durable service protocol, desktop HUD, and iPhone surface. Keep successful correlated deliveries transient, keep failed or measured partial terminal deliveries sticky until durable per-run dismissal, and preserve the full bounded audit record.

## Decisions

- A run is partial only when its terminal collection state is `collected` and the measured `failed` source count is greater than zero.
- Presentation dismissal is separate from delivery acknowledgment. It is allowed only when collection is terminal and service delivery is `delivered`.
- Service persistence retains up to 128 audit rows. Surface replay and broadcasts contain only active or undismissed attention rows.

## Scope and evidence

Track implementation decisions, changed paths, representative rendered artifacts, focused and broad verification, platform gaps, and the final commit here. Do not land, push, release, deploy, activate services, or alter live state.

## Implementation

- Protocol 9 adds strict surface-only `briefing.dismiss { id }` and stable refusal codes.
- Briefing persistence format 2 stores dismissed generation IDs separately from the retained rows and terminal delivery queue. Format 1 loads with no dismissals.
- Surface lists now contain all active rows plus undismissed delivered failures and measured partials. Successful delivered rows and dismissed attention rows remain in the audit only.
- Audit eviction protects active and undismissed attention rows. The oldest delivered success or dismissed attention row yields first.
- Desktop and iPhone use a compact collapsed `BRIEF` drawer, deterministic ordering, explicit expansion, complete accessible state, and per-attention-row dismissal. Neither surface hides a row before the service publishes its next whole-list update.

## Rendered evidence

- `artifacts/briefing-drawer-wide.png`: 760 by 560 desktop HUD, collapsed with all active rows and the newest attention row.
- `artifacts/briefing-drawer-narrow.png`: 430 by 560 desktop HUD, expanded with failed, active, partial, and in-progress rows.
- The sibling HTML files are reproducible static fixtures using the production HUD stylesheet.

## Verification

- `nix develop --command cargo test --workspace`: passed after review fixes, with 25 shared-control, 336 desktop, 61 service, and 9 gateway tests.
- `env -u PI_PACKAGE_DIR npm run check`: passed, including 124 Node tests, type checking, version check (`surface protocol 9`), and repository formatting.
- Focused briefing desktop/service Node tests passed.
- `nix develop --command cargo check -p scufris-desktop`: passed.
- `nix flake check`: passed all 67 Linux flake checks without activation.
- iOS tests were updated for protocol encoding, measured attention, and collapsed/expanded ordering. No Swift or Xcode toolchain is installed in this Linux worktree, so they could not run here.
- The first plain `npm run check` inherited the worker harness `PI_PACKAGE_DIR` for Pi 0.85 while npm dependencies use Pi 0.84. It failed only because that external store lacks the old `dist/modes/interactive/theme/dark.json` path. Removing that harness-only override produced the passing full check above.

## Review

One separate read-only Pi review agent reviewed `ea2ca2d..2b99134`. It reported two medium findings. Both were accepted and fixed: eviction now explicitly excludes active rows, and desktop link refusals preserve operation/code so a briefing rejection reaches the HUD without settling a chat submission. See `REVIEW.md`. No second review pass was run.

## Commits

- `2b99134`: protocol, persistence, desktop and iPhone drawer, docs, tests, and rendered artifacts.
- `56caac1`: accepted review fixes and review record.
