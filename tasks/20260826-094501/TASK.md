# Pill dead to input after a turn ends

- STATUS: IN_PROGRESS
- PRIORITY: 85
- TAGS: desktop, bug

## Goal

Fix the lockup Alex hit on the live desktop (2026-08-26), the first
real session on the bare orb pill. His report, near verbatim:

> I press Super+D, start talking, press Super+D again, it pops up the
> textbox, I can type, press Esc, press Enter and it sends the text
> and does the thing and speaks. Then I press Super+D again while it
> shows it as idle: it shows the textbox, but I cannot type anything
> and Esc does not work. Basically Super+D after it goes into idle
> stops working.

Two oddities to explain, not just patch:

- After the turn ends in idle, Super+D shows "the textbox" instead of
  starting a fresh listen. Either the review window is being raised
  from a stale state, or the state machine re-enters
  review/retained/uncertain with leftover words.
- Once it is up, no key lands: typing dead, Esc dead. That smells like
  the keyboard focus was never acquired on the second show, or the
  invisible transcript field is still hidden or read-only from the
  previous turn.

Note the mid-flow Esc: Alex pressed Esc and then Enter in review, and
the send still worked. Whatever record or flag that Esc left behind
(dismissed, a retained transcript, an unretired pending record) is a
prime suspect for what the next activation trips over.

## Scope

- Reproduce first, at the state-machine level: drive the companion
  through listening -> review -> Esc -> Enter -> working -> speaking ->
  idle -> activate in the cargo test harness (`app.rs` has the fake
  surface) and assert what the second activation presents, what the
  field's hidden/read-only state is, and whether focus was requested.
- Suspects, newest first: the show/cut ordering and focus work from
  `tasks/20260825-235144` (commits 0cfbb48, ac3ac9b), the two-window
  raise ordering and 1x1 field from `tasks/20260825-231826` (3af4921,
  41fc9d6), the `dismissed` flag and `posture()` in `state.rs`, focus
  save/restore in `focus.rs`, pending record retirement in
  `pending.rs`/`app.rs`.
- Fix the root cause. If the Esc-then-Enter flow exposes a second,
  separate defect, record it here and fix it only if it shares the
  cause.
- The startup-restore bug recorded in `tasks/20260825-235144` (a
  transcript restored at startup is always abandoned) is OUT of scope
  unless it turns out to be the same root cause.

## Verification

- A regression test walks Alex's exact sequence and fails before the
  fix, passes after.
- `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test -p
scufris-desktop`, `cargo build`, the build.rs tsc, `npm run
typecheck`, prettier check (never `ui/orb-engine.js`), and
  `TMPDIR=/tmp npm test` all pass.
- The live retry of the same sequence is Alex's.
