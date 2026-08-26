# Pill dead to input after a turn ends

- STATUS: IN_PROGRESS
- PRIORITY: 85
- TAGS: desktop, bug

## Goal

Fix the lockup Alex hit on the live desktop (2026-08-26), the first
real session on the bare orb pill.

His first wording was wrong about where the lockup starts. The
corrected sequence, from him:

- Super+D starts a listen.
- Super+D again stops it and raises the review box. This turn works:
  he edits the words, Enter sends them, the assistant answers and
  speaks, and the companion returns to idle.
- Super+D starts a second listen. This works too.
- Super+D again raises the review box a second time. Here the keyboard
  is dead: typing does nothing, and Escape does nothing.

There is one question, not two. The second activation showing a
textbox is correct: it is a second turn reaching its own review. The
question is why no key reaches that review.

The first wording also had an Escape before the Enter that sent. That
cannot have happened in review: Escape there discards the transcript
and ends the review, so no Enter could follow it into a send. Whatever
key that was, it was not an Escape in review.

## Scope

- Reproduce first, at the state-machine level: drive the companion
  through listening -> review -> Enter -> working -> speaking -> idle
  -> listening -> review in the cargo test harness (`app.rs` has the
  fake surface) and assert what the second review presents, what the
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

## Diagnosis and fix (2026-08-26)

### What the first turn left behind

The transcript box takes the keyboard away from the orb, and only from
its second appearance onwards.

The box is built refusing the keyboard: `.focused(false)` in
`review::ensure`, which tao turns into `accept-focus false` on the GTK
window, which is `WM_HINTS.input = False` on the X11 window. A window
manager that reads that hint must not give the window the keyboard.

tao then gives the right back on its own. It installs a one-shot draw
handler on every window it builds and calls `set_accept_focus(true)`
from it, to restore accept-focus after the window has been drawn. That
handler fires the first time the box is painted, which is the first
review. From that moment the box advertises `input = True`.

Nothing reads the hint again while the window stays mapped, so the
first review is unaffected. Leaving review unmaps the box. i3
unmanages an unmapped window and manages it again on the next map, and
managing it reads `WM_HINTS` afresh. So the second review comes up as
a window that says it accepts input, and i3 gives it the keyboard.

The orb window loses it silently. The box has no key handling of its
own - every key belongs to the orb window - so typing goes nowhere and
Escape goes nowhere. Nothing can take the keyboard back either: the
runtime believes the orb is up and focused, because it was when the
runtime last looked, and an activation inside a review is a no-op in
the state machine. The turn is stuck until the process is killed.

### Where it is not

- Not the state machine. It presents the second review exactly like
  the first: same state, same text, editable, and it asks for the
  keyboard again.
- Not `dismissed`, not a retained transcript, not an unretired pending
  record. The corrected sequence has no Escape in review at all.
- Not the stale `Screen::Ready` record or the bounded repair chain.
  Those explain why nothing notices the loss, not why it happens.

### The fix

`review::raise` says the refusal again before every raise, in the one
order that works: refuse the keyboard, place, show, confirm, keep on
top. The refusal has to reach the window before the window manager
sees the window, and the window manager sees it when it is mapped.

A box that will not refuse the keyboard now does not come up at all.
Unread words are a smaller loss than a person with no keyboard.

The show path moved behind a small `Frame` trait so this order is
testable without a display. No behaviour outside `review.rs` changed.

### Evidence

Reproduced and fixed on a headless Xvfb display running i3 with Alex's
own `focus_follows_mouse yes` and `focus_on_window_activation smart`,
driving real Super+D presses through xdotool against a fake daemon and
a fake transcriber.

Before, with the input focus read from the X server at each review:

    [t1 review] focus=0xa00010   (the orb window)
    [t2 review] focus=0xa00192   (the transcript box)
    [t2 typed]  focus=0xa00192   typing dead
    [t2 escape] focus=0xa00192   Escape dead, box still up

`xprop` on the box after its first draw showed `Client accepts input
or input focus: True`, against the `False` it was built with.

After, four consecutive reviews:

    [t1..t4 review] focus=0xa00004   (the orb window, every time)

with `reviewing -> sent` for the three Enters and `reviewing ->
resting` for the Escape that ended the fourth.

### Regression tests

- `review::tests::every_raise_refuses_the_keyboard_before_the_box_is_on_screen`
  drives two consecutive raises and asserts both say the refusal, and
  say it before the show. With the refusal removed it fails: the
  recorded order starts at `place`, not at the refusal.
- `review::tests::a_box_that_will_not_refuse_the_keyboard_never_comes_up`
  asserts a box that cannot refuse the keyboard is not shown, while a
  placement failure is only reported.
- `app::tests::a_second_turn_reviews_exactly_like_the_first` walks the
  corrected sequence in the fake-ports harness: listen, review, Enter,
  acknowledged, working, speaking, idle, listen, review. It asserts
  the second review presents the same state and text, is editable, and
  holds the keyboard.

Honest note on the fail-before requirement: the state-machine walk
passes on `ca4766f` too, which is the finding rather than a gap - the
phases are innocent. The two `review.rs` tests cover code that did not
exist on `ca4766f`, so "fails before" was shown by removing the fix
from the new code and running them, quoted above. The proof that the
old code was broken is the live evidence above, not a unit test.

### Second defect, recorded and not fixed

Show and hide verdicts are unreliable on this desktop. The same runs
logged `the pill did not take the keyboard` (four times at 250ms while
listening), `the transcript box is still up` after every hide, and
`the transcript box did not come up` and `the pill did not come up` at
startup - while the windows were in fact in the right state. These are
`is_focused` and `is_visible` answers read back too soon after the
request, before the X server round trip. They do not share a cause
with the keyboard loss, so they are not fixed here. They are the same
mechanism as the startup-restore bug in `tasks/20260825-235144`, which
stays out of scope, and they should be fixed with it.

### Checks

All run under `nix develop`.

- `cargo fmt --check`: pass.
- `cargo clippy --all-targets`: pass, no warnings.
- `cargo test -p scufris-desktop`: 119 passed, 0 failed.
- `cargo build -p scufris-desktop`: pass.
- `npx --no-install tsc -p desktop/scufris-desktop/ui/tsconfig.json`:
  pass.
- `npm run typecheck`: pass.
- `npm run format:check`: pass.
- `TMPDIR=/tmp npm test`: 123 passed, 0 failed.

### Not verified

The live retry on Alex's own desktop. The headless harness runs the
same window manager and the same policy, but it is not his session.
