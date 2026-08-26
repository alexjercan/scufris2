# A window that takes focus and dies mid-review strands the keyboard

- STATUS: CLOSED
- PRIORITY: 80
- TAGS: desktop, bug

## Goal

Close the hole left open by task `20260826-131704`, recorded there and
in the review skill's red-team lane.

A window that maps, takes the keyboard, and goes away while a review is
up leaves the keys nowhere. i3 records the transcript box as its
focused container whenever the box is up, so when the predator dies i3
restores focus to the box - and the box refuses the keyboard by
contract, so it never answers the `WM_TAKE_FOCUS` offer. The X focus
reverts to `PointerRoot`, no client holds it, and every key the person
presses at the words on their screen goes nowhere. Nothing recovers it
until the next raise, which needs a decision, which needs a key.
`xmessage` reproduces it.

## Scope

- The runtime notices, on its own, while a phase needs the keyboard.
  Nothing else can: no key of the person's can reach a runtime whose
  window has no keys.
- Only a keyboard nobody is using is taken back. A window that took
  the keyboard and is still there has it because the person put it
  there; that stays the existing behaviour, where the next decision
  raises the pill again.
- The recovery must not poison the handoff. `_NET_ACTIVE_WINDOW` names
  a window of the companion's own while the box is up - measured as the
  pill, see below - so a capture on the recovery path would record the
  companion as the window to give the desktop back to.

## Diagnosis and fix (2026-08-26)

Measured on the harness, with a restored transcript as the review: the
pill holds the keyboard, `xmessage` maps and takes it, and the moment
its process is killed the display answers `PointerRoot` - no client
holds the keyboard at all, and it stays that way for as long as the
box is up. i3 has moved its focused container back, but the box
refuses the keyboard by contract, so i3 only offers `WM_TAKE_FOCUS`
and never calls `XSetInputFocus`. Nothing on the desktop is being
typed into.

Nothing outside the runtime can report this. The person is looking at
their own words with Enter and Escape on the table, and every key they
press reaches nothing, so there is no key left to ask with. The repair
chain does not cover it either: it runs while a window has not reached
its posture and stops once it has, and this pill reached everything it
was asked for before the keyboard was taken.

Fix, in three parts:

- `display::nobody_holds_the_keyboard` asks the display whether the
  keyboard is on any window at all. `XGetInputFocus` answers 0 for
  `None` and 1 for `PointerRoot`; both are answers about the screen
  rather than about a client. A display nothing can be asked of
  answers `Unsure`, which is never a reason to take anything.
- `App::watch` looks every 400ms for as long as the newest phase needs
  the keyboard, and takes it back only when the pill has lost it and
  nobody holds it. A window the person moved to keeps the keyboard:
  they put it there, and a pill that fought them for it would be worse
  than the hole. That case is already covered by the next decision
  raising the pill. The watch stops itself the moment the phase stops
  needing the keys.
- `FocusTracker::capture` refuses to record a window of the
  companion's own. The recovery shows the pill while the box is up,
  and the window manager calls one of our own windows active at that
  moment - measured as the pill - so an unguarded capture would record
  the companion as the window to give the desktop back to.

## Verification

Three runtime tests, each failing before its own half of the fix:
a keyboard left on nothing is taken back with no key pressed (fails
with the watch unarmed), a keyboard another window took is left where
the person put it (fails when the watch acts without asking who holds
it), and the watch stops when the phase stops needing the keyboard.
Two more for the capture rule and one for the two focus answers that
name no window. Suite 141, up from 135.

Harness: Xvfb `:78` plus i3 with `focus_follows_mouse yes` and
`focus_on_window_activation smart`, a window standing in for the
person's with the pointer parked over it, a restored transcript as the
review, no daemon and no microphone. Every process stopped by recorded
PID. Three runs, reading the keyboard from the X server itself:

- The predator dies. Before the fix: pill, predator, then `PointerRoot`
  and still `PointerRoot` five seconds later. After: pill, predator,
  `PointerRoot`, and back on the pill, with one line in the log -
  `the keyboard was left on nothing, so the pill takes it back`.
- The predator stays. The keyboard stays on it for as long as it is
  alive, and the runtime says nothing.
- The turn ends after a recovery. Escape reaches the pill, and the
  desktop goes back to the person's window.

What the harness proves about keys is window-level: Escape and Enter
reach the pill after the recovery. Typing is field-level, and it is
not measurable in this phase - an uncertain retained transcript
resends the stored words and never reads the field (`state.rs`
`Delivery::Uncertain` plus `Event::Enter`), which a control run
without any focus loss shows the same way. It rests instead on the
page's own window-focus handler, which takes the field back and
restores the caret whenever the desktop hands this window the keyboard
without a click: that is exactly what the watch does, and it is
covered by the tests from task `20260826-131704`.

The capture guard has no harness proof of its own: activating a hidden
window is a no-op that leaves i3 on its previous container, so the
handoff above survived without it. What is measured is the reason for
it - the window manager calls the pill the active window at the moment
of the recovery - and the rule is pinned by its unit test.

Checks: `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test
-p scufris-desktop` (141), `cargo build -p scufris-desktop` with its
`build.rs` tsc, `npm run typecheck`, `npm run format:check`,
`TMPDIR=/tmp npm test` (126).

Alex confirmed it live (2026-08-26): a window that takes the keyboard
and dies gives it back to the box on its own - closing the terminal
that had ended up with his keys put them straight back in the
transcript field.

## Where the boundary sits, and why it stays there

Reviewed with Alex on the same day, after he hit the other side of it:
a window that takes the keyboard and stays keeps it, and Super+D is
the way back to the box. He was offered a wider rule - the box owning
the keyboard for as long as it is up, taking it back from any window
inside about half a second - and kept the narrow one: a keyboard
nobody holds is taken from nobody, and anything more is the pill
fighting the person for their own desktop.

The wider question is not answered here at all. Task `20260825-153746`
makes the pill's keys work without focus, through an i3 binding mode
and a `no_focus` rule, which removes the contest rather than deciding
it. Real use before then is what would justify revisiting this sooner.

Closed.
