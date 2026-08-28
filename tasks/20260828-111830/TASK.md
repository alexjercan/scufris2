# Give Scufris a workspace gesture: tap to show, hold to talk, Escape that keeps it

- STATUS: CLOSED
- PRIORITY: 90
- TAGS: desktop, ux

## Why

The companion has one door and the microphone is behind it.

`Event::Activate` is the only thing that clears `dismissed` (`state.rs:557`),
and from `Resting` it is also `StartRecording` (`state.rs:566`). So the only
way to look at the widget layer is to start recording and then cancel. Every
Escape out of an active phase calls `close()`, which rests the phase and
dismisses the pill in one move (`state.rs:851`), and `hide_pill` takes the
widget layer with it (`main.rs:154`). Cancelling a take you did not mean to
start therefore tears down the panels you were reading.

`Posture::Passive` - pill on screen, resting, holding no keyboard - already
exists (`state.rs:432`). Nothing can ask for it. The state the person wants is
already modelled and has no door.

## The gestures

| Gesture                    | Means                                          |
| -------------------------- | ---------------------------------------------- |
| Hotkey tapped              | The workspace comes up, or goes away. No mic.  |
| Hotkey held                | Push to talk. Release ends the take.           |
| Escape, with a workspace   | Cancel the take. The workspace stays.          |
| Escape, with nothing in it | Cancel the take and put the pill away.         |
| Escape again               | Put the pill away.                             |
| Stop key                   | Unchanged.                                     |
| Tray voice, `ctl open`     | Unchanged: start recording, ask again to stop. |

Tap and hold are safe to build on. `global-hotkey` 0.8's X11 backend asks for
`DETECTABLE_AUTO_REPEAT` and guards on `state.pressed`, so a held key sends one
`Pressed` and one `Released` rather than a stream of pairs.

## Steps

1. **`Event::Reveal` and `Event::Dismiss`.** Bring the workspace up, and put it
   away, without touching the phase and without the microphone. Both only in
   the passive phases: a tap must never throw away words on screen. `App` gains
   the toggle that picks between them from its own posture.
2. **`Event::Cancel`.** Escape that leaves the workspace standing. The phases
   answer one verb; whether the pill goes with it is the difference, so
   `close` takes that as an argument rather than the table growing a second row
   per phase.
3. **`Surface::holding`.** Whether there is anything in the layer worth staying
   for. `App` maps a real Escape to `Cancel` only in the phases that hold
   something to cancel and only when the layer is holding something, so the
   ladder ends at a dismissal rather than looping.
4. **Tap and hold in the hotkey handler.** `Pressed` starts a hold; a hold that
   outlives the threshold is `Activate` and its `Released` is the second
   `Activate` that stops the take. A `Released` before the threshold is a tap.
   Cancel and stop stay press-only.
5. **`scufris-ctl show` and `hide`.** The same two events over the command
   socket, so a desktop can bind whatever key it likes without the companion
   grabbing a second accelerator all session. This is also what makes the flow
   drivable from a terminal.
6. **An exhibit that answers a running turn raises the layer.** Routed in the
   link callback, before either lock is taken. A summon does not, and neither
   does an exhibit that arrives while the assistant is idle.
7. **The layer starts where the pill starts.** `Runtime.hidden` is `false` by
   default while the pill starts dismissed, so before the first dismissal the
   layer is up without a pill and after it the layer follows. Same visual state,
   two behaviours.

## Decided

The grace is not frozen while a kept workspace is on screen. The existing
freezes are bounded - speech ends, the microphone closes - and "the person kept
the workspace" is not: it would leave panels up until something else moved. An
exhibit belongs to the turn that opened it and retires with it. A panel worth
keeping is one the person pins, which is what pinning is for.

## Out of scope

Voice activity detection, so that the hotkey arms the microphone rather than
opening it. Discussed and deferred: it needs tuning, and a microphone that is
sort of open is the thing people distrust. Revisit if the flow still reads
wrong after this.

## Verification

- The state machine's own tests for the three new events and the ladder.
- `App` tests for the toggle and for the Escape mapping, both ways of
  `holding`.
- Live on staging: tap to bring the workspace up with no recording, hold to
  talk, Escape out of the textbox with a panel up and without one, and an
  exhibit arriving while the layer is down.

## Outcome

Done. All seven steps landed.

The state machine gained `Cancel`, `Reveal`, and `Dismiss`. `Cancel` is
`Escape` with a `keep` flag threaded into `close`, so no phase grew a second
row for it. `Reveal` and `Dismiss` answer only from the phases that hold
nothing, which is what makes the tap safe on the key that is pressed most.

`Surface::holding` is the layer's answer to a question the phases cannot ask.
`App::escapes_to` maps a real Escape to `Cancel` only in `Watched` and
`Editing` and only when the layer is holding something, so the ladder ends at a
dismissal rather than looping. `App::workspace` picks `Reveal` or `Dismiss`
from its own posture.

`keys::Hold` is where a press waits to learn which gesture it was. It counts
presses, so a timer that woke after its own press ended cannot open the
microphone for the press after it. `HOLD` is 250ms.

`scufris-ctl show` and `hide` are the same two events over the command socket,
for a desktop that would rather bind its own key than let the companion grab a
second accelerator all session.

An exhibit that answers a running turn raises the layer, decided by `answers`
in `main.rs` and dispatched off the link reader thread the way the conversation
window already was. A summon does not, and neither does an exhibit that arrives
with nothing running.

The layer now starts concealed, said at startup rather than as the runtime's
default: the runtime is a layer of panels and has no opinion about a pill.

### Checks

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`
  clean.
- `cargo test --workspace`: 363 pass, up from 348. Fifteen new: four for the
  new events and the ladder, four for `App`, five for `keys::Hold`, one for
  `Runtime::holding`, and one for `answers`.
- `npm run check` clean.
- `nix flake check` clean.
- Live on a real display: see [live-run.md](live-run.md). Every row in the
  gesture table above was driven through `xdotool` and read back off the
  display, including the pair that separates an exhibit answering a turn from
  one arriving with nothing running.

### Not done

Voice activity detection, which was out of scope going in and stays there.
