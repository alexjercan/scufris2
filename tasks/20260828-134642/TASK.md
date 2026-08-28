# Bring the den onto the workspace: three today widgets

- STATUS: IN_PROGRESS
- PRIORITY: 90
- TAGS: desktop,widgets,today

## Ask

Three panels reading the-den journal: notes, tasks with habits and what is
upcoming, and macros with weight. And a home for the widgets, which sat as two
top-level directories beside the crates.

Scoped with the user:

- Widgets only this round. No Pi tools wrapping today's subcommands.
- Weight gets a trend. Calories do not - `today` has no calories history
  command and the user chose not to add one.
- Panels may be ticked and written to, not only read.

## Decisions

**One roof.** `native/widgets/` and `native/backends/` are two halves of one
thing. Both moved under `native/scufris-widgets/`. Not a Cargo crate: it holds
no Rust, and one that builds nothing would be worse than the split it replaces.

**A manifest may say how its backend stands up.** `summon` opens a widget with
an empty payload, so three widgets over one backend could not tell it apart. A
`spawn` table in `widget.toml` is laid _under_ whatever the open carried, at
`opening()` - the one point both roads into an open pass through.

**`today` is asked, never imitated.** The backend shells out and shapes the
answer. It never parses the-den, so the format lives in one place.

**`path`, not `show`, to look at a day.** `today show` calls `read_day` with
`create=True` (`today/application.py:158`), so it makes the entry it reads. A
panel browsing a month with it would leave a month of empty files behind. The
backend asks `today path`, stats it, and only runs `show` when the file is
there.

**One `upcoming` call serves the marks and the list.** It is asked from the day
before the _earlier_ of today and the selection, so a past day still shows what
stands between it and now, and the calendar's dots and the "ahead" list come
out of the same answer.

**Ticking, but no typing.** A click needs no focus. A widget shell is built
unfocusable on purpose (`widgets/windows.rs:9-16`) and pooled, so one that ever
became focusable would stay that way for whatever exhibit reused it. Habits and
tasks tick with a click, and the words go somewhere else - see the next four.

**A fourth window, not a fifth meaning for the textbox.** The textbox is raised
and hidden from `Phase`, and `Phase` is what guarantees a transcript is never
lost and never sent twice. A second meaning inside it puts an unrelated concern
in the one machine that must not grow one. `src/form.rs` is a window of its own,
built to the HUD's recipe - focusable, on top, its own `FocusTracker`,
`set_focusable(true)` before every raise because i3 rereads hints at map time -
and it knows nothing about `Phase`. It refuses to come up while the textbox is
up, which is the rule the HUD already keeps and for the same reason.

**`ctx.ask`, not a widget that raises a window.** A widget asks for a question
to be put; the companion decides whether to put it. `Ask::parse` is the whole
gate - at most four fields, twelve lines each, title and labels clipped - and it
is in Rust because `SCUFRIS_WIDGET_PATH` can install a widget that was never in
this build. `Ask::fill` then copies only the fields the ask declared, so a page
cannot name an argument the backend reads. The answer re-enters through
`Cmd::Sent`, the road a tick already takes: nothing past `Act::Ask` knows a
panel wrote by asking, and a refused write is refused on the same badge.

**One box, four questions.** A single field would have needed a second box for
the note, and the food needs two answers at once. So the box is 1-4 fields and
is resized per question before it maps (`Ask::height`, the same arithmetic
`ui/form.css` lays out with, with a test on it). A one-line field's answer is
flattened host-side: a task with a newline in it is not a task, while a note's
line breaks are the note.

**Food is answered, then offered.** _Superseded by the typeahead below._ The
old widget had a typeahead. A box that cannot see the panel cannot have one, so
the backend queried `today macros query` after both answers were in. One match
was logged; several were handed back as `choices` and the panel offered them as
clicks. `macros calculate` reads `MACROS_DATABASE`, so the module writes it from
`programs.scufris.desktop.macrosDatabase` beside `denPath`.

**A field may ask the backend what it could be.** The user asked for the
typeahead back, first. A `suggest` action on a one-line field is the whole
feature, and it needed no new protocol: the question goes out on `Cmd::Sent`
like any other action, and the answer arrives as an ordinary reading, which
`Form::saw` hands to the box while the box is up for that surface. So there are
no correlation ids, no second failure mode and no timeout. `Ask::look` builds
the question from the field's own declared `suggest` object, exactly as
`Ask::fill` builds the answer, so the page sends only a field name and what is
in it - `suggest` reaches the page as a bare `true`. A block field may not carry
one: prose has no candidates, and it is refused rather than drawn without the
list, because a list that silently never appears is the harder half to find out
about.

**The room for the list is reserved, not grown into.** The window is sized
before it maps and equal min/max hints are what make i3 float it, so
`Ask::height` adds `LIST_GAP + ROW * ROWS` for a suggest field whether or not
anything is in it. A box that grew as the person typed would move the field
they were typing in.

**A keystroke must not cost a day read.** `search` is handled beside `select`
and `refresh` rather than among the writes, and `choices` is laid onto the
macros reading in `read()` rather than built into the cached frame. Otherwise
one letter would cost a `show` plus a month of weights.

**A name is an id or it is one row.** `food` takes an id straight through - that
is what a taken candidate answers with - looks up anything else, and takes a
single match. Several is a sentence beside the day rather than a guess: the list
was under the field the whole time. The held-amount `pick` action and the
panel's own list are gone with it.

**A note is the way back into itself.** Clicking a note opens the same two
fields filled in and sends `edit`, which runs `today note edit`. An empty
heading keeps the one the note has - `today`'s own rule, and the right one here:
the box opened on the note as it stands.

**A place is measured from an edge, never from the middle.** The second slot on
a screen side sat halfway down it, whatever stood above. Any panel taller than a
quarter of the screen therefore overlapped its neighbour, which is every journal
panel: on 1920x1080 the agenda ended at 544 and the notes panel began at 330.
The two places now hang from opposite ends of the side, so the room between them
is what the pair leave rather than a point neither was measured against, and
neither has to be told the other's size. The shelf's pitch had the same defect -
268 between columns for windows 340 wide - and is now a lane per rank, wide
enough for the widest widget that ships. Two panels that together exceed the
side still meet in the middle; nothing can place them apart, and
`every_shipped_widget_fits_the_places_it_can_be_put_in` holds the manifests to
sizes where that cannot happen. The agenda went from 520 to 500 tall to keep
it.

**Trouble does not drop the day.** Clearing the cached frame in the `Trouble`
branch of `act` looked right and was not: the rebuild then fails under `refuse`
and the panel blanks, which contradicts trouble arriving _beside_ the day. The
candidate list is dropped explicitly where it is set instead, so a stale pick
cannot survive a later failure.

**The keyboard is the second answer, not the first.** i3 names a clicked panel
the active window while the keys stay where they were, because the panel
refuses them. That is every capture the form takes, so `FocusTracker::capture`
now asks the keyboard when the active window is one of ours - filtered through
the same `own_windows` list, and only then, so the pill and the conversation
window pay nothing for it. Found on the screen; see
[live-run-writing.md](live-run-writing.md).

**No flake input on the journal.** `nix/desktop.nix` is untouched. The
deployment names the command through
`programs.scufris.desktop.todayCommand`, and the module writes
`SCUFRIS_TODAY_COMMAND` and `DEN_PATH` into the unit. This is narrower than the
plan's wrapper-PATH prefix and needs no package change: the backend already
prefers an absolute command from the environment.

**One upstream line.** `Day.to_dict()` in `~/personal/today` dropped `foods`,
so `show --json` carried the macro totals but not the rows the user asked to
see. Added there as `today` commit `97fe85d`, additive, with the five golden
fixtures regenerated in their original one-line form. The macros widget still
renders without it - an older `today` is a shorter panel, not an error.

**The empty food list is that line, not a widget bug.** The installed `today` is
0.4.0 and `show --json` answers `date, file, habits, macros, notes, tasks,
title, weight` - no `foods`. `nix.dotfiles/flake.nix:52` pins
`github:alexjercan/today/v0.4.0`, and `97fe85d` is unreleased. So the panel's
food list stays empty until that commit is tagged and the input bumped. Nothing
to fix here: the backend must not parse the-den to work around it.

## Shipped

| Commit                              | What                                                |
| ----------------------------------- | --------------------------------------------------- |
| `Give the widgets ... one roof`     | The move, and the manifest `spawn` default          |
| `Put the journal on the workspace`  | The `today` backend, three widgets, wiring and docs |
| `Let the panels write`              | `ctx.ask`, the form window, and the four writes     |
| `Offer the database as it is typed` | The `suggest` typeahead, and rewriting a note       |

Named by subject rather than hash: the row sits in the commit it names, so a
hash written here is one the commit cannot carry.

## Verification

Reading, first increment:

- `cargo test --workspace`: 366 pass (292 companion, 48 service, 26 control).
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 72 pass, including
  20 new ones for the `today` backend. Every one runs against a temporary den
  with a stub `today` in front of it. Nothing reads the real journal.
- `npm run check`: 81 pass, Prettier clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `nix build .#checks.x86_64-linux.desktop-interface`: the unit carries
  `SCUFRIS_TODAY_COMMAND` and `DEN_PATH` when named and neither when not.
- In `~/personal/today`: 95 pytest cases pass, `ruff check` clean, `mypy` clean.
- `nix flake check`: all checks passed.
- Live on the isolated `Super+Y` rig against a copy of the-den. See
  [live-run.md](live-run.md). It caught one real bug: a journal the notes panel
  could not reach read as a day with no notes in it.

Writing, second increment:

- `cargo test --workspace`: 381 pass (307 companion, 48 service, 26 control).
  Fifteen new: nine on the form window - parse bounds, the height arithmetic,
  field filtering, one-line flattening, blank answers, three placements - one
  that a question from a widget with nothing behind it is refused rather than
  asked, two more placements for what the live run caught, and two on the
  focus tracker's second answer.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 82 pass. Ten new ones
  cover the four writes, the two-answer food flow with one match and with
  several, "none of these", a day change dropping a held pick, and `choices`
  reaching the macros view alone. Same stub, same temporary den.
- `npm run check`: 81 pass, Prettier clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `nix flake check`: all checks passed, including the desktop interface check
  asserting `MACROS_DATABASE` is written when named and not when not.
- Live on the isolated `Super+Y` rig against a copy of the-den. See
  [live-run-writing.md](live-run-writing.md). It caught two: the box was placed
  from a window that had never been mapped, and the keyboard was not given back.

The typeahead, third increment:

- `cargo test --workspace`: 385 pass (311 companion, 48 service, 26 control).
  Four new on the form window: the room a suggest field reserves, that the page
  is told `true` and never what the field asks with, the parse bounds on
  `suggest`, and what `Ask::look` builds for a declared field, an undeclared one
  and one with no list.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 91 pass. The `pick`
  tests are gone with the action. Fourteen cover the rewrite (heading given and
  left alone, an empty body, an index that is not there, an index that is not
  one), the food by id and by words, and the search - what it offers, that it
  costs one `macros query` and no day read, that an emptied field offers
  nothing, and that a day change drops the list. Same stub, same temporary den.
  The stub now matches the way `today macros query` does, by subsequence over
  the food id, so an id that is a subsequence of two other rows is a real case.
- `npm run check`: 91 pass, Prettier clean. Ten new: the form page had no tests
  at all, and the typeahead is mostly on the page - when it asks, what the list
  is read from, what a taken candidate answers with, the arrow keys and their
  wrap, Enter taking a row before it saves, and a second question leaving
  nothing of the first behind. The debounce is driven from the test rather than
  by the clock.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `nix flake check`: all checks passed.

Placement, fourth increment:

- `cargo test --workspace`: 387 pass (313 companion, 48 service, 26 control).
  Two new: that the two places on one side stand clear of each other for the
  pair that overlapped on screen, and that every shipped manifest declares a
  size the places can hold. The second is the one that matters - the layout is
  arithmetic over sizes the manifests declare, and the old assertion was against
  a card written for the test rather than against the widgets that ship.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `npm run check`: 91 pass, Prettier clean.
- `nix flake check`: all checks passed.

## Left

Three of the four writes were not driven on the screen - the weight, the food
and the note - and neither was the refusal over a take in the textbox, nor the
typeahead itself. The screen work stopped at the user's word once the two
defects above were found and fixed; everything since is covered by tests
instead, at all three levels: the backend's actions, the host's bounds, and the
page's own behaviour.

The macros panel's food list stays empty until `today` `97fe85d` is released and
`nix.dotfiles/flake.nix:52` is bumped off `v0.4.0`. That is the user's repo and
their call.
