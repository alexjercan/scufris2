"""What the den backend makes of a journal, on a journal of its own.

The program under test is assembled the way `build.rs` assembles it: the
libraries named in `prelude`, then `backend.py`, concatenated into one text.
So the prelude contract is measured here too, and what runs in a test is what
the companion compiles in.

Every test builds a den under a temporary directory. Nothing here reads the
real `$DEN_PATH`.
"""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from datetime import date, timedelta
from pathlib import Path
from types import ModuleType
from typing import ClassVar
from unittest import mock

REPOSITORY = Path(__file__).resolve().parents[1]
BACKENDS = REPOSITORY / "surfaces" / "desktop" / "backends"


def assemble(name: str) -> str:
    """The one program text a backend is, libraries and all."""
    held = BACKENDS / name
    listed = held / "prelude"
    texts: list[str] = []
    if listed.is_file():
        for line in listed.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            path = REPOSITORY / line
            assert path.is_file(), f"prelude names {line}, which is not a file"
            texts.append(path.read_text(encoding="utf-8"))
    texts.append((held / "backend.py").read_text(encoding="utf-8"))
    return "\n".join(texts)


def backend(name: str) -> ModuleType:
    """Runs one assembled backend as a module, since it is not a package."""
    spec = importlib.util.spec_from_loader(f"backend_{name}", loader=None)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    exec(  # noqa: S102 - exercise the exact assembled backend program
        compile(assemble(name), f"{name}/backend.py", "exec"), module.__dict__
    )
    return module


den = backend("den")

TODAY = date(2026, 8, 31)
YESTERDAY = TODAY - timedelta(days=1)
TOMORROW = TODAY + timedelta(days=1)

TEMPLATE = (
    "# {{title}}\n\n### Tasks\n\n### Habits\n\n- [ ] Learn\n- [ ] Gym\n\n"
    "### Macros\n\nwhat,protein,carbs,fat\n\n### Weight\n\n### Notes\n"
)


class Panel(unittest.TestCase):
    """A den with a few days in it, and a panel opened on one of them."""

    view = "agenda"
    spawn: ClassVar[dict[str, object]] = {}

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
        self.write(
            TODAY,
            "# Monday, August 31, 2026\n\n### Tasks\n\n- [ ] ship the panels\n"
            "- [x] water the plants\n\n### Habits\n\n- [ ] Gym\n- [x] Learn\n\n"
            "### Macros\n\nwhat,protein,carbs,fat\nrice 100g,7,78,0.6\n\n"
            "### Weight\n\n81.4 kg\n\n### Notes\n\n#### 09:00 - idea\n\na calendar\n",
        )
        self.write(YESTERDAY, "# Sunday\n\n### Tasks\n\n- [ ] left behind\n")
        self.write(TOMORROW, "# Tuesday\n\n### Tasks\n\n- [ ] coming up\n")
        patch = mock.patch.dict(
            os.environ,
            {"DEN_PATH": str(self.den), "MACROS_DATABASE": str(self.database)},
        )
        patch.start()
        self.addCleanup(patch.stop)
        self.panel = den.Panel(dict(self.spawn))
        self.panel.select(TODAY.isoformat())

    def write(self, day: date, text: str) -> Path:
        path = den.entry_path(self.den, day)
        path.write_text(text, encoding="utf-8")
        return path

    def read(self) -> dict[str, object]:
        return self.panel.read(self.view)

    def do(self, action: dict[str, object]) -> dict[str, object]:
        """One action, then the reading that follows it, as `main` does."""
        refused = self.panel.act(action)
        reading = self.panel.read(self.view)
        if refused:
            reading["trouble"] = refused
        return reading

    def entry(self, day: date = TODAY) -> str:
        return den.entry_path(self.den, day).read_text(encoding="utf-8")


class Agenda(Panel):
    def test_the_day_arrives_with_its_habits_and_tasks(self) -> None:
        reading = self.read()
        self.assertEqual(reading["date"], TODAY.isoformat())
        self.assertTrue(reading["exists"])
        self.assertIsNone(reading["trouble"])
        self.assertEqual(
            [task["text"] for task in reading["tasks"]],
            ["ship the panels", "water the plants"],
        )
        self.assertEqual(
            [habit["name"] for habit in reading["habits"]], ["Gym", "Learn"]
        )

    def test_a_task_is_removed_by_its_number(self) -> None:
        reading = self.do({"action": "untask", "index": 1})
        self.assertEqual(
            [task["text"] for task in reading["tasks"]], ["water the plants"]
        )
        self.assertNotIn("ship the panels", self.entry())

    def test_removing_a_task_that_is_not_there_is_said_out_loud(self) -> None:
        reading = self.do({"action": "untask", "index": 9})
        self.assertIsNotNone(reading["trouble"])
        self.assertEqual(len(reading["tasks"]), 2)

    def test_what_was_left_behind_and_what_is_coming_arrive_apart(self) -> None:
        reading = self.read()
        self.assertEqual(
            [(task["date"], task["text"]) for task in reading["restant"]],
            [(YESTERDAY.isoformat(), "left behind")],
        )
        self.assertEqual(
            [(task["date"], task["text"]) for task in reading["ahead"]],
            [(TOMORROW.isoformat(), "coming up")],
        )

    def test_every_day_carrying_something_undone_is_marked(self) -> None:
        self.assertEqual(
            self.read()["marks"],
            sorted([YESTERDAY.isoformat(), TODAY.isoformat(), TOMORROW.isoformat()]),
        )

    def test_the_agenda_carries_nothing_the_agenda_does_not_draw(self) -> None:
        reading = self.read()
        for absent in ("notes", "foods", "macros", "weight", "lifts", "recent"):
            self.assertNotIn(absent, reading)

    def test_a_habit_is_ticked_by_name(self) -> None:
        reading = self.do({"action": "habit", "name": "Gym"})
        self.assertTrue(reading["habits"][0]["done"])
        self.assertIn("- [x] Gym", self.entry())

    def test_a_task_is_ticked_by_its_number(self) -> None:
        reading = self.do({"action": "task", "index": 1})
        self.assertTrue(reading["tasks"][0]["done"])

    def test_a_task_is_added_to_the_day_that_is_showing(self) -> None:
        reading = self.do({"action": "add", "text": "call the dentist"})
        self.assertEqual(reading["tasks"][-1]["text"], "call the dentist")

    def test_a_task_added_to_a_day_with_no_entry_makes_one(self) -> None:
        later = TODAY + timedelta(days=4)
        self.panel.select(later.isoformat())
        reading = self.do({"action": "add", "text": "plan the week"})
        self.assertTrue(reading["exists"])
        self.assertEqual([task["text"] for task in reading["tasks"]], ["plan the week"])

    def test_a_day_that_moved_under_the_panel_is_shown_again_not_written_over(
        self,
    ) -> None:
        self.read()
        self.write(TODAY, "# Monday\n\n### Tasks\n\n- [ ] something else\n")
        reading = self.do({"action": "task", "index": 1})
        self.assertIn("changed elsewhere", str(reading["trouble"]))
        self.assertEqual(
            [task["text"] for task in reading["tasks"]], ["something else"]
        )
        self.assertFalse(reading["tasks"][0]["done"])

    def test_a_refusal_arrives_beside_the_day_rather_than_instead_of_it(self) -> None:
        reading = self.do({"action": "habit", "name": "Swimming"})
        self.assertIn("Swimming", str(reading["trouble"]))
        self.assertEqual(reading["date"], TODAY.isoformat())
        self.assertEqual(len(reading["tasks"]), 2)

    def test_reading_a_month_leaves_no_entries_behind(self) -> None:
        before = sorted(path.name for path in (self.den / "Daily").glob("*.md"))
        for step in range(1, 20):
            self.panel.select((TODAY + timedelta(days=step)).isoformat())
            self.read()
        after = sorted(path.name for path in (self.den / "Daily").glob("*.md"))
        self.assertEqual(before, after)


class Backlog(Panel):
    def test_an_idea_with_no_day_goes_to_the_backlog(self) -> None:
        reading = self.do({"action": "idea", "text": "learn to weld"})
        self.assertEqual(
            [idea["text"] for idea in reading["backlog"]], ["learn to weld"]
        )
        self.assertTrue(den.backlog_path(self.den).is_file())

    def test_an_idea_pulled_onto_the_day_leaves_the_backlog(self) -> None:
        self.do({"action": "idea", "text": "learn to weld"})
        reading = self.do({"action": "promote", "index": 1})
        self.assertEqual(reading["backlog"], [])
        self.assertEqual(reading["tasks"][-1]["text"], "learn to weld")

    def test_an_idea_is_dropped_by_its_number(self) -> None:
        self.do({"action": "idea", "text": "one"})
        self.do({"action": "idea", "text": "two"})
        reading = self.do({"action": "unidea", "index": 1})
        self.assertEqual([idea["text"] for idea in reading["backlog"]], ["two"])

    def test_ideas_already_done_are_not_offered(self) -> None:
        den.backlog_path(self.den).write_text(
            "# Backlog\n\n- [x] done already\n- [ ] still open\n", encoding="utf-8"
        )
        self.assertEqual(
            [idea["text"] for idea in self.read()["backlog"]], ["still open"]
        )


class Macros(Panel):
    view = "macros"

    def test_the_day_arrives_eaten_weighed_and_lifted(self) -> None:
        reading = self.read()
        self.assertEqual(reading["weight"], 81.4)
        self.assertEqual([food["name"] for food in reading["foods"]], ["rice 100g"])
        self.assertEqual(reading["macros"]["calories"], round(7 * 4 + 78 * 4 + 0.6 * 9))
        self.assertEqual(reading["lifts"], [])
        self.assertEqual(reading["volume"], 0)

    def test_the_macros_panel_carries_nothing_it_does_not_draw(self) -> None:
        reading = self.read()
        for absent in ("notes", "tasks", "habits", "restant", "backlog"):
            self.assertNotIn(absent, reading)

    def test_a_weight_is_logged_and_read_back(self) -> None:
        reading = self.do({"action": "weight", "value": "80.9"})
        self.assertEqual(reading["weight"], 80.9)
        self.assertIn("80.9 kg", self.entry())

    def test_a_food_row_is_written_over_by_the_cells_it_is_asked_for(self) -> None:
        reading = self.do(
            {
                "action": "refood",
                "index": 1,
                "what": "rice 200g",
                "protein": "14",
                "carbs": "156",
                "fat": "1.2",
            }
        )
        self.assertEqual([food["name"] for food in reading["foods"]], ["rice 200g"])
        self.assertEqual(reading["macros"]["carbs"], 156)

    def test_a_food_row_keeps_its_name(self) -> None:
        reading = self.do(
            {
                "action": "refood",
                "index": 1,
                "what": " ",
                "protein": "1",
                "carbs": "1",
                "fat": "1",
            }
        )
        self.assertIn("name", str(reading["trouble"]))
        self.assertEqual([food["name"] for food in reading["foods"]], ["rice 100g"])

    def test_a_cell_that_is_not_a_number_writes_nothing(self) -> None:
        reading = self.do(
            {
                "action": "refood",
                "index": 1,
                "what": "rice 200g",
                "protein": "lots",
                "carbs": "156",
                "fat": "1.2",
            }
        )
        self.assertIsNotNone(reading["trouble"])
        self.assertEqual([food["name"] for food in self.read()["foods"]], ["rice 100g"])

    def test_a_food_row_is_removed_by_its_number(self) -> None:
        reading = self.do({"action": "unfood", "index": 1})
        self.assertEqual(reading["foods"], [])
        self.assertEqual(reading["macros"]["calories"], 0)

    def test_words_are_not_a_weight(self) -> None:
        reading = self.do({"action": "weight", "value": "heavy"})
        self.assertIn("kilograms", str(reading["trouble"]))
        self.assertEqual(reading["weight"], 81.4)

    def test_an_empty_weight_is_a_person_changing_their_mind(self) -> None:
        reading = self.do({"action": "weight", "value": "  "})
        self.assertIsNone(reading["trouble"])
        self.assertEqual(reading["weight"], 81.4)

    def test_the_trend_carries_a_month_and_its_change(self) -> None:
        self.write(YESTERDAY, "# Sunday\n\n### Weight\n\n82.4 kg\n")
        self.panel.forget()
        reading = self.read()
        self.assertEqual([point["weight"] for point in reading["recent"]], [82.4, 81.4])
        self.assertEqual(reading["change"], -1.0)

    def test_a_food_taken_from_the_list_is_scaled_and_logged(self) -> None:
        reading = self.do(
            {"action": "food", "name": "chicken breast:g", "amount": "150"}
        )
        self.assertIn("chicken breast 150g,46.5,0,5.4", self.entry())
        self.assertEqual(reading["macros"]["protein"], 53.5)

    def test_words_matching_one_food_are_taken_as_that_food(self) -> None:
        self.do({"action": "food", "name": "egg", "amount": "2"})
        self.assertIn("egg 2pc,12,0,10", self.entry())

    def test_words_matching_nothing_are_said_out_loud(self) -> None:
        reading = self.do({"action": "food", "name": "tofu", "amount": "100"})
        self.assertIn("no food matching tofu", str(reading["trouble"]))

    def test_an_amount_that_is_not_a_number_is_refused(self) -> None:
        reading = self.do({"action": "food", "name": "egg", "amount": "some"})
        self.assertIn("amount", str(reading["trouble"]))

    def test_a_search_answers_in_choices_and_writes_nothing(self) -> None:
        before = self.entry()
        reading = self.do({"action": "search", "name": "chbr"})
        self.assertEqual(
            reading["choices"],
            [{"id": "chicken breast:g", "label": "chicken breast (g)"}],
        )
        self.assertEqual(self.entry(), before)


class Workout(Panel):
    view = "macros"

    def test_every_set_of_one_movement_is_logged_in_one_answer(self) -> None:
        reading = self.do(
            {
                "action": "lift",
                "split": "Push",
                "exercise": "bench press",
                "sets": "60x8 60x8 60x7",
            }
        )
        self.assertEqual(
            [
                (lift["exercise"], lift["weight"], lift["reps"])
                for lift in reading["lifts"]
            ],
            [
                ("bench press", 60.0, 8),
                ("bench press", 60.0, 8),
                ("bench press", 60.0, 7),
            ],
        )
        self.assertEqual(reading["split"], "Push")
        self.assertEqual(reading["volume"], 60 * 8 + 60 * 8 + 60 * 7)

    def test_the_split_is_the_day_s_and_is_not_asked_for_twice(self) -> None:
        self.do(
            {
                "action": "lift",
                "split": "Push",
                "exercise": "bench press",
                "sets": "60x8",
            }
        )
        # The second exercise carries no split, the way the box asks for it
        # once the day has one.
        reading = self.do({"action": "lift", "exercise": "dips", "sets": "0x12"})
        self.assertEqual(reading["split"], "Push")
        self.assertEqual(len(reading["lifts"]), 2)
        self.assertEqual(self.entry().count("Push"), 1)

    def test_the_split_is_named_on_its_own(self) -> None:
        reading = self.do({"action": "split", "split": "legs"})
        self.assertEqual(reading["split"], "legs")
        reading = self.do({"action": "split", "split": "pull"})
        self.assertEqual(reading["split"], "pull")
        self.assertEqual(self.read()["lifts"], [])

    def test_a_split_with_a_comma_is_refused(self) -> None:
        reading = self.do({"action": "split", "split": "push,pull"})
        self.assertIsNotNone(reading["trouble"])
        self.assertEqual(self.read()["split"], "")

    def test_a_set_lands_in_a_day_that_never_had_a_workout_section(self) -> None:
        self.assertNotIn("### Workout", self.entry())
        self.do(
            {
                "action": "lift",
                "split": "Push",
                "exercise": "bench press",
                "sets": "60x8",
            }
        )
        written = self.entry()
        self.assertIn("### Workout", written)
        self.assertLess(written.index("### Workout"), written.index("### Notes"))

    def test_reps_are_whole_and_a_load_is_not_negative(self) -> None:
        for bad in ("60x8.5", "-1x8", "60"):
            reading = self.do(
                {"action": "lift", "split": "Push", "exercise": "row", "sets": bad}
            )
            self.assertIsNotNone(reading["trouble"])
        self.assertEqual(self.read()["lifts"], [])

    def test_one_bad_set_writes_none_of_them(self) -> None:
        reading = self.do(
            {"action": "lift", "exercise": "row", "sets": "60x8 60x8 60xhalf"}
        )
        self.assertIsNotNone(reading["trouble"])
        self.assertEqual(self.read()["lifts"], [])

    def test_a_set_is_removed_by_its_number(self) -> None:
        self.do({"action": "lift", "exercise": "bench press", "sets": "60x8 60x7"})
        reading = self.do({"action": "unlift", "index": 1})
        self.assertEqual([lift["reps"] for lift in reading["lifts"]], [7])

    def test_a_movement_is_written_over_by_the_sets_it_is_asked_for(self) -> None:
        self.do({"action": "lift", "exercise": "bench press", "sets": "60x8 60x7"})
        self.do({"action": "lift", "exercise": "dips", "sets": "0x12"})
        # A set added and a weight corrected are the same answer as the sets
        # the panel drew, typed back changed.
        reading = self.do(
            {"action": "relift", "was": "bench press", "sets": "60x8 62.5x7 62.5x5"}
        )
        self.assertEqual(
            [(lift["exercise"], lift["weight"]) for lift in reading["lifts"]],
            [
                ("bench press", 60.0),
                ("bench press", 62.5),
                ("bench press", 62.5),
                ("dips", 0.0),
            ],
        )

    def test_a_movement_asked_for_with_no_sets_is_removed(self) -> None:
        self.do({"action": "lift", "exercise": "bench press", "sets": "60x8 60x7"})
        self.do({"action": "lift", "exercise": "dips", "sets": "0x12"})
        reading = self.do({"action": "relift", "was": "bench press", "sets": ""})
        self.assertEqual([lift["exercise"] for lift in reading["lifts"]], ["dips"])
        self.assertNotIn("bench press", self.entry())

    def test_a_movement_is_renamed_without_moving(self) -> None:
        self.do({"action": "lift", "exercise": "bench pres", "sets": "60x8"})
        self.do({"action": "lift", "exercise": "dips", "sets": "0x12"})
        reading = self.do(
            {
                "action": "relift",
                "was": "bench pres",
                "exercise": "bench press",
                "sets": "60x8",
            }
        )
        self.assertEqual(
            [lift["exercise"] for lift in reading["lifts"]], ["bench press", "dips"]
        )

    def test_one_bad_set_writes_over_nothing(self) -> None:
        self.do({"action": "lift", "exercise": "bench press", "sets": "60x8 60x7"})
        reading = self.do(
            {"action": "relift", "was": "bench press", "sets": "60x8 60xhalf"}
        )
        self.assertIsNotNone(reading["trouble"])
        self.assertEqual([lift["reps"] for lift in self.read()["lifts"]], [8, 7])

    def test_writing_over_a_movement_that_is_not_there_is_said_out_loud(self) -> None:
        self.do({"action": "lift", "exercise": "bench press", "sets": "60x8"})
        reading = self.do({"action": "relift", "was": "squat", "sets": "80x5"})
        self.assertIsNotNone(reading["trouble"])
        self.assertEqual(len(self.read()["lifts"]), 1)

    def test_splits_and_movements_are_offered_out_of_what_was_done_before(self) -> None:
        self.write(
            YESTERDAY,
            "# Sunday\n\n### Workout\n\nPull\n\nexercise,weight,reps\n"
            "barbell row,50,10\nlat pulldown,40,12\n",
        )
        self.assertEqual(
            self.do({"action": "splits"})["choices"],
            [{"id": "Pull", "label": "Pull"}],
        )
        self.assertEqual(
            [
                choice["id"]
                for choice in self.do({"action": "moves", "split": "Pull"})["choices"]
            ],
            ["barbell row", "lat pulldown"],
        )

    def test_a_movement_list_narrows_to_what_was_typed(self) -> None:
        self.write(
            YESTERDAY,
            "# Sunday\n\n### Workout\n\nPull\n\nexercise,weight,reps\n"
            "barbell row,50,10\nlat pulldown,40,12\n",
        )
        reading = self.do({"action": "moves", "exercise": "pull"})
        self.assertEqual(
            [choice["id"] for choice in reading["choices"]], ["lat pulldown"]
        )

    def test_the_database_offers_movements_never_trained(self) -> None:
        (self.den / "Exercises.csv").write_text(
            "split,exercise\npush,bench press\npull,tbar\nlegs,squat\n",
            encoding="utf-8",
        )
        self.assertEqual(
            [choice["id"] for choice in self.do({"action": "moves"})["choices"]],
            ["bench press", "tbar", "squat"],
        )
        self.assertEqual(
            [choice["id"] for choice in self.do({"action": "splits"})["choices"]],
            ["push", "pull", "legs"],
        )

    def test_what_was_trained_comes_before_what_is_merely_known(self) -> None:
        self.write(
            YESTERDAY,
            "# Sunday\n\n### Workout\n\nPull\n\nexercise,weight,reps\ntbar,50,10\n",
        )
        (self.den / "Exercises.csv").write_text(
            "split,exercise\npull,barbell row\npull,tbar\n", encoding="utf-8"
        )
        self.assertEqual(
            [choice["id"] for choice in self.do({"action": "moves"})["choices"]],
            ["tbar", "barbell row"],
        )

    def test_the_day_s_split_puts_its_own_movements_first(self) -> None:
        (self.den / "Exercises.csv").write_text(
            "split,exercise\npush,bench press\npull,tbar\n", encoding="utf-8"
        )
        self.do({"action": "split", "split": "pull"})
        self.assertEqual(
            [choice["id"] for choice in self.do({"action": "moves"})["choices"]],
            ["tbar", "bench press"],
        )


class Notes(Panel):
    view = "notes"

    def test_the_notes_arrive_in_the_order_they_were_written(self) -> None:
        reading = self.read()
        self.assertEqual(
            [(note["heading"], note["body"]) for note in reading["notes"]],
            [("09:00 - idea", "a calendar")],
        )

    def test_the_notes_panel_carries_nothing_it_does_not_draw(self) -> None:
        reading = self.read()
        for absent in ("tasks", "habits", "macros", "foods", "weight", "lifts"):
            self.assertNotIn(absent, reading)

    def test_a_note_is_stamped_with_the_time_and_kept(self) -> None:
        reading = self.do({"action": "note", "heading": "standup", "body": "said it"})
        self.assertEqual(len(reading["notes"]), 2)
        self.assertTrue(str(reading["notes"][1]["heading"]).endswith("- standup"))

    def test_a_note_with_nothing_in_it_is_not_kept(self) -> None:
        reading = self.do({"action": "note", "heading": "standup", "body": "  "})
        self.assertEqual(len(reading["notes"]), 1)

    def test_a_note_is_replaced_whole(self) -> None:
        reading = self.do(
            {"action": "edit", "index": 1, "heading": "09:00 - idea", "body": "a month"}
        )
        self.assertEqual(reading["notes"][0]["body"], "a month")

    def test_a_note_is_removed_by_its_number(self) -> None:
        self.do({"action": "note", "heading": "standup", "body": "said it"})
        reading = self.do({"action": "unnote", "index": 1})
        self.assertEqual([note["body"] for note in reading["notes"]], ["said it"])
        self.assertNotIn("a calendar", self.entry())

    def test_an_edit_with_no_heading_keeps_the_one_the_note_has(self) -> None:
        reading = self.do({"action": "edit", "index": 1, "body": "a month"})
        self.assertEqual(reading["notes"][0]["heading"], "09:00 - idea")


class Program(unittest.TestCase):
    """The backend as the companion runs it: one text, through `python3 -c`."""

    def test_the_assembled_program_answers_a_spawn_payload(self) -> None:
        with tempfile.TemporaryDirectory() as room:
            here = Path(room) / "the-den"
            (here / "Daily").mkdir(parents=True)
            running = subprocess.Popen(
                [sys.executable, "-c", assemble("den")],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                text=True,
                env={**os.environ, "DEN_PATH": str(here)},
            )
            try:
                assert running.stdin is not None and running.stdout is not None
                running.stdin.write(json.dumps({"view": "notes"}) + "\n")
                running.stdin.flush()
                reading = json.loads(running.stdout.readline())
            finally:
                running.kill()
                running.wait(timeout=5)
                running.stdin.close()
                running.stdout.close()
        self.assertEqual(reading["view"], "notes")
        self.assertEqual(reading["notes"], [])
        self.assertFalse(reading["exists"])


if __name__ == "__main__":
    unittest.main()
