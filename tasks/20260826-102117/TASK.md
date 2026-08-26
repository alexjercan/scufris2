# Startup restore and window verdicts trust the X round trip

- STATUS: IN_PROGRESS
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

## Diagnosis and fix (2026-08-26)

### Mechanism

Showing a window is a message, not an act. `window.show()`,
`window.hide()` and `window.set_focus()` put a `WindowRequest` on a
glib channel. Only the GTK main loop takes it off and carries it out.
`is_visible()` reads `gtk_widget_get_visible` and `is_focused()` reads
`gtk_window_is_active`, and both of those change only after the loop
has done the work - for focus, only after the X server has sent a
`FocusIn`. So an answer read straight back after a request describes
the world before the request. Both defects are that one read.

At startup the read is not merely early, it is impossible. Tauri's
`setup` runs before the event loop exists, and a request made from the
loop's own thread is handled inline, which means it is not handled at
all until there is a loop. `pill::reveal` asked, got `false`, returned
`the pill did not come up`, and the follower in `App::start` called
`abandon()`. The recovered words were dropped on every start.

Off that thread the visibility answer usually catches up on its own,
but the focus answer never does inside one call. That is `the pill did
not take the keyboard` four times at 250ms: the first attempt and
three repairs, each asking before the answer could exist. `the
transcript box is still up` after every hide is the same read on the
other side.

### The fix

A verdict is now an answer from the X server, not from the toolkit,
and it is only read when it can mean something.

`src/display.rs` is new. It holds a `Verdict` of `Yes`, `No` or
`Unsure`, and its own `RustConnection`. `up` asks
`GetWindowAttributes` for `MapState::VIEWABLE`; `keyboard` asks
`GetInputFocus` and walks parents with `QueryTree`, because the
keyboard lands on the WebKit child, not on the frame. Both are real
round trips, so an answer is the server's truth at the moment it is
given. `came_up`, `went_down` and `took_the_keyboard` wait for the
wanted answer - 500ms for a map or unmap, 250ms for the keyboard,
asking again every 10ms.

Waiting is only allowed where waiting is possible. The module records
which thread runs the event loop and whether the loop is running. On
the loop's own thread, or before the loop starts, nothing can change
while the caller waits, so these return `Unsure` at once instead of
blocking or lying. `Unsure` means "asked, nothing could say". It is
not a failure and it never abandons anything.

`Shown` gained `Unsure(String)` and `Seen` now carries
`Option<String>` so a show that is on screen without the keyboard has
nothing to report. `Hidden` is new: `Down` or `Unsure`. `Screen`
gained `Unknown`, which is not visible and falls short of every
posture, so an unknown screen is asked about again by the repair chain
rather than written down as fact.

The verdict warnings moved. Per-attempt failures no longer warn.
`App::repair` warns once, with `shortfall()`, only when the bounded
chain has given up. A logged `did not come up` / `is still up` / `did
not take the keyboard` now means the window never reached the posture
across the whole chain.

### File by file

- `src/display.rs` (new): `Verdict`; `up` and `keyboard` over x11rb
  round trips; `came_up`, `went_down`, `took_the_keyboard`;
  `runs_the_event_loop` and `the_event_loop_is_running`; `identify`
  caches a window's XID. Falls back to the toolkit's positive answers
  only when there is no X session or the window is not on it. Unit
  tests cover the parent walk (self, child, stranger, `None`,
  `PointerRoot`, cycles) and that nothing waits before the loop runs.
- `src/main.rs`: `display::runs_the_event_loop()` first in `start`.
  `runtime.start()` moved out of `setup` into a `RunEvent::Ready` arm,
  on a thread of its own. The tray voice item also spawns, so an
  activation is not decided on the loop's thread. `Surface` impls
  follow the new return types.
- `src/pill.rs`: verdicts from `display`. `reveal` waits for the map,
  then shapes, then raises, then asks for the keyboard;
  `Verdict::Unsure` yields `Shown::Unsure`, not an error. The entrance
  tween plays only on a confirmed-down window. `cut` uses
  `display::identify`. `hide` returns `Hidden`.
- `src/review.rs`: the keyboard-refusal ordering is untouched -
  `accept_focus(false)` still comes before every show and a box that
  cannot refuse still stays down. Only the verdict changed: the
  `Frame` trait's `visible() -> Result<bool, String>` became
  `seen() -> Verdict`, and only `Verdict::No` is a refusal.
- `src/app.rs`: `Shown::Unsure`, `Hidden`, `Screen::Unknown`;
  `record()` maps a `Shown` onto the screen record and warns only for
  real operation errors; `lower()` treats an unsure hide as `Unknown`
  so it is asked again; `repair()` warns `shortfall()` once when it
  gives up. New tests: a restore nothing can confirm keeps the words
  on screen, an unconfirmed show never opens the microphone, an
  unconfirmed hide is asked again rather than written down, and the
  shortfall names the posture the window never reached.

### How the startup decision now sequences against the event loop

`RunEvent::Ready` is the loop's first iteration, so by then the loop
exists and can carry requests out. `start()` is spawned from there on
its own thread. That gives both halves of what the decision needs: the
loop is running, so a show can actually happen, and the deciding
thread is not the loop, so it may wait for the X server to confirm it.
`display` enforces exactly this - it will not wait on the loop's
thread, and it will not pretend to have waited.

### Live evidence on Xvfb + i3

Xvfb 1920x1080, i3, no compositor, dead socket, seeded state file.

Before, with the state file holding `the words that must survive a
restart`, the same log shape Alex recorded:

    INFO  phase from="resting" to="retained"
    WARN  the transcript box did not come up
    ERROR the pill did not come up
    INFO  phase from="retained" to="resting"

`xwininfo` at that moment: `[Scufris] Map State: IsViewable`. The pill
was up while the log said it was not, and the words were gone.

After, same harness and same seed:

    INFO  starting version="0.4.0" ...
    INFO  phase from="resting" to="retained"

No `did not come up`, no `retained -> resting`, no further lines. The
words stayed on screen. Windows at that moment:

    6291525 [Scufris transcript] 650,614 620x140 IsViewable
    6291460 [Scufris]            865,778 190x230 IsViewable
    focus: 6291460

Both at their computed positions ((1920-620)/2=650, 778-140-24=614;
(1920-190)/2=865, 1080-230-72=778), and the keyboard on the pill.

A full turn on the same instance, driven with xdotool - Escape to
discard, then Super+D on and off:

    INFO phase from="retained" to="resting"
    INFO phase from="resting" to="listening"
    INFO phase from="listening" to="transcribing"
    INFO phase from="transcribing" to="reviewing"
    INFO phase from="reviewing" to="resting"

No verdict warnings anywhere. Map state followed the phases: both
windows `IsUnMapped` after the discard, the pill `IsViewable` and
focused during listening, the box `IsViewable` during review, both
`IsUnMapped` after the final discard. Before the fix this same
sequence logged `the transcript box is still up` after every hide.

One warning does still appear in the harness, and it is true. On a
cold start with no pending transcript, the first Super+D logs `the
pill did not take the keyboard` once, about 1.8s in, when the repair
chain gives up. Sampling `xdotool getwindowfocus` every 200ms shows
the keyboard on `0x200064 "i3"` - i3's own window - for the whole
listening phase, while the pill was mapped. The pill got the keyboard
only when the review box was raised. So the pill really did not have
it, i3 kept it, and the warning says so. That is the intended
behaviour: one honest line when the bounded chain gives up, instead of
four false ones at 250ms.

### Checks

All run under `nix develop`.

- `cargo fmt --check`: clean.
- `cargo clippy --all-targets`: clean, no warnings.
- `cargo test -p scufris-desktop`: 130 passed, 0 failed (119 before).
- `cargo build -p scufris-desktop`: ok.
- `npx --no-install tsc -p ui/tsconfig.json` from
  `desktop/scufris-desktop/`: clean.
- `npm run typecheck`: clean.
- `npm run format:check` (prettier over the tree, this file included):
  all matched files use Prettier code style.
- `TMPDIR=/tmp npm test`: 123 passed, 0 failed.

### What stays unverified

- Alex's own rerun of the reproduction on his display is the sign-off.
  The harness is Xvfb + i3 with no compositor and no pointer, and its
  focus behaviour is not his.
- The four-at-250ms keyboard warning was not reproduced before the fix
  on this harness, because there the pill genuinely does not take the
  keyboard during listening, so a true warning is expected. The
  evidence that those lines were false is Alex's log plus the read
  order in the old code; the after state is proved instead by the
  restore run, where the pill did have the keyboard and nothing warned.
- On a slower cold start the repair chain could exhaust before a first
  WebKit window accepts focus, which would log one true but unhelpful
  keyboard line. Worth watching in Alex's rerun.
