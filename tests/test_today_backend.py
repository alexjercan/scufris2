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

#: A stub standing in for `today`. It answers the four subcommands the backend
#: asks for out of a plan file, records every call, and refuses what the plan
#: tells it to refuse - so a test can drive a failure without a broken journal.
STUB = '''#!/usr/bin/env python3
import json, os, pathlib, sys

home = pathlib.Path(os.environ["STUB_HOME"])
plan = json.loads((home / "plan.json").read_text())
args = sys.argv[1:]
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
elif rest[0] == "weight":
    print(json.dumps(plan["weight"]))
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
        self.assertEqual(
            [habit["name"] for habit in frame["habits"]], ["Gym", "Read"]
        )
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
        frame = journal.read("agenda")
        self.assertIn("is not on the path", frame["trouble"])
        self.assertEqual(frame["view"], "agenda")
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
        first = self.start(
            den, {"view": "macros"}, {"action": "select", "date": DAY}
        )
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
