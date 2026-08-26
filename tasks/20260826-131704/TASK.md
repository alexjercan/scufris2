# First review of a fresh process loses the keyboard

- STATUS: IN_PROGRESS
- PRIORITY: 85
- TAGS: desktop, bug

## Goal

Fix the remaining keyboard loss Alex hit live (2026-08-26), after the
pre-map claim from task `20260826-102117` landed. It is now confined
to the first review of a fresh process; every later review works.

His flow, near verbatim: run `nix run .#scufris-desktop --
--foreground`, Super+D, speak, Super+D. The review box shows the text
(correct format, caret included) but he cannot type; Esc and Enter do
nothing. Clicking the pill recovers Esc but nothing else. He isolated
the variants across multiple fresh runs:

- run + click + Esc: works (the pill closes).
- run + click + Enter: nothing.
- run + click + anything else (typing, clicking the textbox): nothing,
  only Esc works after the click.
- After a click + Esc recovery, a second Super+D turn in the same
  process works completely, no click needed.

## What the report pins down

- Before the click, even Esc is dead: the pill window holds no X
  keyboard focus at all during the first review.
- After the click, Esc works but Enter does not: Esc must be handled
  at the window or document level in `ui/pill.ts`, while Enter needs
  the invisible transcript field to be the focused element - and the
  click gave the window focus without giving the field page focus.
  Both halves need fixing: the window must hold the keyboard in the
  first review, and a recovered window must put the field back too.
- First review versus every later review is the asymmetry to explain.
  The prime suspect is the one-shot accept-focus restore tao installs
  per window (`connect_draw` -> `set_accept_focus(true)`), which
  fires exactly once, on each window's first paint - the review box's
  fires in the middle of the process's first review, flipping
  `WM_HINTS.input` to 1 while the box is mapped. The box's first
  manage by i3 (a fresh container, possibly focused at manage time
  with `WM_TAKE_FOCUS` offered and never answered - the original pill
  crime, now on the box) is the other first-time-only event in that
  window.

## Scope

- Reproduce on the Xvfb + i3 harness from `20260826-102117`: fresh
  process, first turn straight into review, i3 debug logging at the
  box's first manage and at the moment tao's draw handler flips the
  hint. Confirm where the keyboard actually is during the first
  review.
- Fix the window half: the pill must hold the keyboard through the
  first review of a fresh process. If tao's one-shot restore is the
  mechanism, neutralize it for both windows (the box must never
  advertise input, the pill already claims before map).
- Fix the page half: `ui/pill.ts` must re-focus the transcript field
  when the window regains focus while a review is editable, so any
  recovery (click included) restores Enter and typing, not only Esc.
  Check the Esc/Enter handler asymmetry and make recovery symmetric.
- The claims that hold today stay: pre-map keyboard claim on the pill,
  pre-raise refusal on the box, fresh repair budget at review entry,
  honest verdicts, second-turn behavior.

## Verification

- Harness: a fresh process's first turn holds the keyboard through
  listening and review, Enter sends, Esc discards; a second turn still
  works; zero false verdict warnings.
- Regression coverage for whatever mechanism is found, failing before
  the fix where the layer allows it.
- The full check suite: cargo fmt/clippy/test/build, build.rs tsc,
  npm typecheck, prettier (never `ui/orb-engine.js`), `TMPDIR=/tmp npm
test`.
- Alex's live rerun of his exact flow is the sign-off.

## Diagnosis (2026-08-26)

Two defects, one in each half, both reproduced on the harness.

### The window half: the box answered an offer it should have let lapse

A window manager reads whether a window wants the keyboard once, at
manage time. tao builds a window created hidden and unfocused with
`accept_focus(false)`, and installs a one-shot `connect_draw` handler
that calls `set_accept_focus(true)` after the first paint. For the
review box that first paint lands in the middle of the process's first
review, which is why only the first review is lost.

i3 manages the box, reads `input = 0`, and offers the keyboard with
`WM_TAKE_FOCUS` instead of taking it away by force. GTK answers such an
offer only if `accept_focus` is true when it processes the message. The
draw handler flips the hint in between, so GTK answers, calls
`XSetInputFocus` on the box, and the pill loses the keyboard mid-review.
The box has no key handlers, so every key dies, Escape included. From
the i3 log, before the fix:

    01:20:27 manage_window:108 - window 0x00800293
    01:20:27 WM_HINTS.input changed to "0"
    01:20:27 x_push_changes:1413 - Updating focus by sending
             WM_TAKE_FOCUS to window 0x00800293
    01:20:27 WM_HINTS.input changed to "1"
    01:20:27 handle_focus_out: window 0x00800010 lost focus
             detail=Nonlinear, mode=Normal
    01:20:27 handle_focus_in: focus change in, for window 0x00800293

`0x00800010` is the pill. Every later review reuses the same box, whose
draw handler has already run and disconnected, so no later review is
touched.

Fix: build both windows with `.focusable(false)`. That is not the same
as `.focused(false)`: it sets the hint at build time and skips the
restore handler entirely, so the box never advertises input at any
moment, and what the pill says about the keyboard is only ever what
`open` last said. After the fix, the same sequence ends at the offer:

    01:48:26 x_push_changes:1423 - Updating focus (...) to X11 window
             0x00800010
    01:48:27 manage_window:108 - window 0x008002d7
    01:48:27 WM_HINTS.input changed to "0"
    01:48:27 x_push_changes:1413 - Updating focus by sending
             WM_TAKE_FOCUS to window 0x008002d7

### The page half: the click that brings the window back takes the field

The report reads the click as restoring the window without the field.
The measurement says the opposite: the field holds the page focus
across the whole window focus loss, and it is the click itself that
takes it away. Probes in the page, at the moment Alex's recovery click
lands:

    10:44:00.705 PROBE focus     active=true  editing=true
    10:44:00.713 PROBE mousedown active=true  editing=true
    10:44:00.716 PROBE click     active=false editing=true

A click's default is to move the page focus to whatever lies under the
pointer, which on this window is never the invisible field. Enter and
Escape are both read from the window, so both live through it - Enter
was measured landing after the click, which is where the report and the
harness differ - and everything read from the field dies: the arrows,
Backspace, and every letter. Measured by what a review submits: typed
text reached the field without a click and was dropped after one.

Fix, in `ui/pill.ts`: refuse the default of a `mousedown` while a review
is editable, so no click can take the field, and take the field again
when the window is handed the keyboard by something other than a click.

### Evidence

Harness: Xvfb `:77`, i3 with `focus_follows_mouse yes` and
`focus_on_window_activation smart`, pointer over a terminal under the
pill, every process stopped by recorded PID.

- Three fresh processes, first turn straight into review, no click
  anywhere: keyboard on the pill through listening and review, pill
  `input=1`, box `input=0`, typed text reaching the field, Enter
  sending, a second turn ending on Escape, zero verdict warnings.
- The recovery click: submitted text is `*sad music*ZZZ` after the fix
  where it was `*sad music*` before it.

### Not fixed, and known

i3 records the box as its focused container while the box refuses the
keyboard. Nothing takes the keys during a review, but a window that
maps and goes while a review is up - a notification that takes focus -
leaves i3 restoring focus to the box, which never answers, and the
keyboard lands nowhere until the next raise. This is not new: before
the fix that restore reached a box that answered and swallowed the keys
just the same. It is a hole to close on its own terms, not by letting
the box advertise input again.

### Checks

`cargo fmt --check`, `cargo clippy --all-targets`, `cargo test -p
scufris-desktop` (135), `cargo build -p scufris-desktop`, `tsc -p
ui/tsconfig.json`, `npm run typecheck`, `npm run format:check`,
`TMPDIR=/tmp npm test` (126, up from 124: one test for each half of the
page fix, both failing before it).

### Unverified

Alex's live rerun of his exact flow on his own desktop.
