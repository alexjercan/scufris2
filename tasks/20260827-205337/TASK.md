# Settle the submission identifier and its stale timer

- STATUS: CLOSED
- PRIORITY: 85
- TAGS: bug, desktop, service

## Source

Review round 1 of `20260827-081702`, range `185034a..a13cb38`. Findings
M1 and M2. Full record: `tasks/20260827-081702/REVIEW.md`.

Both hinge on the same sentence, `state.rs:105`: "Identifier reused by
every retry." One finding is a guard that vanished and left its comments
behind; the other is a timer that cannot tell two attempts apart because
of that reuse.

## M1. Duplicate submission suppression was dropped

`command()` (`service.rs:733`) keys `pending` by its own `c-{N}`
correlation. The client's `id` is carried for the reply and never looked
up. v2 suppressed by identifier: `accepted: Map<string, Set<string>>`
and `SubmissionConflictError` at `desktop/server.ts:514`, deleted in this
range with nothing to replace it.

Six places still say the service suppresses: `app.rs:546`,
`app.rs:1335`, `app.rs:2365`, `state.rs:105`, `state.rs:358`,
`conversation.rs:57`. Two are load-bearing:

- `clear_pending` declines to reopen the pill on a failed removal
  _because_ it believes a resend cannot reach the conversation twice.
- `process_prefix` spends 16 bytes of OS randomness on the stated
  grounds that a collision would have a genuine request refused.

Not a blocker: no path resends on its own. Every resend goes through
`Delivery::Uncertain`, which is non-editable and needs two Enters past
an explicit warning, so the person is always asked.

Decide one of two, and record which with this task:

- Add an accepted-identifier set to the service and refuse a repeat,
  restoring what the comments describe; or
- Rewrite the six comments to say the identifier is a correlation handle
  and the warning is the only guard, then revisit whether
  `clear_pending` should still stay quiet and whether `process_prefix`
  still needs 16 bytes of randomness.

## M2. A stale acknowledgement timer freezes a live retry

`submit()` (`app.rs:1499`) spawns one 15 s timer per call, keyed only by
`id`, with no cancellation and no generation guard. `take_recording` and
`transcription` both use `capture_generation` for exactly this
(`app.rs:1370`) - that is the pattern to copy. The receiving guard at
`state.rs:780` is `*id == uncertain` alone, so with the identifier reused
by every retry the timers cannot be told apart.

Submit at t=0, refused at t=2, retry at t=3. At t=15 the first timer
fires, matches the second attempt's phase, and freezes a 12-second-old
live submission into `Delivery::Uncertain` with "The backend did not
confirm delivery." and a forced-send warning it has not earned. A retry
issued 14 s after the original gets a one-second deadline.

Not a blocker: a genuine late acknowledgement still retires the pill
through `Retained + Acknowledged` (`state.rs:699`), so the wrong state
is usually transient.

The red team lane reported the opposite - that the identifier and
`Phase::Sent` together make a stale timer harmless. That holds for a
later take, which gets a new identifier. It does not hold for a retry,
which reuses one, and the retry is the case that matters.

Fix: give the timer a generation, the way `capture_generation` does, and
have `state.rs:780` check it alongside the identifier.

## Proof

The existing test `a_daemon_refusal_keeps_the_words_editable_and_
ordinarily_retriable` (`app.rs:2596`) already builds this state and stops
one line short. `QueueExecutor::spawn_after` discards the delay and
`expire()` drains in push order, so one added `expire()` exhibits M2.
Write that as a failing test first.

- `cd native && TMPDIR=/tmp nix develop --offline -c cargo test
--workspace`.
- If M1 is fixed rather than documented: a service-level test that a
  repeated identifier is refused, and a check that the pill's
  `{prefix}-{n}` and the conversation window's `{prefix}-h{n}` still
  cannot collide.

## Outcome (2026-08-27)

### M1: documented, not restored

Took the second option. The identifier is a correlation handle. Nothing
suppresses by it, and the comments now say so.

The guard is not gone, it moved. `Companion::restore` brings a recovered
record back as `Delivery::Uncertain`, which is not editable and needs two
Enters past an explicit warning. Read `restore` and every path that
reaches it: there is no path that resends without asking. Adding an
accepted-identifier set to the service would have added a second guard
behind the one that already holds, and a table that grows for the life
of the process.

Both decisions the finding asked to revisit stand, on new grounds:

- `clear_pending` stays quiet. Not because a resend cannot reach the
  conversation twice, but because the person is asked before it does.
  Reopening the pill on a failed removal would take a window back from
  someone who just finished with it, to warn about something they are
  going to be warned about anyway.
- `process_prefix` keeps its 16 bytes. A collision no longer refuses a
  genuine request; it does something worse. The service echoes the
  identifier and nothing else names the submission, so two live
  submissions sharing one take each other's answers.

Seven comments rewritten: `app.rs` process_prefix, clear_pending, and
the test at 2418; `state.rs` `Retained.id`, `Companion::new`, `restore`;
`conversation.rs` `Conversation::new`. Plus `docs/src/dev/desktop.md`,
which stated the deleted rule as the reason the conversation window
takes a `-h` counter.

### M2: fixed with a generation

`App::submissions`, an `AtomicU64` beside `capture_generation`. `submit`
claims a generation before anything can go out, so a later attempt has
already retired this one's timer whichever of them the executor runs
first. The timer returns if the count has moved.

Guarded in `submit` rather than at `state.rs:780`, which the finding
suggested. `Event::SubmissionUncertain` is also raised by the transport
error path, which is not stale and carries no generation. Keeping the
guard where the timer is spawned leaves that path alone.

### Proof

Written failing first, and it took three attempts to write one that
fails. `expire()` fires every timer, so the live attempt's own timer
settles the state legitimately and the assertion passes with the defect
present. Firing two `expire_oldest()` calls assumed the stale timer was
first; it is not - an interleaved task sits at index 0 and there are
three pending timers, not two. What works: snapshot `pending_delays()`,
fire all but the last one at a time, and assert the state stays `sent`
throughout. Added `QueueExecutor::pending_delays` and `expire_oldest`
for it.

Verified both ways. With the guard disabled the test fails with "timer 2
of 3 settled a submission that is still running". With it restored it
passes.

- `cargo test --workspace`: 336 passed, 0 failed.
- `cargo clippy --workspace --all-targets`: clean.
- `cargo fmt --all --check`: clean.
- `npx prettier --check docs/src/dev/desktop.md`: clean.
