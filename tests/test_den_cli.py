"""The den command line, run as the agent runs it.

Every test runs the real program against a den under a temporary directory.
Nothing here reads the real `$DEN_PATH`.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[1]
COMMAND = REPOSITORY / "tools" / "den" / "cli.py"

TEMPLATE = (
    "# {{title}}\n\n### Tasks\n\n### Habits\n\n- [ ] Gym\n\n"
    "### Macros\n\nwhat,protein,carbs,fat\n\n### Weight\n\n### Notes\n"
)


class Command(unittest.TestCase):
    def setUp(self) -> None:
        self.room = tempfile.TemporaryDirectory()
        self.addCleanup(self.room.cleanup)
        self.den = Path(self.room.name) / "the-den"
        (self.den / "Daily").mkdir(parents=True)
        (self.den / "Templates").mkdir()
        (self.den / "Templates" / "daily.md").write_text(TEMPLATE, encoding="utf-8")
        self.database = Path(self.room.name) / "macros.csv"
        self.database.write_text(
            "chicken breast 100g,31,0,3.6\negg 1pc,6,0,5\n", encoding="utf-8"
        )

    def run_den(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(COMMAND), "--date", "2026-08-31", *arguments],
            capture_output=True,
            text=True,
            check=False,
            env={
                **os.environ,
                "DEN_PATH": str(self.den),
                "MACROS_DATABASE": str(self.database),
            },
        )

    def ok(self, *arguments: str) -> str:
        done = self.run_den(*arguments)
        self.assertEqual(done.returncode, 0, done.stderr)
        return done.stdout.strip()

    def answered(self, *arguments: str) -> object:
        return json.loads(self.ok(*arguments))

    def entry(self) -> Path:
        return self.den / "Daily" / "2026-08-31-Monday.md"


class Reads(Command):
    def test_a_read_creates_no_entry(self) -> None:
        self.ok("show", "--json")
        self.ok("task", "list", "--json")
        self.assertFalse(self.entry().exists())

    def test_the_path_is_printed_without_making_it(self) -> None:
        self.assertTrue(self.ok("path").endswith("2026-08-31-Monday.md"))
        self.assertFalse(self.entry().exists())

    def test_create_makes_the_entry_from_the_template(self) -> None:
        self.ok("create")
        self.assertIn("### Habits", self.entry().read_text(encoding="utf-8"))

    def test_json_is_taken_before_or_after_the_subcommand(self) -> None:
        self.ok("task", "add", "call the dentist")
        after = self.answered("task", "list", "--json")
        done = subprocess.run(
            [
                sys.executable,
                str(COMMAND),
                "--json",
                "--date",
                "2026-08-31",
                "task",
                "list",
            ],
            capture_output=True,
            text=True,
            check=False,
            env={**os.environ, "DEN_PATH": str(self.den)},
        )
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(json.loads(done.stdout), after)


class Writes(Command):
    def test_a_task_is_added_and_ticked(self) -> None:
        self.ok("task", "add", "call the dentist")
        self.ok("task", "done", "1")
        found = self.answered("task", "list", "--json")
        assert isinstance(found, list)
        self.assertEqual(found[0]["text"], "call the dentist")
        self.assertTrue(found[0]["done"])

    def test_a_habit_is_toggled_by_name(self) -> None:
        self.ok("habit", "toggle", "Gym")
        self.assertIn("- [x] Gym", self.entry().read_text(encoding="utf-8"))

    def test_a_weight_is_logged_and_read_back(self) -> None:
        self.ok("weight", "81.4")
        found = self.answered("weight", "--json")
        assert isinstance(found, dict)
        self.assertEqual(found["weight"], 81.4)

    def test_words_are_not_a_weight(self) -> None:
        done = self.run_den("weight", "heavy")
        self.assertEqual(done.returncode, 1)
        self.assertIn("kilograms", done.stderr)

    def test_sets_are_logged_into_a_section_that_did_not_exist(self) -> None:
        self.ok("gym", "split", "Push")
        self.ok("gym", "add", "bench press", "60x8", "60x7")
        found = self.answered("gym", "list", "--json")
        assert isinstance(found, dict)
        self.assertEqual(found["split"], "Push")
        self.assertEqual(found["volume"], 900.0)
        self.assertEqual(len(found["lifts"]), 2)
        self.assertIn("### Workout", self.entry().read_text(encoding="utf-8"))

    def test_half_a_repetition_is_refused(self) -> None:
        done = self.run_den("gym", "add", "bench press", "60x8.5")
        self.assertEqual(done.returncode, 1)
        self.assertIn("60x8", done.stderr)

    def test_the_split_is_read_back_and_corrected_in_place(self) -> None:
        self.ok("gym", "split", "push")
        self.ok("gym", "add", "bench press", "60x8")
        self.ok("gym", "split", "pull")
        found = self.answered("gym", "split", "--json")
        assert isinstance(found, dict)
        self.assertEqual(found["split"], "pull")
        self.assertEqual(len(self.answered("gym", "list", "--json")["lifts"]), 1)

    def test_a_movement_is_written_over_and_then_removed(self) -> None:
        self.ok("gym", "add", "bench press", "60x8", "60x7")
        self.ok("gym", "add", "dips", "0x12")
        self.ok("gym", "edit", "bench press", "60x8", "60x8", "--rename", "incline")
        found = self.answered("gym", "list", "--json")
        assert isinstance(found, dict)
        self.assertEqual(
            [(lift["exercise"], lift["reps"]) for lift in found["lifts"]],
            [("incline", 8), ("incline", 8), ("dips", 12)],
        )
        self.ok("gym", "edit", "incline")
        found = self.answered("gym", "list", "--json")
        assert isinstance(found, dict)
        self.assertEqual([lift["exercise"] for lift in found["lifts"]], ["dips"])

    def test_writing_over_a_movement_that_is_not_there_is_refused(self) -> None:
        self.ok("gym", "add", "bench press", "60x8")
        done = self.run_den("gym", "edit", "squat", "80x5")
        self.assertEqual(done.returncode, 1)
        self.assertIn("squat", done.stderr)

    def test_the_exercise_database_is_learned_and_forgotten(self) -> None:
        self.ok("gym", "learn", "pull", "tbar")
        self.ok("gym", "learn", "push", "dips")
        found = self.answered("gym", "known", "--json")
        assert isinstance(found, list)
        self.assertEqual([move["name"] for move in found], ["tbar", "dips"])
        self.assertEqual(
            self.ok("gym", "database").strip(), str(self.den / "Exercises.csv")
        )
        self.ok("gym", "forget", "tbar")
        self.assertEqual(
            [move["name"] for move in self.answered("gym", "known", "--json")], ["dips"]
        )
        done = self.run_den("gym", "forget", "tbar")
        self.assertEqual(done.returncode, 1)

    def test_a_food_is_scaled_out_of_the_database(self) -> None:
        self.ok("macros", "log", "chicken breast:g", "150")
        self.assertIn(
            "chicken breast 150g,46.5,0,5.4", self.entry().read_text(encoding="utf-8")
        )

    def test_a_food_row_is_written_over_and_then_removed(self) -> None:
        self.ok("macros", "log", "chicken breast:g", "150")
        self.ok("macros", "edit", "1", "chicken breast 200g,62,0,7.2")
        found = self.answered("macros", "list", "--json")
        assert isinstance(found, dict)
        self.assertEqual(
            [food["name"] for food in found["foods"]], ["chicken breast 200g"]
        )
        self.ok("macros", "rm", "1")
        self.assertEqual(self.answered("macros", "list", "--json")["foods"], [])

    def test_words_matching_more_than_one_food_are_refused_with_the_names(self) -> None:
        self.database.write_text(
            "egg 1pc,6,0,5\negg white 100g,11,0,0.2\n", encoding="utf-8"
        )
        done = self.run_den("macros", "log", "egg", "2")
        self.assertEqual(done.returncode, 1)
        self.assertIn("pick one", done.stderr)
        self.assertIn("egg:pc", done.stderr)

    def test_a_note_is_kept_and_edited_in_place(self) -> None:
        self.ok("note", "add", "a calendar", "--title", "idea")
        self.ok("note", "edit", "1", "a month")
        found = self.answered("note", "list", "--json")
        assert isinstance(found, list)
        self.assertEqual(found[0]["body"], "a month")
        self.assertTrue(str(found[0]["heading"]).endswith("- idea"))


class Ideas(Command):
    def test_an_idea_is_kept_and_pulled_onto_a_day(self) -> None:
        self.ok("backlog", "add", "learn to weld")
        self.ok("backlog", "promote", "1")
        self.assertEqual(self.answered("backlog", "list", "--json"), [])
        found = self.answered("task", "list", "--json")
        assert isinstance(found, list)
        self.assertEqual(found[0]["text"], "learn to weld")

    def test_pulling_an_idea_that_is_not_there_is_refused(self) -> None:
        done = self.run_den("backlog", "promote", "3")
        self.assertEqual(done.returncode, 1)
        self.assertIn("not found", done.stderr)


class Windows(Command):
    def test_restant_and_upcoming_answer_around_the_day(self) -> None:
        (self.den / "Daily" / "2026-08-30-Sunday.md").write_text(
            "# S\n\n### Tasks\n\n- [ ] left behind\n", encoding="utf-8"
        )
        (self.den / "Daily" / "2026-09-02-Wednesday.md").write_text(
            "# W\n\n### Tasks\n\n- [ ] coming up\n", encoding="utf-8"
        )
        late = self.answered("restant", "--json")
        coming = self.answered("upcoming", "--json")
        assert isinstance(late, list) and isinstance(coming, list)
        self.assertEqual([task["text"] for task in late], ["left behind"])
        self.assertEqual([task["text"] for task in coming], ["coming up"])

    def test_restant_stops_at_the_days_it_was_given(self) -> None:
        (self.den / "Daily" / "2026-07-01-Wednesday.md").write_text(
            "# J\n\n### Tasks\n\n- [ ] long gone\n", encoding="utf-8"
        )
        near = self.answered("restant", "--days", "7", "--json")
        far = self.answered("restant", "--days", "90", "--json")
        assert isinstance(near, list) and isinstance(far, list)
        self.assertEqual(near, [])
        self.assertEqual([task["text"] for task in far], ["long gone"])

    def test_workout_history_is_newest_first(self) -> None:
        (self.den / "Daily" / "2026-08-29-Saturday.md").write_text(
            "# F\n\n### Workout\n\nPull\n\nexercise,weight,reps\nbarbell row,50,10\n",
            encoding="utf-8",
        )
        self.ok("gym", "split", "Push")
        self.ok("gym", "add", "bench press", "60x8")
        found = self.answered("gym", "history", "--days", "7", "--json")
        assert isinstance(found, list)
        self.assertEqual([day["date"] for day in found], ["2026-08-31", "2026-08-29"])
        self.assertEqual([day["split"] for day in found], ["Push", "Pull"])


if __name__ == "__main__":
    unittest.main()
