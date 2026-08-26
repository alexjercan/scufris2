# Review: widgets runtime, increment 1

- ROUND: 1
- REVIEWER: five scufris-review lanes (contract, desktop, correctness,
  documentation, red team), adjudicated in one pass
- RANGE: `50c6f90..f0e56a8` - 47 files, 4853 insertions, 263 deletions
- VERDICT: CHANGES REQUESTED
- READABLE: https://claude.ai/code/artifact/8d5a43a2-4cf9-469e-ad4e-2cce9663da9c

## Verdict rationale

The design holds. The ports split, the pure runtime, the warm pool, and
the unfocusable window recipe are all sound, and the four commits do what
increment 1 said they would.

What fails is the honesty of the boundary. Five separate defects let the
companion tell the daemon that a widget is on screen when it is not, or
let it stop telling the daemon anything at all. Scufris then talks about
panels the person cannot see. That is not a polish item: the whole point
of the surface protocol is that both sides agree on what is up.

One BLOCKER is a live regression on this desktop, not a hypothetical.

## Findings

### BLOCKER

**B1. A widget window can be handed the keyboard the person was using.**
`DesktopSurface::windows()` (`main.rs:231`) returns only the pill and the
review box. `FocusTracker::capture` therefore records a widget shell as
the window to return to, and `restore_focus` hands it the keyboard. The
shell is `.focusable(false)`, so the keyboard lands nowhere the person
can type. Widget windows must be excluded from `capture` the way the
review box already is. One line.

### MAJOR

**M1. A shell that never loads kills the pool for good.**
`Pool::warm` sizes its work from `idle.len() + loading`, and only
`Pool::ready` or a build error ever decrements `loading`. A window that
builds but whose page never loads leaves `loading` at 1 forever. Two of
them and `warm` mints nothing again for the life of the process. Every
later open refuses with `no_shell`, and nothing in the log says why.

**M2. A widget can be adopted, reported open, and never shown.**
`Widgets::place` returns early when `windows::monitor` answers nothing
(`mod.rs:552`). On the `first` path that early return skips
`windows::show` entirely, so the window is sized and left hidden. The
comment there reasons about a `Move`, where leaving the window where it
is really is the better failure - it does not hold for the first show.

**M3. `widget_opened` is reported whatever the placement did.**
`runtime.rs:572` appends the report after the `Adopt`, and `perform`
only warns when `place` fails. So M2, a `show` that `came_up` rejects,
and a `fit` that the toolkit refuses all still answer the daemon with a
surface id and a success. Scufris is told the panel is up.

**M4. The catalog is silently lost when the welcome wins the race.**
`DaemonLink::start` is given a closure that calls `surfaces.announce()`
on `Connected`, and its reader thread is running before
`widgets.attach(link)` runs at `main.rs:447`. A welcome that arrives in
that window finds `self.link` empty, and `report` drops the catalog with
a `debug!`. The daemon types its widget tool from that message, so
Scufris cannot name a single widget for the rest of the session. It is a
startup race, so it will be intermittent.

**M5. A failed `fit` still ships the window.**
`Widgets::fit` warns and continues. Equal min and max hints are what make
i3 float the window (`windows.rs:9-16`); without them i3 tiles it, and
the person's workspace is rearranged by a panel that arrived mid
sentence.

**M6. Surface ids restart at `widget-1` after a companion restart.**
`PoolState.minted` starts at zero each process. `pool.rs:9-13` promises
that a label handed out twice would let an update land on whatever took
its place, and that is exactly what a restart arranges: a daemon that
outlives the companion still holds `widget-1` from the old one.

**M7. A crowded-out exhibit is never reported closed.**
`crowd_out` pushes `Unsubscribe` and `Retire` and no `WidgetEvent`.
Nothing downstream covers it either: `Act::Retire` reaches
`pool.discard`, which calls `window.destroy()`, and destroy does not
raise `CloseRequested`. So the fourth exhibit silently retires the first
and Scufris keeps updating a surface that is gone.

**M8. A late `widget_opened` leaves an untrackable panel.**
`server.ts:773` drops an answer no command is waiting for. After the five
second timeout the widget is on screen and its id exists nowhere, so
Scufris can neither update nor close it. Only the person can.

**M9. A lone surrogate in widget `data` tears down the control link.**
`encodeDaemonMessage` checks sizes and nothing else, so a model-supplied
`"\ud800"` is written out by `JSON.stringify` as a lone escape.
`serde_json` rejects it (`LoneLeadingSurrogateInHexEscape`), and
`daemon.rs:217-219` answers `Err(())`, which the supervisor treats as a
disconnect. Reconnect with backoff, a "backend unavailable" flash, and an
in-flight submission that can go uncertain.

*Not raised to BLOCKER:* it self-heals on the next connection and no
words are lost. The red team ranked it BLOCKER on the tear-down alone.

**M10. The per-surface module gate does not hold.**
`build.rs:88` reads each compiled widget from
`scufris-desktop/ui/dist/widgets/<id>/widget.js`, and `frontendDist` is
`ui/dist`. Every widget module is therefore also an ordinary asset any
window can fetch. `mod.rs:289-293` claims a page "cannot ask for a widget
it is not holding however it writes the URL". Today no privilege is
gained by taking the other road - every shell shares one capability - so
what is lost is the defence in depth and the truth of the comment.

**M11. The shelf is placed inside the transcript box.**
`SHELF_GAP` is 36 and puts a card's bottom at `pill.y - 36`. The review
box occupies `pill.y - 164` to `pill.y - 24` (`review::GAP` 24,
`review::HEIGHT` 140). Both are up at once whenever Scufris opens a panel
during a review.

**M12. The monitor is asked before the window is mapped.**
`place` calls `windows::monitor` before `windows::show`. `current_monitor`
on an unmapped window answers from its placeholder position, so on more
than one monitor the widget is placed against the wrong one.

**M13. The widget contract is declared twice.**
`desktop/widgets/widget.d.ts` and
`desktop/scufris-desktop/shell/contract.d.ts` each declare
`WidgetContext` and `WidgetView`, in two tsconfig projects, with nothing
binding them. Drift compiles clean on both sides and breaks at runtime.

**M14. `Pool::take` refills only once the pool is dry.**
A successful take does not warm. Depth goes 2, 1, 0, and the third widget
of a session pays the full build wait inside `take` - the wait the pool
exists to remove. `discard` warms, so it recovers as soon as anything
retires.

**M15. Decide-under-lock and perform-unlocked are not serialized.**
`mod.rs:120-127` explains that the sweep runs on the event loop precisely
so that no second thread performs its own acts. `Widgets::command` does
not: it runs on the daemon reader thread, and the chrome ticks run on
Tauri's command pool. Three threads reach `perform`. This is the same
class as the hotkey deadlock found live in the sibling task; a
single-threaded command queue is the shape that fixes it.

**M16. The documentation does not know widgets exist.**
`docs/src/overview.md:15` still says four extensions. `architecture.md`
never mentions `widgets/`.

### MINOR

- `widgets/index.ts:82` builds a `surfaces` map that is written, deleted,
  and cleared, and never read.
- Every widget renders twice at mount: `note/widget.ts:36` calls
  `view.update(ctx.spawn)` and `shell.ts:110` calls `deliver` with the
  same payload. `widget.d.ts` says update is called "once at mount".
- `calm.ts:15` repeats the literal `"scufris-widget-event"` instead of
  importing `WIDGET_EVENT_MESSAGE`.
- `Pool::ready` pushes to `idle` without checking for the label, so a
  page that reports twice puts one window in the queue twice.
- `Widgets::dismissed` reports `Closed` even when `retire` removed
  nothing.
- Widget answers are matched on correlation id alone. Ids restart at
  `w-1` per daemon, and `companion()` picks the most recent socket
  without recording which one was asked.
- `server.ts:736` flattens every encode failure to `invalid_command`,
  including the typed `widget_data_too_large`.
- The `Act::Retire` doc says the shell goes "back to the pool"; discard
  destroys it.
- `catalog.rs` checks that a manifest id matches its directory but never
  that the directory is a valid protocol identifier.
- `encodeDaemonMessage` validates sizes and no identifiers.
- `every_widget_window_label_matches_the_capability_glob` is a substring
  search of the whole capability file.
- `runtime.rs:2133` guards the shelf pitch against a hardcoded `250.0`
  instead of `CARD.width`.
- No widgets section in the user guide. Owed before the feature is
  announced, not before increment 4.

## Dropped

- **"The `widget-*` capability glob is not a real gate in Tauri 2.11.5."**
  Refuted. `tauri-utils` resolves `windows` to `Vec<glob::Pattern>`
  (`acl/resolved.rs:41`) and `authority.rs:460` matches the window label
  against them.
- **"The TS fixture test skips `tolerated.daemon` and `rejected.daemon`."**
  Correct by direction. The TS side never produces those lines, and
  `scufris-control/src/lib.rs:846-866` covers all six groups.
- **"`shelf_column` collapses ranks above 1."** `SHELF_SLOTS` is 3 and
  `crowd_out` holds the shelf at three, so rank 2 is the last one and
  `-1` is its column. Latent only.

## Already fixed in the range

- Pinning left two windows in one shelf column - fixed in `3534fe5`.
- `clear` took down instruments - fixed in `fbf8644`.

## Verified

Re-derived from the tree rather than taken from a lane:

- B1: `main.rs:231`, `focus.rs:57`, `review.rs:129`, `widgets/windows.rs:62`.
- M1, M14: `pool.rs` `warm`, `ready`, `take`, `discard` read in full.
- M2, M3, M12: `mod.rs:544-570`, `runtime.rs:540-575`, `windows.rs:97-116`.
- M4: `main.rs:425-450` against `mod.rs:269-282` and `mod.rs:572-580`.
- M6: `pool.rs:97-150`.
- M7: `runtime.rs:918-932` against `windows.rs:131-135`; `destroy` does
  not raise `CloseRequested`, and `main.rs:499-505` listens for nothing
  else.
- M8: `server.ts:725-785`.
- M9: `protocol.ts:161-182`, `serde_json` 1.0.151 `ErrorCode`,
  `daemon.rs:202-219`, supervisor at `daemon.rs:130-143`. Node's
  `JSON.stringify` emits the lone escape - checked.
- M10: `build.rs:86-93` against `tauri.conf.json` `frontendDist`.
- M11: `runtime.rs:44`, `runtime.rs:1045`, `review.rs:57`, `review.rs:62`.
- M13, and the double render: `widget.d.ts`, `shell/contract.d.ts`,
  `shell.ts:89-116`, `note/widget.ts:36`.
- M15: `mod.rs:128-183` against `mod.rs:299-314` and `main.rs:626-673`.
- The three dropped findings, each against the source named above.

## Skipped

Named, because a skip is not a pass:

- The backends layer (`backends.rs`, `desktop/backends/*/backend.py`) was
  read only where a finding pointed at it. Process lifetime, the shared
  flag, and restart were not audited.
- No lane ran the widgets end to end against a live daemon. Every finding
  here is derived from the source.
- The `tasks` widget's typed entry is not in this range.
- Increment 2 and increment 3 (`3534fe5`, `fbf8644` and after) were read
  only to settle whether a finding was already fixed.

## Proofs rerun

None. Nothing was changed, so nothing needed rerunning. The range's own
checks were green when it landed.

## Note on the review skill

The 2000-line cap in `.agents/skills/scufris-review/SKILL.md` did not
survive its first real subsystem. This range is 4853 insertions across 47
files. Five lanes at one increment each is the shape that worked. Task
`20260826-142245` stays open on that.
