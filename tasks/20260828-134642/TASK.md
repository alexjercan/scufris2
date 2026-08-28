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

**Food is answered, then offered.** The old widget had a typeahead. A box that
cannot see the panel cannot have one, so the backend queries `today macros
query` after both answers are in. One match is logged; several are handed back
as `choices` and the panel offers them as clicks - it has the room, and the
person already answered the amount once. `macros calculate` reads
`MACROS_DATABASE`, so the module writes it from
`programs.scufris.desktop.macrosDatabase` beside `denPath`.

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

## Shipped

| Commit                             | What                                                |
| ---------------------------------- | --------------------------------------------------- |
| `Give the widgets ... one roof`    | The move, and the manifest `spawn` default          |
| `Put the journal on the workspace` | The `today` backend, three widgets, wiring and docs |
| `Let the panels write`             | `ctx.ask`, the form window, and the four writes     |

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

## Left

Three of the four writes were not driven on the screen - the weight, the food
and the note - and neither was the refusal over a take in the textbox. The
screen work stopped at the user's word once the two defects above were found
and fixed. Each is covered by tests: ten in `tests/test_today_backend.py` for
what the backend does with the action, and the form's own for what reaches it.
