#!/usr/bin/env python3
"""The-den on the command line, for whoever is not a panel.

The desktop panels read the journal in their own process. This is the same
library with a command line in front of it, so the agent can read and write the
day the same way and by the same rules - one lock, one format, one place where
a half-written entry fails.

    scufris-den show --json
    scufris-den --date 2026-09-01 task add "call the dentist"
    scufris-den restant --json
    scufris-den backlog add "learn to weld"
    scufris-den gym split push
    scufris-den gym add "bench press" 60x8 60x8 60x6
    scufris-den gym edit "bench press" 60x8 60x8 60x7
    scufris-den weight 81.4

Reads answer in plain lines, or in JSON with `--json`. Nothing here opens an
editor: every operation is one non-interactive subcommand, because the caller
is usually not a person.

A read never creates an entry. Only a write does, and only for the day it was
asked to write.
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import date, datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import den


class Stop(Exception):
    """Something the caller should read on standard error and act on."""


def shared() -> argparse.ArgumentParser:
    """The flags every level accepts, wherever the caller puts them.

    Carried by the top parser and by every subcommand, so `--json show` and
    `show --json` are the same command. Nothing has a default here: an absent
    flag leaves its name off the namespace instead of writing a default over
    what an outer level already read.
    """
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument(
        "--den",
        default=argparse.SUPPRESS,
        help="journal directory; else DEN_PATH, else the default",
    )
    when = common.add_mutually_exclusive_group()
    when.add_argument(
        "--date", default=argparse.SUPPRESS, help="the day to work on, as YYYY-MM-DD"
    )
    when.add_argument(
        "-N",
        "--offset",
        type=int,
        default=argparse.SUPPRESS,
        help="days from the date it is",
    )
    common.add_argument(
        "--json", action="store_true", default=argparse.SUPPRESS, help="answer in JSON"
    )
    return common


def parse(argv: list[str]) -> argparse.Namespace:
    common = shared()
    parser = argparse.ArgumentParser(
        prog="scufris-den",
        description="Read and write the-den journal.",
        parents=[common],
    )
    group = parser.add_subparsers(dest="command", required=True)

    def top(name: str, **rest: str) -> argparse.ArgumentParser:
        return group.add_parser(name, parents=[common], **rest)

    top("path", help="print the day's entry without creating it")
    top("create", help="create the day's entry and print it")
    top("show", help="the whole day")

    task = nested(top("task", help="the day's tasks"), common)
    task.add_parser("list")
    task.add_parser("add").add_argument("text")
    task.add_parser("done").add_argument("index", type=int)
    task.add_parser("rm").add_argument("index", type=int)

    habit = nested(top("habit", help="the day's habits"), common)
    habit.add_parser("list")
    habit.add_parser("toggle").add_argument("name")

    late = top("restant", help="what was left undone before the day")
    late.add_argument("--days", type=int, default=den.HORIZON, help="how far back")
    top("upcoming", help="what is dated after the day")

    idea = nested(top("backlog", help="what has no day yet"), common)
    idea.add_parser("list")
    idea.add_parser("add").add_argument("text")
    idea.add_parser("done").add_argument("index", type=int)
    idea.add_parser("rm").add_argument("index", type=int)
    idea.add_parser("promote").add_argument("index", type=int)

    note = nested(top("note", help="the day's structured notes"), common)
    note.add_parser("list")
    adding = note.add_parser("add")
    adding.add_argument("body")
    adding.add_argument("--title")
    editing = note.add_parser("edit")
    editing.add_argument("index", type=int)
    editing.add_argument("body")
    editing.add_argument("--heading")
    note.add_parser("rm").add_argument("index", type=int)

    weight = top("weight", help="the day's weight, and its trend")
    weight.add_argument("value", nargs="?")
    weight.add_argument(
        "--days", type=int, default=30, help="how far the trend reaches"
    )

    food = nested(top("macros", help="the day's food rows"), common)
    food.add_parser("list")
    food.add_parser("add").add_argument("row", help="what,protein,carbs,fat")
    correcting = food.add_parser("edit", help="write one row over the one there")
    correcting.add_argument("index", type=int)
    correcting.add_argument("row", help="what,protein,carbs,fat")
    food.add_parser("rm").add_argument("index", type=int)
    logging = food.add_parser("log", help="one database food, scaled to an amount")
    logging.add_argument("name")
    logging.add_argument("amount", type=float)
    food.add_parser("query").add_argument("words")
    food.add_parser("insert").add_argument("row", help="food 100g,protein,carbs,fat")
    food.add_parser("database", help="print where the food database is")

    gym = nested(top("gym", help="the day's sets"), common)
    gym.add_parser("list")
    naming = gym.add_parser("split", help="the day's split, read or named")
    naming.add_argument("value", nargs="?")
    adding = gym.add_parser("add", help="every set of one movement")
    adding.add_argument("exercise")
    adding.add_argument("sets", nargs="+", help="60x8 60x8 60x6")
    editing = gym.add_parser("edit", help="write over every set of one movement")
    editing.add_argument("exercise")
    editing.add_argument("sets", nargs="*", help="60x8 60x8, or none to remove it")
    editing.add_argument("--rename", help="the name to keep the sets under")
    gym.add_parser("rm").add_argument("index", type=int)
    history = gym.add_parser("history", help="what was trained, newest first")
    history.add_argument("--days", type=int, default=90)
    gym.add_parser("known", help="the movements the database knows")
    learning = gym.add_parser("learn", help="add a movement to the database")
    learning.add_argument("split")
    learning.add_argument("exercise")
    gym.add_parser("forget").add_argument("exercise")
    gym.add_parser("database", help="print where the exercise database is")

    return parser.parse_args(argv)


class Nested:
    """One subcommand's own subcommands, each carrying the shared flags."""

    def __init__(self, group: object, common: argparse.ArgumentParser) -> None:
        self.group = group
        self.common = common

    def add_parser(self, name: str, **rest: str) -> argparse.ArgumentParser:
        return self.group.add_parser(name, parents=[self.common], **rest)


def nested(parser: argparse.ArgumentParser, common: argparse.ArgumentParser) -> Nested:
    """One subcommand's own subcommands, each carrying the shared flags."""
    return Nested(parser.add_subparsers(dest="what", required=True), common)


def asked(args: argparse.Namespace) -> bool:
    """Whether JSON was asked for, at whichever level it was written."""
    return getattr(args, "json", False)


def emit(args: argparse.Namespace, value: object, plain: str | None = None) -> None:
    """Answers in JSON when asked, and in one plain line when not."""
    if asked(args):
        print(json.dumps(value, ensure_ascii=False))
    elif plain is not None:
        print(plain)


def lines(args: argparse.Namespace, value: object, plain: list[str]) -> None:
    if asked(args):
        print(json.dumps(value, ensure_ascii=False))
        return
    for line in plain:
        print(line)


def written(day: den.Day) -> str:
    return f"{day.date}: {day.file}"


def read(den_path: Path, when: date) -> tuple[den.Day, str]:
    return den.read_day(den_path, when, create=False)


def apply(den_path: Path, when: date, rewrite) -> den.Day:
    """One change, against the day as it is at this moment."""
    _day, current = read(den_path, when)
    try:
        changed, _after = den.change(den_path, when, current, rewrite)
    except den.Conflict as trouble:
        raise Stop(str(trouble)) from None
    except (LookupError, IndexError, ValueError) as trouble:
        raise Stop(str(trouble)) from None
    return changed


def tasks(args: argparse.Namespace, here: Path, when: date) -> None:
    if args.what == "list":
        day, _current = read(here, when)
        lines(
            args,
            [task.to_dict() for task in day.tasks],
            [
                f"{task.index}. [{'x' if task.done else ' '}] {task.text}"
                for task in day.tasks
            ],
        )
        return
    if args.what == "add":
        day = apply(here, when, lambda text: den.add_task(text, args.text.strip()))
    elif args.what == "done":
        day = apply(here, when, lambda text: den.toggle_task(text, args.index))
    else:
        day = apply(here, when, lambda text: den.remove_task(text, args.index))
    emit(args, day.to_dict(), written(day))


def habits(args: argparse.Namespace, here: Path, when: date) -> None:
    if args.what == "list":
        day, _current = read(here, when)
        lines(
            args,
            [habit.to_dict() for habit in day.habits],
            [f"[{'x' if habit.done else ' '}] {habit.name}" for habit in day.habits],
        )
        return
    day = apply(here, when, lambda text: den.toggle_habit(text, args.name))
    emit(args, day.to_dict(), written(day))


def backlog(args: argparse.Namespace, here: Path, when: date) -> None:
    ideas, current = den.read_backlog(here)
    if args.what == "list":
        lines(
            args,
            [idea.to_dict() for idea in ideas],
            [
                f"{idea.index}. [{'x' if idea.done else ' '}] {idea.text}"
                for idea in ideas
            ],
        )
        return
    if args.what == "promote":
        # The day's revision is read here rather than inside, because promote
        # writes two files and both have to be the ones the caller looked at.
        _day, day_revision = read(here, when)
        try:
            day, _after, left, _revision = den.promote(
                here, args.index, when, current, day_revision
            )
        except (den.Conflict, IndexError, ValueError) as trouble:
            raise Stop(str(trouble)) from None
        emit(
            args,
            {"day": day.to_dict(), "backlog": [idea.to_dict() for idea in left]},
            written(day),
        )
        return
    if args.what == "add":
        rewrite = lambda text: den.add_idea(text, args.text.strip())
    elif args.what == "done":
        rewrite = lambda text: den.toggle_idea(text, args.index)
    else:
        rewrite = lambda text: den.remove_idea(text, args.index)
    try:
        left, _revision = den.change_backlog(here, current, rewrite)
    except (den.Conflict, IndexError, ValueError) as trouble:
        raise Stop(str(trouble)) from None
    emit(args, [idea.to_dict() for idea in left], str(den.backlog_path(here)))


def notes(args: argparse.Namespace, here: Path, when: date) -> None:
    if args.what == "list":
        day, _current = read(here, when)
        lines(
            args,
            [note.to_dict() for note in day.notes],
            [f"{note.index}. {note.heading}\n{note.body}" for note in day.notes],
        )
        return
    if args.what == "add":
        try:
            heading = den.note_heading(args.title, datetime.now().astimezone())
        except ValueError as trouble:
            raise Stop(str(trouble)) from None
        body = args.body.strip()
        if not body:
            raise Stop("a note with nothing in it is not a note")
        day = apply(here, when, lambda text: den.add_note(text, heading, body))
    elif args.what == "edit":
        day, _current = read(here, when)
        if args.index < 1 or args.index > len(day.notes):
            raise Stop(f"note {args.index} not found")
        heading = args.heading or day.notes[args.index - 1].heading
        body = args.body.strip()
        if not body:
            raise Stop("a note with nothing in it is a note to remove")
        day = apply(
            here, when, lambda text: den.edit_note(text, args.index, heading, body)
        )
    else:
        day = apply(here, when, lambda text: den.remove_note(text, args.index))
    emit(args, [note.to_dict() for note in day.notes], written(day))


def weight(args: argparse.Namespace, here: Path, when: date) -> None:
    if args.value is not None:
        try:
            value = den.normalize_weight(args.value)
        except ValueError as trouble:
            raise Stop(str(trouble)) from None
        day = apply(here, when, lambda text: den.set_weight(text, value))
        emit(args, day.to_dict(), f"{day.date}: {value} kg")
        return
    trend = den.weight_history(here, when, args.days)
    change = round(trend[-1][1] - trend[0][1], 1) if len(trend) > 1 else None
    day, _current = read(here, when)
    lines(
        args,
        {
            "weight": day.weight,
            "change": change,
            "recent": [{"date": at, "weight": value} for at, value in trend],
        },
        [f"{at}  {value} kg" for at, value in trend],
    )


def macros(args: argparse.Namespace, here: Path, when: date) -> None:
    if args.what == "database":
        print(den.resolve_database(None, here))
        return
    if args.what == "query":
        try:
            found = den.Database.load(den.resolve_database(None, here)).query(
                args.words
            )
        except ValueError as trouble:
            raise Stop(str(trouble)) from None
        lines(
            args,
            [item.choice() for item in found],
            [f"{item.id}\t{item.label}" for item in found],
        )
        return
    if args.what == "insert":
        try:
            item = den.insert_item(den.resolve_database(None, here), args.row)
        except ValueError as trouble:
            raise Stop(str(trouble)) from None
        emit(args, item.to_dict(), item.to_row())
        return
    if args.what == "list":
        day, _current = read(here, when)
        lines(
            args,
            {
                "macros": day.macros.to_dict(),
                "foods": [food.to_dict() for food in day.foods],
            },
            [f"{food.index}. {food.name}" for food in day.foods]
            + [f"{day.macros.calories} kcal"],
        )
        return
    if args.what == "log":
        database = den.Database.load(den.resolve_database(None, here))
        found = database.query(args.name)
        wanted = args.name.strip().lower()
        exact = [item for item in found if item.id == wanted]
        if exact:
            chosen = exact[0]
        elif len(found) == 1:
            chosen = found[0]
        elif not found:
            raise Stop(f"no food matching {args.name}")
        else:
            names = ", ".join(item.id for item in found[:5])
            raise Stop(f"{len(found)} foods match {args.name} - pick one: {names}")
        try:
            row = database.calculate(chosen.id, args.amount).to_row()
        except ValueError as trouble:
            raise Stop(str(trouble)) from None
        day = apply(
            here,
            when,
            lambda text: den.add_row(text, "macros", den.normalize_food(row)),
        )
        emit(args, day.to_dict(), f"{day.date}: {row}")
        return
    if args.what in ("add", "edit"):
        try:
            row = den.normalize_food(args.row)
        except ValueError as trouble:
            raise Stop(str(trouble)) from None
        if args.what == "edit":
            index = args.index
            day = apply(
                here, when, lambda text: den.edit_row(text, "macros", index, row)
            )
            emit(args, day.to_dict(), f"{day.date}: {row}")
            return
        day = apply(here, when, lambda text: den.add_row(text, "macros", row))
    else:
        day = apply(here, when, lambda text: den.remove_row(text, "macros", args.index))
    emit(args, day.to_dict(), written(day))


def gym(args: argparse.Namespace, here: Path, when: date) -> None:
    book = den.resolve_exercises(None, here)
    if args.what == "database":
        print(book)
        return
    if args.what == "known":
        known = den.Exercises.load(book)
        lines(
            args,
            [move.to_dict() for move in known.moves],
            [f"{move.split}\t{move.name}" for move in known.moves],
        )
        return
    if args.what == "learn":
        try:
            move = den.learn_move(book, args.split, args.exercise)
        except (OSError, ValueError) as trouble:
            raise Stop(str(trouble)) from None
        emit(args, move.to_dict(), f"{move.split},{move.name}")
        return
    if args.what == "forget":
        try:
            move = den.forget_move(book, args.exercise)
        except (OSError, ValueError) as trouble:
            raise Stop(str(trouble)) from None
        emit(args, move.to_dict(), f"{move.split},{move.name}")
        return
    if args.what == "history":
        found = den.lift_history(here, when, args.days)
        lines(
            args,
            [session.to_dict() for session in found],
            [
                f"{session.date}  {session.split}  "
                f"{len(session.lifts)} sets  {session.volume:g} kg"
                for session in found
            ],
        )
        return
    if args.what == "split":
        if args.value is None:
            day, _current = read(here, when)
            lines(args, {"split": day.split}, [day.split] if day.split else [])
            return
        try:
            named = den.normalize_split(args.value)
        except ValueError as trouble:
            raise Stop(str(trouble)) from None
        day = apply(here, when, lambda text: den.set_split(text, named))
        emit(args, day.to_dict(), f"{day.date}: {named}")
        return
    if args.what == "list":
        day, _current = read(here, when)
        lines(
            args,
            {
                "split": day.split,
                "lifts": [lift.to_dict() for lift in day.lifts],
                "volume": round(sum(lift.volume for lift in day.lifts), 1),
            },
            [
                f"{lift.index}. {lift.exercise}  {lift.weight:g}x{lift.reps}"
                for lift in day.lifts
            ],
        )
        return
    if args.what == "add":
        try:
            sets = den.parse_sets(" ".join(args.sets))
            rows = [
                den.normalize_lift(args.exercise, load, reps) for load, reps in sets
            ]
        except ValueError as trouble:
            raise Stop(str(trouble)) from None

        def rewrite(text: str) -> str:
            for row in rows:
                text = den.add_row(text, "workout", row)
            return text

        day = apply(here, when, rewrite)
        emit(args, day.to_dict(), f"{day.date}: {'; '.join(rows)}")
        return
    if args.what == "edit":
        # The movement is written whole, so a set added and a set dropped are
        # the same call. No sets at all removes it.
        try:
            named = args.rename or args.exercise
            sets = den.parse_sets(" ".join(args.sets)) if args.sets else []
            rows = [den.normalize_lift(named, load, reps) for load, reps in sets]
        except ValueError as trouble:
            raise Stop(str(trouble)) from None
        day = apply(
            here,
            when,
            lambda text: den.set_rows(text, "workout", args.exercise, rows),
        )
        said = "; ".join(rows) if rows else f"removed {args.exercise}"
        emit(args, day.to_dict(), f"{day.date}: {said}")
        return
    day = apply(here, when, lambda text: den.remove_row(text, "workout", args.index))
    emit(args, day.to_dict(), written(day))


def main(argv: list[str] | None = None) -> int:
    args = parse(list(sys.argv[1:] if argv is None else argv))
    try:
        here = den.resolve_den(getattr(args, "den", None))
        when = den.resolve_date(
            getattr(args, "date", None), getattr(args, "offset", None)
        )
        if args.command == "path":
            print(den.entry_path(here, when))
        elif args.command == "create":
            print(den.ensure_day(here, when))
        elif args.command == "show":
            day, _current = read(here, when)
            emit(args, day.to_dict(), day.file)
        elif args.command == "task":
            tasks(args, here, when)
        elif args.command == "habit":
            habits(args, here, when)
        elif args.command == "restant":
            found = den.restant(here, when, args.days)
            lines(
                args,
                [task.to_dict() for task in found],
                [f"{task.date}  {task.text}" for task in found],
            )
        elif args.command == "upcoming":
            found = den.upcoming(here, when)
            lines(
                args,
                [task.to_dict() for task in found],
                [f"{task.date}  {task.text}" for task in found],
            )
        elif args.command == "backlog":
            backlog(args, here, when)
        elif args.command == "note":
            notes(args, here, when)
        elif args.command == "weight":
            weight(args, here, when)
        elif args.command == "macros":
            macros(args, here, when)
        elif args.command == "gym":
            gym(args, here, when)
    except Stop as trouble:
        print(f"scufris-den: {trouble}", file=sys.stderr)
        return 1
    except ValueError as trouble:
        print(f"scufris-den: {trouble}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
