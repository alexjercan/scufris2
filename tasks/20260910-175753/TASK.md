# Add briefing-loop safeguards

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, briefing, reliability

## Purpose

Implement the safeguards recommended by the 2026-09-10 repeated-briefing
incident investigation. Preserve proactive event identity through Pi, bound
consecutive proactive model turns with service-owned backoff, make briefing
publication idempotent, improve queue and delivery evidence, and add reliable
production-shaped regression coverage.

## Constraints

- Work in the `briefing-loop-safeguards` Sprout from master `fa46300`.
- Do not land, deploy, restart services, or alter live Scufris state.
- Keep collection, delivery, and user-owned turns separate.
- Prefer focused real service/store/extension tests over an unreliable full
  provider integration when the repository cannot control Pi internals.

## Incident basis

The prior investigation proved at least 38 foreground starts from 16:30:49 to
16:41:07. It could not recover the exact renewal source because cleanup removed
the incident store, manifests, canonical conversation, and Pi session. It did
prove a separate defect: `AgentClient` stores one mutable proactive ID and the
service extension drops that ID from the Pi custom message, so a response can
acknowledge another queued follow-up.

## Decisions and verification

### Decisions

- Correlation is attached to the exact Pi custom message in its `details`, not
  held on the agent socket client. Original wake details remain at their
  existing keys. The extension reports exact `proactive_started` and
  `proactive_settled` turn boundaries over the ordered agent socket and adds
  the event ID only to that turn's first atomic response.
- The host counts proactive turn starts, applies exponential backoff from 2
  seconds with a 30-second cap, and opens a circuit after three consecutive
  starts. A user turn or a queue that stays empty through backoff resets the
  count. The circuit marks every queued row failed and needs a service restart.
- Proactive publication is visible only after the canonical response and inbox
  acknowledgement are both durable. A canonical write that precedes a failed
  inbox acknowledgement is recovered by the exact settlement marker or at
  startup. Store mutations roll back on persistence failure.
- Filesystem publication uses a generation-local lock. The first prose wins.
  An identical retry is a no-op or repairs an interrupted manifest/page write;
  different prose for the same generation is refused.
- Protocol 10 adds exact proactive turn boundary messages and the failed
  briefing-delivery state. Desktop and iPhone show it as nondismissible
  attention with restart guidance. The service and tray report failed state.
- Briefing ingress, dispatch, turn start, retry, circuit, acknowledgement, and
  startup recovery logs include structured outcome and pending counts. Event
  paths also include run and event IDs.

### Changed areas

- `agent/extensions/scufris/service/`: event-bound correlation, ordered turn
  boundary messages, protocol 10.
- `host/service/src/` and `shared/control/src/service.rs`: durable delivery
  transactions, queue outcomes, backoff/circuit state, failed delivery, and
  structured tracing.
- `tools/briefing/briefing.py`: locked idempotent publication and recovery
  outcomes.
- `surfaces/desktop/` and `surfaces/ios/`: protocol 10 and failed-delivery
  attention presentation.
- `tests/`, `docs/src/dev/`, and `CHANGELOG.md`: regression coverage and durable
  documentation.

### Verification

- `env -u PI_PACKAGE_DIR npm run check`: passed; 125 Node tests, TypeScript,
  version consistency, and Prettier.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: passed; 393 tests.
- `nix develop -c cargo test --workspace`: passed; 443 Rust tests (26 control,
  336 desktop, 72 service, and 9 gateway).
- `nix flake check`: passed all 67 compatible-system checks. It omitted the
  incompatible `aarch64-darwin` and `aarch64-linux` systems.
- `git diff --check` and `cargo fmt --all -- --check`: passed.
- Native Swift was not available on this Linux host. Protocol/UI Swift sources
  were updated with focused protocol tests, but `swift test` could not run.
- The first unmodified `npm run check` inherited Pi's `PI_PACKAGE_DIR`, whose
  installed package omits `dist/modes/interactive/theme/dark.json`. That made
  only `tests/calm.test.ts` fail. Unsetting the harness-only variable produced
  the clean required run above.

No branch landing, deployment, service restart, or live-state change was
performed.

## Recurrent-loop follow-up

The foreground loop recurred while this task's checks ran. Read-only live-state
inspection found a concrete producer that the original incident no longer had
enough evidence to name:

- Python briefing fixtures isolated their state and project roots but did not
  isolate `scufris-ctl`. Each synthetic collection announcement resolved
  `/home/alex/.nix-profile/bin/scufris-ctl` from `PATH` and entered the live
  protocol-9 service. Temporary fixture manifests were then removed normally,
  so the foreground correctly found no run and refused publication.
- Six test invocations from this worker produced 145 unique generations and
  exactly 145 correlated Pi turns from 18:16:56 through 19:02:18 local time.
  The full suite produced two identical 66-wake shapes. The dates and profiles
  were the fixtures' 2026-08-31 morning/evening, 2026-09-01 morning, and
  default-date 2026-09-10 morning/nightly cases.
- All 145 Pi event IDs match canonical conversation delivery IDs in exact
  order. Thus the old mutable proactive ID happened to acknowledge correctly
  in this recurrence because no other proactive event interleaved. It was not
  the producer.
- The live store retained its 128-row cap: 125 wake rows and three zero-source
  rows, with the oldest 20 wake rows evicted. All retained wakes are delivered
  and its pending queue is empty. Reconciliation reported `sent: 0` and was not
  the producer.

Exact IDs, timestamps, source counts, canonical sequences, responses, store
retention, source hashes, and producing command IDs are preserved in
`recurrent-loop-events.json`. Analysis is in
`recurrent-loop-investigation.md`.

Commit `593a045` would have stopped execution after three starts, but it needed
two corrections. The fixtures now pin an explicit temporary control client,
and ingress arriving after an open circuit is immediately marked failed rather
than left pending for another burst after restart.

Follow-up verification:

- Three focused Python fixture tests passed and left the live briefing-store
  SHA-256 unchanged.
- `consecutive_proactive_turns_back_off_and_open_the_circuit` passed with the
  new post-circuit ingress case.
- The full Python suite passed all 394 tests. The live briefing-store SHA-256
  and nanosecond mtime were unchanged before and after that run.
- `env -u PI_PACKAGE_DIR npm run check` passed: protocol/version consistency,
  strict TypeScript, 125 Node tests, and Prettier.
- `nix develop -c cargo test --workspace` passed: 443 Rust tests, including 72
  service tests. Rust formatting and `git diff --check` passed.
- `nix flake check` passed all 49 checks for the compatible local system. Nix
  omitted `aarch64-darwin` and `aarch64-linux`. The live store hash and mtime
  again remained unchanged.

No live cleanup was performed. The service, timers, queue, retained rows,
conversation, and Pi session were not changed by the investigation.
