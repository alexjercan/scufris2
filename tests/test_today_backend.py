"""What the `today` backend makes of a journal, without touching a real one.

Every test here builds a den under a temporary directory and puts a stub
`today` in front of it. Nothing in this file reads `$DEN_PATH`, and nothing
runs the real command: the backend's job is to ask `today` and shape what it
says, and that is what is measured.
"""

import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
import unittest.mock
from pathlib import Path
from types import ModuleType

BACKENDS = Path(__file__).parents[1] / "native" / "scufris-widgets" / "backends"

#: A stub standing in for `today`. It answers the subcommands the backend asks
#: for out of a plan file, records every call, and refuses what the plan tells
#: it to refuse - so a test can drive a failure without a broken journal.
STUB = '''#!/usr/bin/env python3
import json, os, pathlib, sys

home = pathlib.Path(os.environ["STUB_HOME"])
plan = json.loads((home / "plan.json").read_text())
args = sys.argv[1:]


def inside(query, target):
    """`today macros query` matches by subsequence over the food id."""
    at = 0
    for letter in target:
        if at < len(query) and letter == query[at]:
            at += 1
    return at == len(query)


with (home / "calls.log").open("a") as log:
    log.write(json.dumps(args) + "\\n")

den, date, at = None, None, 0
while at < len(args) and args[at].startswith("--"):
    if args[at] == "--den":
        den, at = args[at + 1], at + 2
    elif args[at] == "--date":
        date, at = args[at + 1], at + 2
    else:
        at += 1
rest = args[at:]
day = plan["days"].get(date)

if plan.get("refuse"):
    print("today: " + plan["refuse"], file=sys.stderr)
    raise SystemExit(1)

if rest[0] == "path":
    print(pathlib.Path(den) / "Daily" / (date + ".md"))
elif rest[0] == "show":
    print(json.dumps(day or {}))
elif rest[0] == "upcoming":
    print(json.dumps(plan["upcoming"].get(date, [])))
elif rest[0] == "weight" and len(rest) > 1 and rest[1] != "--days":
    plan["days"].setdefault(date, {})["weight"] = float(rest[1])
    (home / "plan.json").write_text(json.dumps(plan))
elif rest[0] == "weight":
    print(json.dumps(plan["weight"]))
elif rest[0] == "note" and rest[1] == "add":
    notes = plan["days"].setdefault(date, {}).setdefault("notes", [])
    title = rest[rest.index("--title") + 1] if "--title" in rest else ""
    notes.append({"index": len(notes) + 1, "heading": title, "body": rest[2]})
    (home / "plan.json").write_text(json.dumps(plan))
elif rest[0] == "note" and rest[1] == "edit":
    notes = plan["days"].setdefault(date, {}).setdefault("notes", [])
    at = int(rest[2]) - 1
    if at < 0 or at >= len(notes):
        print("today: note " + rest[2] + " not found", file=sys.stderr)
        raise SystemExit(1)
    notes[at]["body"] = rest[3]
    if "--heading" in rest:
        notes[at]["heading"] = rest[rest.index("--heading") + 1]
    (home / "plan.json").write_text(json.dumps(plan))
elif rest[0] == "macros" and rest[1] == "query":
    wanted = rest[2].lower()
    found = [row for row in plan.get("foods", []) if inside(wanted, row["id"])]
    print(json.dumps({"results": found}))
elif rest[0] == "macros" and rest[1] == "calculate":
    food = rest[rest.index("--food") + 1]
    amount = rest[rest.index("--amount") + 1]
    print(food + " " + amount + "g,1,2,3")
elif rest[0] == "macros" and rest[1] == "add":
    plan["added"] = plan.get("added", []) + [rest[2]]
    (home / "plan.json").write_text(json.dumps(plan))
elif rest[0] == "habit" and rest[1] == "toggle":
    for habit in day["habits"]:
        if habit["name"] == rest[2]:
            habit["done"] = not habit["done"]
    (home / "plan.json").write_text(json.dumps(plan))
elif rest[0] == "task" and rest[1] == "done":
    for task in day["tasks"]:
        if task["index"] == int(rest[2]):
            task["done"] = not task["done"]
    (home / "plan.json").write_text(json.dumps(plan))
else:
    raise SystemExit(2)
'''


def backend(name: str) -> ModuleType:
    """Loads one backend by path, since the directories are not a package."""
    path = BACKENDS / name / "backend.py"
    spec = importlib.util.spec_from_file_location(f"backend_{name}", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


today = backend("today")

DAY = "2026-08-28"
NEXT = "2026-08-30"


def plan() -> dict[str, object]:
    """One den's worth of answers, the shape `today --json` writes."""
    return {
        "days": {
            DAY: {
                "date": DAY,
                "habits": [
                    {"name": "Gym", "done": False},
                    {"name": "Read", "done": True},
                ],
                "tasks": [
                    {"index": 1, "text": "Ship the panels", "done": False},
                    {"index": 2, "text": "Water the plants", "done": True},
                ],
                "notes": [{"index": 1, "heading": "Idea", "body": "A calendar."}],
                "macros": {
                    "protein": 128.0,
                    "carbs": 214.0,
                    "fat": 66.0,
                    "calories": 1962,
                },
                "weight": 81.4,
                "foods": [
                    {
                        "index": 1,
                        "name": "oats 80g",
                        "protein": 10.0,
                        "carbs": 54.0,
                        "fat": 7.0,
                    }
                ],
            }
        },
        "upcoming": {
            "2026-08-27": [
                {"date": DAY, "index": 1, "text": "Ship the panels", "done": False},
                {"date": NEXT, "index": 1, "text": "Call the dentist", "done": False},
                {"date": "2026-09-04", "index": 1, "text": "Renew it", "done": False},
            ]
        },
        "weight": {
            "weight": 81.4,
            "change": -0.6,
            "recent": [
                {"date": "2026-07-30", "weight": 82.0},
                {"date": DAY, "weight": 81.4},
            ],
        },
        # The food database, which the stub searches the way `today` does. One
        # row whose name is its own word, and three that share one - including
        # a row whose id is a subsequence of the other two, which is the case
        # that makes taking a candidate from the list mean something.
        "foods": [
            {"id": "oats:g", "name": "Oats", "unit": "g"},
            {"id": "chicken:g", "name": "Chicken", "unit": "g"},
            {"id": "chicken breast:g", "name": "Chicken breast", "unit": "g"},
            {"id": "chicken thigh:g", "name": "Chicken thigh", "unit": "g"},
        ],
    }


class Den:
    """A temporary den, a stub `today`, and the environment that finds them."""

    def __init__(self, stack: unittest.TestCase, written: dict[str, object]) -> None:
        root = Path(tempfile.mkdtemp())
        stack.addCleanup(_remove, root)
        self.root = root
        self.den = root / "the-den"
        (self.den / "Daily").mkdir(parents=True)
        self.home = root / "stub"
        self.home.mkdir()
        (self.home / "plan.json").write_text(json.dumps(written))
        self.command = root / "today"
        self.command.write_text(STUB)
        self.command.chmod(0o755)

    def entry(self, date: str) -> Path:
        """Makes the day's file, the way an entry that exists looks."""
        path = self.den / "Daily" / f"{date}.md"
        path.write_text(f"# {date}\n")
        return path

    def environment(self) -> dict[str, str]:
        return {
            "SCUFRIS_TODAY_COMMAND": str(self.command),
            "DEN_PATH": str(self.den),
            "STUB_HOME": str(self.home),
        }

    def calls(self) -> list[list[str]]:
        log = self.home / "calls.log"
        if not log.is_file():
            return []
        return [json.loads(line) for line in log.read_text().splitlines()]

    def plan(self) -> dict[str, object]:
        return json.loads((self.home / "plan.json").read_text())


def _remove(root: Path) -> None:
    shutil.rmtree(root, ignore_errors=True)


class Reading(unittest.TestCase):
    """The frames one panel gets out of one den."""

    def setUp(self) -> None:
        self.den = Den(self, plan())
        self.den.entry(DAY)
        patch = unittest.mock.patch.dict(os.environ, self.den.environment())
        patch.start()
        self.addCleanup(patch.stop)
        self.journal = today.Journal({})
        # Every test picks its day rather than inheriting the machine's, so a
        # suite run after midnight reads the same as one run before it.
        self.journal.select(DAY)

    def test_the_agenda_is_the_day_then_what_comes_after_it(self) -> None:
        frame = self.journal.read("agenda")
        self.assertEqual(frame["date"], DAY)
        self.assertTrue(frame["exists"])
        self.assertIsNone(frame["trouble"])
        self.assertEqual([habit["name"] for habit in frame["habits"]], ["Gym", "Read"])
        self.assertEqual([task["index"] for task in frame["tasks"]], [1, 2])
        # Strictly after the selected day: the day's own task is above, in
        # `tasks`, and naming it twice would be naming it twice.
        self.assertEqual(
            [task["date"] for task in frame["ahead"]], [NEXT, "2026-09-04"]
        )

    def test_the_month_is_marked_from_the_day_on(self) -> None:
        frame = self.journal.read("agenda")
        self.assertEqual(frame["marks"], [DAY, NEXT, "2026-09-04"])

    def test_the_agenda_names_no_more_days_than_it_was_opened_for(self) -> None:
        journal = today.Journal({"ahead": 1})
        journal.select(DAY)
        self.assertEqual(len(journal.read("agenda")["ahead"]), 1)

    def test_the_macros_view_carries_the_day_and_the_trend(self) -> None:
        frame = self.journal.read("macros")
        self.assertEqual(frame["macros"]["calories"], 1962)
        self.assertEqual([food["name"] for food in frame["foods"]], ["oats 80g"])
        self.assertEqual(frame["weight"], 81.4)
        self.assertEqual(frame["change"], -0.6)
        self.assertEqual(len(frame["recent"]), 2)

    def test_the_notes_view_carries_only_the_notes(self) -> None:
        frame = self.journal.read("notes")
        self.assertEqual([note["heading"] for note in frame["notes"]], ["Idea"])
        self.assertNotIn("habits", frame)

    def test_a_day_with_no_entry_is_read_without_making_one(self) -> None:
        self.journal.select("2026-09-09")
        frame = self.journal.read("agenda")
        self.assertFalse(frame["exists"])
        self.assertEqual(frame["habits"], [])
        self.assertEqual(frame["tasks"], [])
        # `show` creates the entry it reads, so a panel that browsed a month
        # with it would leave a month of empty files behind.
        self.assertNotIn(
            "show", [call[-2] for call in self.den.calls() if len(call) >= 2]
        )
        self.assertFalse((self.den.den / "Daily" / "2026-09-09.md").exists())

    def test_a_selected_day_is_read_again(self) -> None:
        self.journal.read("agenda")
        self.den.entry(NEXT)
        self.journal.act({"action": "select", "date": NEXT})
        self.assertEqual(self.journal.read("agenda")["date"], NEXT)

    def test_a_day_that_is_not_a_date_is_refused(self) -> None:
        self.journal.act({"action": "select", "date": "next tuesday"})
        self.assertEqual(self.journal.read("agenda")["date"], DAY)

    def test_the_panel_goes_back_to_the_day_it_is(self) -> None:
        self.journal.act({"action": "select", "date": None})
        frame = self.journal.read("agenda")
        self.assertEqual(frame["date"], frame["today"])

    def test_an_unchanged_day_is_not_read_again(self) -> None:
        self.journal.read("agenda")
        before = len(self.den.calls())
        self.journal.read("agenda")
        self.assertEqual(len(self.den.calls()), before)

    def test_a_changed_day_is_read_again(self) -> None:
        self.journal.read("agenda")
        before = len(self.den.calls())
        entry = self.den.den / "Daily" / f"{DAY}.md"
        os.utime(entry, (0, 0))
        self.journal.read("agenda")
        self.assertGreater(len(self.den.calls()), before)


class Ticking(unittest.TestCase):
    """A tick is carried out through `today` and then read back."""

    def setUp(self) -> None:
        self.den = Den(self, plan())
        self.den.entry(DAY)
        patch = unittest.mock.patch.dict(os.environ, self.den.environment())
        patch.start()
        self.addCleanup(patch.stop)
        self.journal = today.Journal({})
        self.journal.select(DAY)

    def test_a_habit_is_toggled_on_the_selected_day(self) -> None:
        self.journal.read("agenda")
        self.assertIsNone(self.journal.act({"action": "habit", "name": "Gym"}))
        self.assertIn(
            ["--den", str(self.den.den), "--date", DAY, "habit", "toggle", "Gym"],
            self.den.calls(),
        )
        habits = self.journal.read("agenda")["habits"]
        self.assertTrue(habits[0]["done"])

    def test_a_task_is_ticked_by_its_own_index(self) -> None:
        self.journal.read("agenda")
        self.assertIsNone(self.journal.act({"action": "task", "index": 1}))
        self.assertIn(
            ["--den", str(self.den.den), "--date", DAY, "task", "done", "1"],
            self.den.calls(),
        )
        self.assertTrue(self.journal.read("agenda")["tasks"][0]["done"])

    def test_a_tick_with_nothing_to_tick_is_dropped(self) -> None:
        self.assertIsNone(self.journal.act({"action": "habit", "name": ""}))
        self.assertIsNone(self.journal.act({"action": "task", "index": "one"}))
        self.assertIsNone(self.journal.act({"action": "sing"}))
        self.assertEqual(self.den.calls(), [])

    def test_a_written_task_reaches_the_selected_day(self) -> None:
        self.journal.act({"action": "add", "text": "  Buy oats  "})
        self.assertIn(
            ["--den", str(self.den.den), "--date", DAY, "task", "add", "Buy oats"],
            self.den.calls(),
        )

    def test_a_weight_reaches_the_selected_day(self) -> None:
        self.assertIsNone(self.journal.act({"action": "weight", "value": " 81.9 "}))
        self.assertIn(
            ["--den", str(self.den.den), "--date", DAY, "weight", "81.9"],
            self.den.calls(),
        )
        self.assertEqual(self.journal.read("macros")["weight"], 81.9)

    def test_a_weight_that_is_not_one_is_said_rather_than_written(self) -> None:
        self.assertEqual(
            self.journal.act({"action": "weight", "value": "heavy"}),
            "a weight is a number of kilograms",
        )
        self.assertEqual(self.den.calls(), [])

    def test_a_field_left_empty_writes_nothing_and_says_nothing(self) -> None:
        # The person opened the box and changed their mind. That is not an
        # error, and a panel wearing one would be a panel to dismiss.
        for action in (
            {"action": "weight", "value": "  "},
            {"action": "add", "text": ""},
            {"action": "note", "body": " "},
            {"action": "food", "name": "", "amount": "100"},
        ):
            self.assertIsNone(self.journal.act(action), action)
        self.assertEqual(self.den.calls(), [])

    def test_a_note_carries_a_heading_only_when_it_was_given_one(self) -> None:
        self.journal.act(
            {"action": "note", "heading": " Standup ", "body": " Shipped it. "}
        )
        self.journal.act({"action": "note", "body": "No heading."})
        den = str(self.den.den)
        self.assertIn(
            [
                *["--den", den, "--date", DAY],
                *["note", "add", "Shipped it.", "--title", "Standup"],
            ],
            self.den.calls(),
        )
        self.assertIn(
            ["--den", den, "--date", DAY, "note", "add", "No heading."],
            self.den.calls(),
        )

    def test_a_rewritten_note_replaces_the_one_that_was_there(self) -> None:
        self.assertIsNone(
            self.journal.act(
                {
                    "action": "edit",
                    "index": 1,
                    "heading": " Plan ",
                    "body": " A month view. ",
                }
            )
        )
        den = str(self.den.den)
        self.assertIn(
            [
                *["--den", den, "--date", DAY],
                *["note", "edit", "1", "A month view.", "--heading", "Plan"],
            ],
            self.den.calls(),
        )
        self.assertEqual(
            self.journal.read("notes")["notes"][0],
            {"index": 1, "heading": "Plan", "body": "A month view."},
        )

    def test_a_rewritten_note_left_without_a_heading_keeps_the_one_it_had(self) -> None:
        # The box opens on the note as it stands, so a heading that comes back
        # empty is a note that never had one. `today note edit` keeps the old
        # heading in that case, which is what makes the field safe to leave.
        self.journal.act({"action": "edit", "index": 1, "body": "Still an idea."})
        den = str(self.den.den)
        self.assertIn(
            [*["--den", den, "--date", DAY], *["note", "edit", "1", "Still an idea."]],
            self.den.calls(),
        )
        self.assertEqual(self.journal.read("notes")["notes"][0]["heading"], "Idea")

    def test_a_note_rewritten_to_nothing_is_refused_rather_than_emptied(self) -> None:
        self.assertEqual(
            self.journal.act({"action": "edit", "index": 1, "body": "  "}),
            "a note with nothing in it is a note to remove",
        )
        self.assertEqual(self.den.calls(), [])

    def test_a_rewrite_of_a_note_that_is_not_there_carries_the_complaint(self) -> None:
        self.assertEqual(
            self.journal.act({"action": "edit", "index": 9, "body": "Nope."}),
            "note 9 not found",
        )

    def test_a_rewrite_without_an_index_is_dropped(self) -> None:
        self.assertIsNone(
            self.journal.act({"action": "edit", "index": "one", "body": "Nope."})
        )
        self.assertIsNone(self.journal.act({"action": "edit", "index": 0, "body": "x"}))
        self.assertEqual(self.den.calls(), [])

    def test_a_food_the_database_names_once_is_logged_straight_away(self) -> None:
        self.assertIsNone(
            self.journal.act({"action": "food", "name": " oats ", "amount": "80"})
        )
        # Queried, scaled, then written. The row is the database's to compose:
        # `macros add` takes a `what 100g,protein,carbs,fat` line, which is a
        # thing to calculate rather than to type.
        self.assertEqual(self.den.plan()["added"], ["oats:g 80.0g,1,2,3"])
        self.assertEqual(self.journal.read("macros")["choices"], [])

    def test_a_food_taken_from_the_list_is_logged_by_its_own_id(self) -> None:
        # `chicken:g` is a subsequence of two other rows, so the search matches
        # three. It is still that row that gets logged: a candidate answers
        # with its id, and an id that names a row exactly is that row.
        self.assertIsNone(
            self.journal.act({"action": "food", "name": "chicken:g", "amount": "150"})
        )
        self.assertEqual(self.den.plan()["added"], ["chicken:g 150.0g,1,2,3"])

    def test_a_food_that_matches_more_than_one_is_said_not_guessed(self) -> None:
        self.assertEqual(
            self.journal.act({"action": "food", "name": " chicken ", "amount": "150"}),
            "3 foods match chicken - pick one",
        )
        self.assertNotIn("added", self.den.plan())

    def test_a_food_with_no_amount_or_no_match_says_which(self) -> None:
        self.assertEqual(
            self.journal.act({"action": "food", "name": "oats", "amount": "lots"}),
            "an amount is a number of grams or pieces",
        )
        self.assertEqual(
            self.journal.act({"action": "food", "name": "quinoa", "amount": "80"}),
            "no food matching quinoa",
        )
        self.assertNotIn("added", self.den.plan())

    def test_a_search_offers_the_database_rows_by_id_and_by_label(self) -> None:
        self.assertIsNone(self.journal.act({"action": "search", "name": " chick "}))
        self.assertEqual(
            self.journal.read("macros")["choices"],
            [
                {"id": "chicken:g", "label": "Chicken (g)"},
                {"id": "chicken breast:g", "label": "Chicken breast (g)"},
                {"id": "chicken thigh:g", "label": "Chicken thigh (g)"},
            ],
        )

    def test_a_search_costs_one_query_and_never_a_day_read(self) -> None:
        # One keystroke, one `macros query`. A search that made the day stale
        # would cost a `show` and a month of weights for every letter typed.
        self.journal.read("macros")
        before = self.den.calls()
        self.journal.act({"action": "search", "name": "chick"})
        self.journal.read("macros")
        self.assertEqual(
            self.den.calls()[len(before) :],
            [["--den", str(self.den.den), "macros", "query", "chick", "--json"]],
        )

    def test_a_search_with_nothing_typed_yet_offers_nothing(self) -> None:
        self.journal.act({"action": "search", "name": "chick"})
        self.assertIsNone(self.journal.act({"action": "search", "name": "  "}))
        self.assertEqual(self.journal.read("macros")["choices"], [])

    def test_a_logged_food_clears_the_list_that_named_it(self) -> None:
        self.journal.act({"action": "search", "name": "oats"})
        self.journal.act({"action": "food", "name": "oats:g", "amount": "80"})
        self.assertEqual(self.journal.read("macros")["choices"], [])

    def test_moving_the_day_drops_the_list_under_the_field(self) -> None:
        # The box was open over the day that was showing. Offering its answers
        # against another day is worse than asking again.
        self.journal.act({"action": "search", "name": "chick"})
        self.journal.act({"action": "select", "date": NEXT})
        self.assertEqual(self.journal.read("macros")["choices"], [])

    def test_only_the_macros_view_carries_the_list(self) -> None:
        self.journal.act({"action": "search", "name": "chick"})
        self.assertNotIn("choices", self.journal.read("agenda"))
        self.journal.forget()
        self.assertNotIn("choices", self.journal.read("notes"))


class Trouble(unittest.TestCase):
    """What a panel says when the journal will not answer."""

    def setUp(self) -> None:
        self.den = Den(self, plan())
        self.den.entry(DAY)

    def journal(self, **extra: str) -> ModuleType:
        patch = unittest.mock.patch.dict(
            os.environ, {**self.den.environment(), **extra}
        )
        patch.start()
        self.addCleanup(patch.stop)
        journal = today.Journal({})
        journal.select(DAY)
        return journal

    def test_a_missing_command_is_a_sentence_rather_than_an_empty_panel(self) -> None:
        journal = self.journal(
            SCUFRIS_TODAY_COMMAND=str(self.den.root / "no-such-today")
        )
        # Every view, not only the ones that ask a second question. Notes asks
        # for the day and nothing else, so it is the one that would otherwise
        # report an empty day and look like a day with no notes in it.
        for view in ("agenda", "macros", "notes"):
            journal.forget()
            frame = journal.read(view)
            self.assertIn("is not on the path", frame["trouble"], view)
            self.assertEqual(frame["view"], view)
            self.assertEqual(frame["date"], DAY)

    def test_a_refused_command_carries_its_own_complaint(self) -> None:
        (self.den.home / "plan.json").write_text(
            json.dumps({**plan(), "refuse": "the den is locked"})
        )
        journal = self.journal()
        self.assertEqual(journal.read("agenda")["trouble"], "the den is locked")

    def test_a_refused_tick_is_reported_without_losing_the_day(self) -> None:
        journal = self.journal()
        journal.read("agenda")
        (self.den.home / "plan.json").write_text(
            json.dumps({**plan(), "refuse": "no habit named Swim"})
        )
        self.assertEqual(
            journal.act({"action": "habit", "name": "Swim"}), "no habit named Swim"
        )
        # The reading was kept: a habit that would not toggle is no reason to
        # blank a panel that was reading fine a moment ago.
        self.assertEqual(len(journal.read("agenda")["habits"]), 2)


class Driving(unittest.TestCase):
    """The backend as the companion runs it: one process, lines both ways."""

    def start(self, den: Den, *lines: dict[str, object]) -> dict[str, object]:
        """Runs the backend, writes what it is told, and reads one frame."""
        running = subprocess.Popen(
            [sys.executable, str(BACKENDS / "today" / "backend.py")],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            env={**os.environ, **den.environment()},
        )
        self.addCleanup(_stop, running)
        assert running.stdin is not None and running.stdout is not None
        for line in lines:
            running.stdin.write(json.dumps(line) + "\n")
        running.stdin.flush()
        return json.loads(running.stdout.readline())

    def test_a_spawn_payload_picks_the_view_and_an_action_moves_the_day(self) -> None:
        den = Den(self, plan())
        den.entry(DAY)
        first = self.start(den, {"view": "macros"}, {"action": "select", "date": DAY})
        self.assertEqual(first["view"], "macros")
        self.assertEqual(first["date"], DAY)
        self.assertEqual(first["macros"]["calories"], 1962)

    def test_a_view_nobody_offers_opens_as_an_agenda(self) -> None:
        den = Den(self, plan())
        den.entry(DAY)
        self.assertEqual(self.start(den, {"view": "horoscope"})["view"], "agenda")


def _stop(running: "subprocess.Popen[str]") -> None:
    """Ends one backend and closes what was held open on it."""
    running.kill()
    running.wait()
    for pipe in (running.stdin, running.stdout):
        if pipe is not None:
            pipe.close()


if __name__ == "__main__":
    unittest.main()
