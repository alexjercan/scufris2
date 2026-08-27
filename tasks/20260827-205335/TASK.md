# Finish the conversation window: focus, stacking and tests

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, desktop, hud

## Source

Review round 1 of `20260827-081702`, range `185034a..a13cb38`. Findings
B2, B3, M3, M4, M8, m6 and m9. Full record:
`tasks/20260827-081702/REVIEW.md`.

The conversation window is the newest surface and the least finished.
Two of the review's three blockers are its own, and both are the kind
the desktop lane exists to catch: it strands the keyboard, and it draws
itself over the textbox.

## B2. It strands the keyboard when it goes away

`Hud::windows()` (`hud.rs:281`) passes only the HUD to
`FocusTracker::capture`. `DesktopSurface::windows()` (`main.rs:239`)
passes the pill, the textbox and every widget shell, and its doc comment
says exactly why: a capture that records an unfocusable window hands the
person's keys somewhere they cannot type.

`capture` records `_NET_ACTIVE_WINDOW` unless it is in the list
(`focus.rs:97`). i3 marks the pill active on map even though the pill is
built `focusable(false)`, so the pill is what gets recorded, and
`restore()` activates it. Measured on Xvfb with i3 4.25.1:
`XGetInputFocus` returns `PointerRoot` after the window is put away and
stays there. `App::look` only watches while `Posture::Editing`, so
nothing recovers it.

This is a recurrence. Round 1 of `20260825-215520` raised the identical
defect against `DesktopSurface::windows()` and it was fixed there. The
HUD was then given its own `FocusTracker` and made the same mistake.

Fix: give the two surfaces the same window list, or one shared tracker.
Prefer the shared tracker, because two lists are what let this come
back.

## B3. It is drawn over the textbox and clips the pill

`hud.rs:301` builds the window `always_on_top(false)` on the reasoning
that the pill and textbox, which are `always_on_top(true)`, will stay
above it. i3 does not stack floating windows by `_NET_WM_STATE_ABOVE`;
it echoes the state and ignores it. Last mapped wins.

Geometry, derived from the constants, on 1920x1080:

| Window  | x span   | y span   |
| ------- | -------- | -------- |
| HUD     | 580-1340 | 260-820  |
| textbox | 650-1270 | 614-754  |
| pill    | 865-1055 | 778-1008 |

The textbox is entirely inside the HUD. The pill's top 42 px are
covered; on 1366x768 it is 198 px, and the privacy ring can sit behind
another window with the microphone open.

The worst of it is in `Posture::Editing`. A pill click raises the HUD
over the take the person is editing, takes its keyboard, and leaves
`state.rs` in `Editing` with `App::screen()` at `Screen::Ready`. The
repair chain does not fire because `falls_short` is false at `Ready`,
and the watch does not fire because the HUD holds the keys. The
companion sits there with an invisible, keyless textbox.

### Direction, from Alex

Draw the textbox on top, and move the HUD up a bit.

Both halves are needed. They cover different cases, and neither one
covers the other's:

| Monitor   | HUD y to clear the pill | HUD y to clear the textbox |
| --------- | ----------------------- | -------------------------- |
| 1920x1080 | 194                     | 30                         |
| 1600x900  | 14                      | -150, impossible           |
| 1366x768  | -118, impossible        | -282, impossible           |

Moving the HUD up clears the pill on 1920x1080 with room to spare, and
only just fits at 1600x900. It cannot clear the pill at 1366x768: the
HUD is 560 tall and the pill top is at 466. Restacking is the only thing
that works at every size for the textbox, because the HUD is taller than
the space above it on anything below 1080p.

So: place the HUD clear of the pill where there is room, and raise the
textbox over the HUD because there never is room for that one.

### Work for B3

- Bound the HUD placement against the pill band in `hud::center`
  (`hud.rs:86`): `y = min(centred, pill_top - height - GAP)`, clamped to
  the monitor origin. Take the pill band from the pill's own constants
  rather than repeating them.
- Consider making the 560 px height responsive where the bound clamps,
  so 1366x768 degrades to a shorter window rather than to an overlap.
  Decide this when the placement test is written.
- Re-assert the pill's and the textbox's stacking after the HUD maps.
  i3 ignores the hint, so this must be an active raise through the
  display module, not a state change, and it must run on every HUD
  raise.
- Settle the keyboard, which restacking alone does not: raising the HUD
  during `Posture::Editing` takes the textbox's keys even when the
  textbox is visually on top. Either refuse the raise while `Editing`,
  or return the keyboard to the textbox after the raise. Record which,
  and why, with this task.
- Give `hud.rs` the placement test `textbox.rs:483` already has, covering
  1920x1080, 1600x900 and 1366x768.

## M3. It reads the toolkit for whether it is up

`up()` (`hud.rs:318`) reads `window.is_visible()`. The desktop lane's
first law is that a Tauri call is a message onto the GTK loop, and
verdicts come from the X server through `display.rs`. The flag survives
i3 unmapping the window on a workspace switch, so on any workspace but
the one it was left on, `toggle` takes the hide path.

`hide()` (`hud.rs:225`) then calls `focus.restore()` unconditionally,
with no counterpart to the `holds_keyboard` guard `show()` has at
`hud.rs:199`.

Measured: window up on workspace 1, person on workspace 2. One press of
the binding answers `taken`, shows nothing, and activates the textbox,
which pulls i3 back to workspace 1. The first press of "show me the
conversation" moves the person off the workspace they were on. Without
workspaces the same unguarded restore is a plain focus steal.

Fix: read the display the way every other window in the crate does, and
restore only when the window actually held the keyboard.

## M4. The page throws typed words away before the host decides

`ui/hud.ts:132` sets `words.value = ""` and then invokes `hud_submit`.
`Hud::typed` (`hud.rs:237`) returns early with no `tell()` when
`Conversation::typed` refuses a second line while one is in flight
(`conversation.rs:126`). Nothing comes back: no transcript entry, no
notice, no trouble, no log line.

Three places state the opposite, and it is the whole justification for
refusing rather than queueing. `hud.rs:239`: "The words are still in the
field either way." `conversation.rs:125`: "the field still holds the
words either way." D-HUD-9 says the same.

The blank case is fine - the page guards `text.trim() === ""` before
clearing. Only the in-flight case loses a sentence, and that is the case
the design was written around.

Fix: clear the field when the host confirms the line was taken, or
refuse Enter in the page while the notice says `sending`.

## M8. 186 lines of page behaviour with no test

`tests/desktop-ui.test.ts:25` compiles the whole `ui/tsconfig.json`
project and reads `dist/pill.js` and `dist/textbox.js`. `dist/hud.js` is
built by the same call and never read; the file header still says "The
two desktop webview pages".

Untested: the follow-scroll threshold (`hud.ts:63`), the notice
precedence (`:99`), Enter against Shift+Enter (`:143`), the focus rescue
(`:154`), and the field clearing (`:126`), which is M4. The textbox has
pinned tests for its equivalents, and AGENTS.md says to add files with
their first tested behaviour. M4 is the proof this is not hypothetical.

## Also here

- **m6.** The conversation verb stalls the whole service stream.
  `link.rs:202` calls `observe` inline on the single reader thread, and
  `LinkEvent::Conversation` runs `Hud::show()`, which blocks on GTK plus
  up to 500 ms of `display::came_up` polling. For that window the
  frontend reads no transcript, no state, no answer and no widget
  command. Nothing is lost, but a `Sent` pill and a `sending` window
  both wait on answers sitting unread. `MENU_VOICE` (`main.rs:395`)
  already dispatches off-thread for this reason.
- **m9.** Three of four capability labels are unguarded.
  `every_widget_window_label_matches_the_capability_glob`
  (`widgets/windows.rs:169`) asserts `widget-*` is in
  `capabilities/default.json`. `pill`, `textbox` and `hud` each carry a
  doc comment claiming the file names them, and nothing checks. The
  existing test extends in three lines.

## Proof

- Placement test in `hud.rs` at three monitor sizes, asserting no
  overlap with the pill band.
- A stacking check on Xvfb with i3: raise the HUD, then read `XQueryTree`
  and confirm the textbox and pill are above it.
- Focus check on Xvfb: put the window away and assert `XGetInputFocus`
  lands on the window that held the keyboard before, not `PointerRoot`
  and not the pill.
- Workspace check for M3: window up on workspace 1, person on workspace
  2, one press of the binding, assert the person stays on workspace 2.
- `tests/desktop-ui.test.ts` reads `dist/hud.js` and pins the five
  behaviours listed under M8.
- `cd native && TMPDIR=/tmp nix develop --offline -c cargo test
--workspace`, and `TMPDIR=/tmp npm test`.

One lane holds the X display at a time.

## Outcome (2026-08-27)

Done, with one decision recorded below and one residual stated.

### B2

One list, derived rather than passed. `focus::own_windows(app)` names the
pill, the textbox, the conversation window and every widget shell, and
both trackers call it. `Hud::windows()` and `DesktopSurface::windows()`
are gone. Two lists were what let the second one be short, and deriving
the list from the app keeps it right for a surface nobody has added yet.

### B3

Alex's direction, and both halves are in, but the second one is
implemented as a refusal rather than an X restack. The reason:

- **Move the HUD up.** `hud::center` bounds the bottom edge against the
  pill band instead of centering blind. It stands clear of the pill at
  1920x1080, 2560x1440 and 1600x900, and only moves as far as it has to -
  on a monitor with room to spare it stays centered.
- **Draw the textbox on top.** Restacking through X would have put the
  box in front and left the keyboard behind: `raise` calls `set_focus`,
  so the HUD takes the keys whatever the stacking order says, and the
  person types into a box they can see that is not receiving. That is
  the state B3 describes, minus the invisibility. So the box wins the
  band outright: `Hud::show` refuses while the box is up. The box is up
  only while there is a take in it, and one take is the shortest-lived
  thing on this desktop.

  This also settles the stuck state B3 names. Nothing enters `Editing`
  with the keyboard elsewhere, so the repair chain that does not run at
  `Screen::Ready` never has to.

  The unverifiable part is why it was not done the other way: whether i3
  honours `_NET_RESTACK_WINDOW` for a floating window is not something
  this machine can answer without a display, and the review's own
  measurement is that i3 ignores the stacking hint it already echoes.

**Residual, stated rather than left to be found.** 560 of window, 230 of
pill, a 72 margin and a 24 gap come to 886, and a 768-tall monitor does
not have it. Below about 1600x900 there is no position that clears the
pill, so the window takes the top of the monitor: 94 pixels of the pill
behind it at 1366x768, against 198 before. Pinned by
`the_window_never_covers_more_of_the_pill_than_it_has_to`.

A responsive height would remove that last 94. Not done: the window
carries equal min and max size hints, which is what makes a tiling window
manager float it, and changing them while the window is up is the class
of thing this whole task is fixing. It is a real option and it is not
free.

### M3, M4, M8, m6, m9

- `hud::up` asks the display. Only the toolkit's positive answer is used
  when nothing can be asked. This is what made one press of the binding
  on another workspace show nothing and pull the person back.
- `hide` restores the keyboard only when the window held it, read before
  it gives it up. `show` already guarded its capture the same way.
- `hud_submit` answers whether the line was taken, and the page clears
  the field on that alone. Refusing a second line rather than queueing it
  is only acceptable because the words stay in the field, and they did
  not.
- `tests/desktop-ui.test.ts` reads `dist/hud.js` and pins all five: the
  follow-scroll threshold at its exact boundary, the notice precedence,
  Enter against Shift+Enter, the focus rescue, and the field clearing.
- m6: `LinkEvent::Conversation` dispatches off the link's reader thread.
- m9: the capability test covers all four labels, not only the widget
  glob.

### Proof

- `cargo test --workspace`: 336 pass, 0 fail. `cargo fmt --check` and
  `cargo clippy --workspace --all-targets` clean.
- `npm test`: 80 pass, 0 fail (23 in `desktop-ui`, up from 16).
- `npm run check` clean.

### Not done

No X display was used. The placement, the refusal and the page behaviour
are all pinned by unit tests; the stacking and focus outcomes on a real
i3 are not measured here and are part of the live acceptance
`20260827-081702` still has open.
