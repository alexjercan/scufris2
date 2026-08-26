# Startup restore and window verdicts trust the X round trip

- STATUS: OPEN
- PRIORITY: 80
- TAGS: desktop, bug

## Goal

Two defects with one mechanism: the companion reads `is_visible` and
`is_focused` back before the X server round trip has happened, and
acts on the false answer.

1. A transcript restored at startup is always abandoned.
   `runtime.start()` runs inside Tauri's `setup`, before the event
   loop, so the pill's show has not been carried out when the verdict
   is read; `is_visible` answers false, the state machine reads "the
   pill did not come up", and `abandon()` drops the recovered words
   (`app.rs:477-493`). This loses text the pending store exists to
   protect (`pending.rs` doc: durable from accept until ack or
   discard).
2. Show and hide verdicts are wrong in normal operation too. Alex's
   live session (2026-08-26) logged `the pill did not take the
keyboard` four times at 250ms intervals during a listening that in
   fact had the keyboard, `the transcript box is still up` after every
   hide, and at startup `the transcript box did not come up` plus `the
pill did not come up` - while the windows were verifiably in the
   right state (task `20260826-094501` reproduced this on Xvfb + i3
   and confirmed the windows were correct while the verdicts said
   otherwise).

The diagnosis record in `tasks/20260826-094501/TASK.md` ("Second
defect, recorded and not fixed") is the starting point; it concluded
these belong to one fix, separate from the keyboard-refusal bug fixed
there.

## Reproduction (confirmed live by Alex, 2026-08-26)

Dead daemon socket keeps an accepted transcript unacknowledged:

    SCUFRIS_DESKTOP_SOCKET=/tmp/scufris-repro.sock \
    SCUFRIS_DESKTOP_STATE_FILE=/tmp/scufris-repro-pending.json \
    nix run .#scufris-desktop -- --foreground

Super+D, speak, Super+D, Enter (no Escape - Escape discards the
record). The state file holds the words. Ctrl+C, run the same command
again. Observed log, second run:

    INFO  phase from="resting" to="retained"
    WARN  the transcript box did not come up
    ERROR the pill did not come up
    INFO  phase from="retained" to="resting"

The restore itself works: the words enter `Phase::Retained` with
`Delivery::Uncertain` (`state.rs:395-408`, presents as "uncertain", so
the box raise is attempted). The false verdict then abandons them.
Expected: the pill up with the recovered words, box raised, Enter to
confirm or resend, Esc to discard.

## Scope

- Make the show/hide/focus verdicts honest: verify after the request
  has actually reached the X server (or stop pretending to verify).
  The startup path additionally must not decide before the event loop
  has run - the restore decision has to survive until the first real
  show completes.
- The verdict warnings should become trustworthy enough that a logged
  failure means a real failure.
- Keep the keyboard-refusal ordering from `20260826-094501` intact:
  `review::raise` refuses the keyboard before every show, and a box
  that cannot refuse stays down.
- The restored transcript must come up: pill visible, uncertain
  presentation, Enter confirms or resends, Esc discards.

## Verification

- Alex's reproduction above, rerun live: the second start presents the
  recovered words instead of dropping them.
- A startup log free of false `did not come up` / `still up` /
  `did not take the keyboard` verdicts in a session where the windows
  behave.
- Regression tests at the state-machine level: a restore decided
  before the first show completes is not abandoned.
- The usual checks: cargo fmt/clippy/test/build, build.rs tsc, npm
  typecheck, prettier (never `ui/orb-engine.js`), `TMPDIR=/tmp npm
test`.
