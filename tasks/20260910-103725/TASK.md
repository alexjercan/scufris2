# Spike durable scheduled briefing lifecycle and delivery

- STATUS: CLOSED
- PRIORITY: 0
- TAGS: spike, briefing, reliability

## Purpose

Settle and, only if safe, implement the first complete Scufris2 vertical slice for durable scheduled-briefing lifecycle visibility and terminal reporting. This task does not land, deploy, alter live state, or rerun the 2026-09-09 incident.

## Measured current failure modes

- The schedule unit runs `collect` and then best-effort `wake` in one cgroup and one `set -e` script. Collector death prevents finalization and wake.
- The 2026-09-09 nightly run stayed `collecting` after a Scufris2 red-team profile-reader probe followed a symlink to `/dev/zero`. The unbounded Python read exhausted memory. `OOMPolicy=stop` stopped the shared briefing cgroup, including the collector and both sources. Neither source had returned a complete contribution. The configured deadline had not expired.
- Source state is updated while collection runs, but valid contributions are written only by final `finish()`. A collector crash can discard a completed answer.
- `pending()` excludes `collecting`; Pi checks only today and yesterday at session start and does not periodically reconcile. A stale run can remain invisible forever.
- `scufris-ctl wake` is best effort. No connected agent means `agent_unavailable`. An accepted wake is only queued in the volatile agent connection, not durably processed.
- `publish()` changes collection state to `delivered` before a correlated response is known to be in canonical replay. Collection and delivery are conflated.
- Start and per-source progress have no durable surface protocol. Desktop and iPhone cannot show a live briefing row.
- The Python and TypeScript profile readers do not apply the required no-follow regular-file and size validation. Source `capture_output` is unbounded while the child runs.
- Duplicate delivery, reruns, multiple profiles, late source answers, open user turns, Pi/service restart, and crash windows do not have one explicit briefing ownership protocol.

## User experience

- A scheduled start becomes durable immediately.
- Desktop and iPhone show a compact briefing row with profile, collection state, and completed/total source counts. Start and progress are quiet: no model turn and no speech.
- Normal completion, partial completion, terminal failure, or lost ownership asks foreground Scufris to synthesize or escalate exactly once.
- A proactive briefing response does not attach itself to an open user turn. It enters canonical replay as an unprompted response and survives reconnect and reboot.
- Text names only durable measured facts: profile/date, completed and total source counts, and trusted failure cause. It does not infer source output or blame a source without source-level evidence.

## Ownership and delivery invariants

1. A run is owned by an opaque generation-fenced identity, not by a Pi session or surface.
2. A run writes state/artifacts before the event that announces them. A matching generation may move forward only. Late or duplicate writers cannot reopen or replace a terminal run.
3. Valid source output is bounded and persisted atomically before source completion is published.
4. Collection state and delivery state are separate. Writing briefing prose is not delivery acknowledgment.
5. Event transport is at least once. Stable event IDs plus durable acknowledgments make terminal model handling and surface presentation idempotent.
6. Filesystem watchers are latency hints. Startup, reconnect, and periodic reconciliation are authoritative.
7. A terminal event remains pending until the service has durably stored its correlated final response in canonical replay.
8. Normal lifecycle state never creates a model turn or speech. One terminal generation can create at most one proactive synthesis response.
9. An external owner finalizes a collector whose cgroup dies. The failure path does not depend on code in that cgroup.
10. Profiles and dates are labels, not identities. Concurrent profiles and reruns cannot acknowledge each other's state.
11. An open user turn keeps its own response association. Terminal briefing delivery waits for a safe proactive slot and uses the unprompted association.
12. All persisted and protocol data is private, bounded, validated, and safe to replay.

## Alternatives to settle

### A. Extend best-effort wake plus a Pi filesystem watcher

Small, but unsafe. It cannot guarantee delivery with no agent, during restart, after missed/coalesced notifications, or across the response/manifest crash window. Reject unless used only as a hint.

### B. Briefings as foreground-owned jobs

Reuses mature job status handling, but gives a schedule the wrong owner and authority. A briefing must exist without a foreground Pi session and must not become steerable model work. Reject. Reuse only event, cursor, generation, and dedup concepts.

### C. Durable run events plus a service-side proactive inbox and external finalizer

Persist generation-fenced collection state and ordered events. Let surfaces observe quiet lifecycle events. Queue terminal events durably in the service, deliver them when the agent is available and no user turn owns the slot, and acknowledge only after the correlated final response enters canonical replay. Add an external systemd `OnFailure` finalizer and bounded reconciliation. This is the preferred architecture if it can be implemented as one coherent vertical slice.

### D. Per-source transient systemd units/scopes

Improves failure isolation, but is not a delivery protocol and adds runtime/systemd orchestration complexity. Assess measured normal resource use and include only if required to promise source isolation safely. Otherwise make it an explicit follow-up while adding a conservative whole-run boundary and external finalizer now.

## Decision gate and scope

The spike must verify the service/agent/surface protocol can represent durable briefing state and correlated terminal ingress without changing job ownership or stealing an open user turn. If this cannot be done coherently in one change, stop after this design and record the unsafe boundary. If it can, implement the smallest complete vertical slice: memory-safe readers and bounded source output; immediate source persistence; generation-fenced state/events; stale/failure finalization outside the collector cgroup; durable service ingress, dedup, correlation, and reconciliation; quiet desktop/iPhone lifecycle rows; canonical terminal replay; tests and docs.

## Acceptance criteria

- Safe bounded readers reject symlinks, non-regular files, oversize files, and replacement races without opening special-file content.
- Source stdout/stderr is bounded while produced. Tests use finite regular-file fixtures only.
- Start, source completion/failure, and terminal state survive process/service restart and preserve event order.
- Repeated finalization, duplicate delivery, stale generations, duplicate profiles, reruns, and late answers are idempotent.
- Saved contributions survive collector death and are used for partial completion.
- A stale `collecting` run with no live matching owner reaches a measured terminal state. An external `OnFailure` finalizer records systemd result, including OOM, without depending on the failed cgroup.
- Desktop and iPhone show accessible compact state at narrow widths and reconstruct it after reconnect/replay. Start/progress do not create conversation messages or speech.
- No-agent and disconnect cases queue terminal ingress durably. Reconnect/restart retries it. A correlated response is acknowledged only after canonical replay storage.
- An open user turn is not captured by a briefing. Late or duplicate answers do not acknowledge the wrong event.
- User-visible text includes only measured causes and source counts.
- Focused Python, TypeScript, Rust, desktop, iPhone, protocol, replay, systemd/Nix, formatting, and broad checks pass as applicable. Generated or rendered output is inspected.
- Task, design, implementation, tests, docs, and Unreleased changelog are committed in this Sprout. Nothing is landed or deployed.

## Evidence

### Decision and implementation

- Chose alternative C. Protocol v8 now carries quiet briefing rows, durable control ingress, and proactive response correlation. Collection and delivery are independent.
- `tools/briefing/briefing.py` uses version-2 generation-fenced manifests, ordered events, immediate atomic contribution writes, bounded descriptor reads, bounded child pipes, per-run exclusion, legacy migration, and ownerless finalization.
- `host/service/src/briefings.rs` stores rows and terminal wakes atomically. `host/service/src/conversation.rs` records one delivery ID at the canonical replay boundary. Startup repairs the replay/inbox crash window before agent registration.
- `nix/home-manager.nix` installs per-generation external `OnFailure` finalizers and an external one-minute reconciliation timer. `nix/checks/briefing.nix` inspects the generated generation, finalizer, and reconciliation units.
- Desktop and iPhone surfaces render replayable, noninteractive, accessible briefing rows. The agent extension performs a quiet startup reconciliation and carries only one durable proactive correlation into the next atomic response.
- Developer lifecycle documentation and `[Unreleased]` changelog entries describe ownership, recovery, protocol update, operations, and failure behavior.

### Validation

- After synchronization with current master, `python3 -m unittest tests.test_briefing tests.test_briefing_cli tests.test_scufris_jobs`: 181 passed. Cases include special-file and symlink refusal, output overflow, immediate persistence, dead-owner finalization, generation fencing, legacy delivery, duplicate profiles, and reconciliation.
- `env -u PI_PACKAGE_DIR npm test`: 123 passed. `npm run typecheck` passed. Focused briefing, service, and desktop suites passed 79 tests after the final TypeScript changes.
- `cargo test -p scufris-control -p scufris-service --no-fail-fast` in `nix develop`: 24 control, 55 service, and 9 gateway tests passed. `cargo check --workspace` and `cargo fmt --all -- --check` passed.
- Final synchronized `nix flake check --keep-going`: all 73 Linux checks passed, including helper, package, docs, native format, Home Manager, generated Nix-store link, briefing timer/finalizer/reconciler, memory bounds, service, and desktop checks. Nix reported Darwin and aarch64 as incompatible systems, so iPhone/Xcode tests were not available on this Linux worker.
- A headless Chromium review rendered the desktop rows at 360 by 640 pixels with collecting, failed, delivered, long-profile, and long-summary cases. Counts remained visible, text did not overlap, summaries wrapped to their narrow layout row, and full accessible labels remained in the DOM. iPhone source review found and fixed a clipped four-line fallback; the final two-line layout reserves fixed state/count space and a 42-point row budget.
- After synchronization, the complete `env -u PI_PACKAGE_DIR npm run check` passes version checks, typecheck, all 123 Node tests, and repository-wide Prettier. `git diff --check` also passes.

### Boundaries

- No deployment, service restart, live-state mutation, incident rerun, landing, or per-source systemd isolation was performed. Per-source transient scopes remain an optional later hardening layer; this slice bounds source output and the whole run, and finalizes cgroup failure externally.
