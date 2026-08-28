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
tasks tick; food rows and notes stay with the command.

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

Named by subject rather than hash: the row sits in the commit it names, so a
hash written here is one the commit cannot carry.

## Verification

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

## Left

Writing a task from the panel, which the user asked for and this does not do.
The backend already takes it - `{"action": "add", "text": ...}` runs
`today task add` for the selected day, and it is tested - but nothing on screen
sends it. Two things stand between here and there, and both are decisions
rather than work:

**A widget cannot ask the companion for anything.** `ctx.send` becomes
`Act::Send { surface, action }` and goes to the backend's standard input
(`widgets/runtime.rs:666`, `widgets/mod.rs:698`). A "+" tick needs a class of
action the widget layer keeps rather than forwards, which is a new rule in the
widget contract and needs its own answer to which widget may ask for what.

**The textbox has one meaning and it is a careful one.** It is raised and
hidden from `Phase`, and `Phase` is the machine that guarantees a transcript is
never lost and never sent twice - `Sent`, `Retained`, `Delivery`, `warned`.
Giving `Editing` a second meaning puts an unrelated concern inside it. Keeping
the flag off `Phase` instead means a textbox that is up for a reason `settle`
does not know about, and `settle` is what takes it down.

Neither is hard. Both change something load-bearing, so they are the next
increment rather than the tail of this one - which is what the plan said when
it called this step separable.
