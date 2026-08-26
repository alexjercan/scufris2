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

## Regression (2026-08-26)

Alex's live retry failed, worse than the original bug: the transcript
box came up and nothing worked in it. Escape echoed `^[` in his shell
and Enter pressed in his terminal, so the keyboard had never left the
window he started from. Only Super+D still worked, which is a global
hotkey grab and works whoever holds the keyboard. His log:

    09:21:19.146 INFO phase resting -> listening
    09:21:20.966 WARN the pill did not take the keyboard
    09:21:26.836 INFO phase listening -> transcribing
    09:21:27.329 INFO phase transcribing -> reviewing

I had seen the same line at the same 1.8s on the harness and recorded
it above as "true, and the intended behaviour". That was wrong. It is
this bug, and the entry above that calls it intended is wrong with it.

### What was actually happening

The pill never takes the keyboard on the show that first puts it on
screen, and once that has happened no later request can take it
either. i3's own log, with the pill mapping at the start of listening:

    manage_window:108 - window 0x00800010
    WM_HINTS.input changed to "0"
    x_push_changes:1413 - Updating focus by sending WM_TAKE_FOCUS to window 0x00800010
    WM_HINTS.input changed to "1"
    handle_client_message:760 - _NET_ACTIVE_WINDOW: Window 0x00800010 should be activated   (x4)

A window manager reads whether a window will take the keyboard once,
when it takes the window over, and it takes it over when the window is
mapped. tao builds the pill with `accept_focus(false)`, because the
window is created hidden and unfocused, and restores it from a one-shot
`connect_draw` handler - after the mapping. So i3 manages the pill as a
window that cannot be focused. It focuses the pill's container anyway
and offers the keyboard with `WM_TAKE_FOCUS`, which GTK never answers
because as far as GTK is concerned the window does not want it. The
keyboard stays on the person's window.

The second half is what makes it unrecoverable. i3 now has the pill
recorded as its focused window. Every `_NET_ACTIVE_WINDOW` that
`set_focus` sends afterwards is a request to activate the window i3
already believes is active, so i3 changes nothing and pushes nothing:
the four requests in the log above are all no-ops. The runtime asks
four times, is refused silently four times, and gives up.

This is the same defect the honest verdicts were built to report, and
they reported it correctly. What they removed was the accident that
used to paper over it: the old false negatives made the runtime keep
re-showing, and on some machines one of those later attempts landed
after i3 had been made to re-push focus for an unrelated reason. On
this harness that accident fires at the phase change out of listening,
which is why turn one looked fine here both before and after the
earlier commits. On Alex's desktop it never fired, so his box was dead
from the moment it came up. Both are the same root cause; only the luck
differs.

### The fix

The pill says what it will do with the keyboard before it is shown, so
the window manager reads the truth when it takes the window over.

- `src/pill.rs`: an `Opening` trait with `accept_keyboard` and `show`,
  and `open(frame, accept)` which runs them in that order. `reveal`
  calls `open(window, true)`; a pill that cannot claim the keyboard
  stays down, because the phase that asked for it has nowhere else to
  send an Enter. `show_passive` calls `open(&window, false)`: the
  handoff posture must refuse the keyboard before the mapping too, or
  it takes the keys the person is typing into their own window. A
  passive pill that could not say so is still put up. Four tests cover
  the order and both refusals.
- `src/app.rs`: `Surface::pill_has_keyboard`, and `raise` no longer
  trusts `Screen::Ready` on its own. What a show achieved was true when
  it was written down; the person can click their own window between
  one phase and the next. Entering review now asks the display, and a
  pill that has lost the keyboard is shown again with a whole repair
  budget rather than skipped on an older record.
- `src/main.rs`: `DesktopSurface::pill_has_keyboard` answers from
  `pill::focused`, which already asks the display.

The keyboard-refusal contract in `review.rs` is untouched. The box
still refuses the keyboard before every show, and i3 still answers it
with `WM_TAKE_FOCUS` that nothing takes up, which is exactly what keeps
the orb its keys.

### Live evidence

Xvfb 1920x1080 + i3 with Alex's settings (no titlebars, zero borders,
`focus_follows_mouse` and `focus_on_window_activation` at the i3
defaults his config never overrides), an xterm holding the keyboard,
the pointer on it, and Super+D pressed 1.2s after start as in his log.
Keyboard sampled every 100ms.

Before, at 7f3b49f, the commit before this task's work - the same
failure the regression shows, so this is not something today's commits
introduced:

    phase resting -> listening
    WARN the pill did not take the keyboard   (x4)
    phase listening -> transcribing
    WARN the pill did not take the keyboard
    ...
    WARN the transcript box is still up
    WARN the pill is still up

    turn1-listening   the terminal   (4s, every sample)
    turn1-review      Scufris

After:

    phase resting -> listening
    phase listening -> transcribing
    phase transcribing -> reviewing
    phase reviewing -> resting
    (and the same four again for turn two)

    turn1-listening   Scufris        turn2-listening   Scufris
    turn1-review      Scufris        turn2-review      Scufris
    turn1-after-escape the terminal  turn2-after-escape the terminal

Two full turns, no warnings at all, the keyboard on the pill from the
first moment of listening through review, and back on the terminal
after each Escape. i3 now reports what it does with it:

    manage_window:108 - window 0x00800004
    WM_HINTS.input changed to "1"
    x_push_changes:1423 - Updating focus (focused: ...) to X11 window 0x00800004

The startup restore still holds, and now holds the keyboard too: the
seeded transcript comes up `retained`, pill and box both `IsViewable`,
input focus on the pill, Escape discards and gives the terminal its
keyboard back. No warnings.

### Checks

- `cargo fmt --check`: clean.
- `cargo clippy --all-targets`: clean, no warnings.
- `cargo test -p scufris-desktop`: 135 passed, 0 failed (130 before).
- `cargo build -p scufris-desktop`: ok.
- `npx --no-install tsc -p ui/tsconfig.json`: clean.
- `npm run typecheck`: clean.
- `npm run format:check`: all matched files use Prettier code style.
- `TMPDIR=/tmp npm test`: 123 passed, 0 failed.

The new `raise` test was checked against the unfixed code and fails
there, so it holds the behaviour rather than describing it.

### What stays unverified

- Alex's live rerun is the sign-off. The acceptance surface is his own
  session shape: the keyboard on the pill through listening and review,
  Escape and Enter reaching the box rather than his shell, and any
  warning that does print being a real, final failure.
- Alex's exact review failure was not reproduced here. This harness
  recovered the keyboard at the phase change out of listening on every
  pre-fix run, his never did. The account above explains both from one
  cause, and the fix removes the cause rather than the luck, but the
  difference between the two machines is inferred and not measured.
- Nothing here was tested on a compositor or on any window manager
  other than i3. The hint is what the ICCCM says it is, so this should
  hold anywhere, but only i3 was watched doing it.
