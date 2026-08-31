"""What the den library makes of a journal, on a journal of its own.

Every test builds a den under a temporary directory. Nothing here reads
`$DEN_PATH` and nothing touches the real journal: this library is the only
thing that understands the format, so what is measured is the format.
"""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from datetime import date, datetime
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[1]
LIBRARY = REPOSITORY / "tools" / "den" / "den.py"

_spec = importlib.util.spec_from_file_location("den", LIBRARY)
assert _spec is not None and _spec.loader is not None
den = importlib.util.module_from_spec(_spec)
# Registered before it is run: `dataclass` looks the defining module up by name
# while it builds each class, and a module that is not there yet fails there.
sys.modules["den"] = den
_spec.loader.exec_module(den)


DAY = """# Monday, August 31, 2026

### Tasks

- [x] call the dentist
- [ ] write the plan

### Habits

- [x] Learn
- [ ] Gym

### Macros

what,protein,carbs,fat
chicken breast 150g,46.5,0,5.4
rice 100g,7,78,0.6

### Weight

71.2 kg

### Workout

split,exercise,weight,reps
Push,bench press,60,8
Push,bench press,60,7
Pull,barbell row,50,10

### Notes

#### 22:04 - standup

what was said
over two lines

#### 22:30

no title here
"""

#: An entry as they were written before there was a Workout section. Years of
#: them exist, so a write into one has to put the section back.
OLD = """# Sunday, August 30, 2026

### Tasks

- [ ] something

### Habits

- [ ] Gym

### Macros

what,protein,carbs,fat

### Weight

### Notes
"""


class Den(unittest.TestCase):
    """A den under a temporary directory, with a template and no entries."""

    def setUp(self) -> None:
        self.room = tempfile.TemporaryDirectory()
        self.addCleanup(self.room.cleanup)
        self.den = Path(self.room.name) / "the-den"
        (self.den / "Daily").mkdir(parents=True)
        (self.den / "Templates").mkdir()
        (self.den / "Templates" / "daily.md").write_text(
            "# {{title}}\n\n### Tasks\n\n### Habits\n\n### Macros\n\n"
            "what,protein,carbs,fat\n\n### Weight\n\n### Notes\n",
            encoding="utf-8",
        )

    def write(self, day: date, text: str) -> Path:
        path = den.entry_path(self.den, day)
        path.write_text(text, encoding="utf-8")
        return path


class ParseDay(Den):
    def setUp(self) -> None:
        super().setUp()
        self.path = self.write(date(2026, 8, 31), DAY)
        self.day = den.parse_day(self.path)

    def test_reads_the_title_and_the_stem(self) -> None:
        self.assertEqual(self.day.title, "Monday, August 31, 2026")
        self.assertEqual(self.day.date, "2026-08-31-Monday")

    def test_reads_tasks_in_order_with_their_state(self) -> None:
        self.assertEqual(
            [(task.index, task.text, task.done) for task in self.day.tasks],
            [(1, "call the dentist", True), (2, "write the plan", False)],
        )

    def test_reads_habits(self) -> None:
        self.assertEqual(
            [(habit.name, habit.done) for habit in self.day.habits],
            [("Learn", True), ("Gym", False)],
        )

    def test_totals_the_macros_and_the_calories(self) -> None:
        self.assertEqual(len(self.day.foods), 2)
        self.assertAlmostEqual(self.day.macros.protein, 53.5)
        self.assertAlmostEqual(self.day.macros.carbs, 78.0)
        self.assertAlmostEqual(self.day.macros.fat, 6.0)
        self.assertEqual(self.day.macros.calories, round(53.5 * 4 + 78 * 4 + 6 * 9))

    def test_reads_the_weight(self) -> None:
        self.assertEqual(self.day.weight, 71.2)

    def test_reads_every_set_and_the_splits_behind_them(self) -> None:
        self.assertEqual(
            [(lift.split, lift.exercise, lift.weight, lift.reps) for lift in self.day.lifts],
            [
                ("Push", "bench press", 60.0, 8),
                ("Push", "bench press", 60.0, 7),
                ("Pull", "barbell row", 50.0, 10),
            ],
        )
        self.assertEqual(self.day.splits, ["Push", "Pull"])
        self.assertEqual(self.day.lifts[0].volume, 480.0)

    def test_reads_notes_as_blocks_with_their_bodies(self) -> None:
        self.assertEqual(
            [(note.index, note.heading) for note in self.day.notes],
            [(1, "22:04 - standup"), (2, "22:30")],
        )
        self.assertEqual(self.day.notes[0].body, "what was said\nover two lines")


class ParseEdges(Den):
    def test_missing_sections_read_as_empty(self) -> None:
        path = self.write(date(2026, 8, 31), "# Monday\n")
        day = den.parse_day(path)
        self.assertEqual(day.tasks, [])
        self.assertEqual(day.lifts, [])
        self.assertIsNone(day.weight)
        self.assertEqual(day.macros.calories, 0)

    def test_two_weights_are_a_file_being_edited_and_read_as_none(self) -> None:
        path = self.write(date(2026, 8, 31), "# M\n\n### Weight\n\n70 kg\n71 kg\n")
        self.assertIsNone(den.parse_day(path).weight)

    def test_half_written_rows_are_passed_over(self) -> None:
        path = self.write(
            date(2026, 8, 31),
            "# M\n\n### Workout\n\nsplit,exercise,weight,reps\n"
            "Push,bench press,60\nPush,bench press,heavy,8\n"
            "Push,bench press,60,0\nPush,bench press,60,8\n",
        )
        lifts = den.parse_day(path).lifts
        self.assertEqual([(lift.index, lift.reps) for lift in lifts], [(1, 8)])

    def test_a_bodyweight_set_is_a_set(self) -> None:
        path = self.write(
            date(2026, 8, 31),
            "# M\n\n### Workout\n\nsplit,exercise,weight,reps\nPull,pull up,0,12\n",
        )
        self.assertEqual(den.parse_day(path).lifts[0].weight, 0.0)


class Sections(unittest.TestCase):
    def test_a_missing_section_is_put_back_above_the_one_after_it(self) -> None:
        written = den.ensure_section(OLD, "workout")
        self.assertIn("### Workout", written)
        self.assertLess(written.index("### Weight"), written.index("### Workout"))
        self.assertLess(written.index("### Workout"), written.index("### Notes"))
        self.assertIn("split,exercise,weight,reps", written)

    def test_a_section_that_is_there_is_left_alone(self) -> None:
        self.assertEqual(den.ensure_section(DAY, "workout"), DAY)


class Edits(unittest.TestCase):
    def test_a_task_is_added_under_the_ones_already_there(self) -> None:
        written = den.add_task(DAY, "buy milk")
        tasks = written.split("### Habits")[0]
        self.assertTrue(tasks.rstrip().endswith("- [ ] buy milk"))

    def test_a_task_is_ticked_by_its_number(self) -> None:
        self.assertIn("- [x] write the plan", den.toggle_task(DAY, 2))

    def test_ticking_a_task_that_is_not_there_is_refused(self) -> None:
        with self.assertRaises(IndexError):
            den.toggle_task(DAY, 9)

    def test_a_task_is_removed_by_its_number(self) -> None:
        self.assertNotIn("call the dentist", den.remove_task(DAY, 1))

    def test_a_habit_is_ticked_by_name(self) -> None:
        self.assertIn("- [x] Gym", den.toggle_habit(DAY, "gym"))

    def test_a_habit_is_ticked_without_the_icon_it_is_written_with(self) -> None:
        text = "# M\n\n### Habits\n\n- [ ] * Gym\n"
        self.assertIn("- [x] * Gym", den.toggle_habit(text, "Gym"))

    def test_a_weight_replaces_the_one_written_before_it(self) -> None:
        written = den.set_weight(DAY, "70.4")
        self.assertIn("70.4 kg", written)
        self.assertNotIn("71.2 kg", written)

    def test_a_set_is_added_to_a_day_that_had_no_workout_section(self) -> None:
        written = den.add_row(OLD, "workout", "Legs,squat,80,5")
        self.assertIn("Legs,squat,80,5", written)
        self.assertLess(written.index("### Workout"), written.index("### Notes"))

    def test_a_set_is_removed_by_its_number(self) -> None:
        written = den.remove_row(DAY, "workout", 2)
        self.assertEqual(written.count("Push,bench press"), 1)

    def test_a_note_is_added_after_the_notes_already_there(self) -> None:
        written = den.add_note(DAY, "23:00 - later", "one more thing")
        self.assertTrue(written.rstrip().endswith("one more thing"))

    def test_a_note_is_replaced_whole(self) -> None:
        written = den.edit_note(DAY, 1, "22:04 - standup", "said something else")
        self.assertIn("said something else", written)
        self.assertNotIn("what was said", written)
        self.assertIn("#### 22:30", written)

    def test_a_note_is_removed_by_its_number(self) -> None:
        written = den.remove_note(DAY, 1)
        self.assertNotIn("standup", written)
        self.assertIn("no title here", written)


class Validation(unittest.TestCase):
    def test_a_weight_is_written_with_a_decimal(self) -> None:
        self.assertEqual(den.normalize_weight("70"), "70.0")
        self.assertEqual(den.normalize_weight(" 70.4 kg "), "70.4")

    def test_words_are_not_a_weight(self) -> None:
        with self.assertRaises(ValueError):
            den.normalize_weight("heavy")

    def test_a_food_row_is_checked_before_it_is_written(self) -> None:
        self.assertEqual(den.normalize_food(" eggs , 12 ,1, 10 "), "eggs,12,1,10")
        for bad in ("eggs,12,1", "eggs,a,1,10", "what,1,1,1"):
            with self.assertRaises(ValueError):
                den.normalize_food(bad)

    def test_a_set_is_checked_before_it_is_written(self) -> None:
        self.assertEqual(
            den.normalize_lift("Push", "bench press", "60.0", "8"),
            "Push,bench press,60,8",
        )
        with self.assertRaises(ValueError):
            den.normalize_lift("Push", "bench press", "-1", "8")
        with self.assertRaises(ValueError):
            den.normalize_lift("Push", "bench press", "60", "8.5")
        with self.assertRaises(ValueError):
            den.normalize_lift("Push", "bench, press", "60", "8")

    def test_a_note_is_stamped_with_the_time_it_was_written(self) -> None:
        when = datetime(2026, 8, 31, 22, 4)
        self.assertEqual(den.note_heading(None, when), "22:04")
        self.assertEqual(den.note_heading("standup", when), "22:04 - standup")


class Store(Den):
    def test_a_day_is_made_from_the_template_when_it_is_read(self) -> None:
        day, current = den.read_day(self.den, date(2026, 8, 31))
        self.assertTrue(den.entry_path(self.den, date(2026, 8, 31)).is_file())
        self.assertEqual(day.title, "Monday, August 31, 2026")
        self.assertNotEqual(current, "")

    def test_reading_without_creating_leaves_no_file_behind(self) -> None:
        day, current = den.read_day(self.den, date(2026, 8, 31), create=False)
        self.assertFalse(den.entry_path(self.den, date(2026, 8, 31)).exists())
        self.assertEqual(current, "")
        self.assertEqual(day.tasks, [])

    def test_a_change_carries_the_revision_forward(self) -> None:
        day, current = den.read_day(self.den, date(2026, 8, 31))
        after, later = den.change(
            self.den, date(2026, 8, 31), current, lambda text: den.add_task(text, "go")
        )
        self.assertEqual([task.text for task in after.tasks], ["go"])
        self.assertNotEqual(later, current)

    def test_a_change_against_a_stale_revision_is_refused(self) -> None:
        day, current = den.read_day(self.den, date(2026, 8, 31))
        den.change(
            self.den, date(2026, 8, 31), current, lambda text: den.add_task(text, "one")
        )
        with self.assertRaises(den.Conflict):
            den.change(
                self.den,
                date(2026, 8, 31),
                current,
                lambda text: den.add_task(text, "two"),
            )

    def test_a_caller_that_saw_no_file_may_make_it(self) -> None:
        day, current = den.read_day(self.den, date(2026, 9, 1), create=False)
        after, _later = den.change(
            self.den, date(2026, 9, 1), current, lambda text: den.add_task(text, "go")
        )
        self.assertEqual([task.text for task in after.tasks], ["go"])

    def test_a_caller_that_saw_no_file_loses_to_whoever_made_it(self) -> None:
        _day, current = den.read_day(self.den, date(2026, 9, 1), create=False)
        den.ensure_day(self.den, date(2026, 9, 1))
        with self.assertRaises(den.Conflict):
            den.change(
                self.den,
                date(2026, 9, 1),
                current,
                lambda text: den.add_task(text, "go"),
            )

    def test_a_set_is_logged_through_the_store(self) -> None:
        self.write(date(2026, 8, 30), OLD)
        _day, current = den.read_day(self.den, date(2026, 8, 30))
        after, _later = den.add_lift(
            self.den, date(2026, 8, 30), "Legs", "squat", "80", "5", current
        )
        self.assertEqual(after.lifts[0].to_dict()["exercise"], "squat")


class Backlog(Den):
    def test_a_den_with_no_backlog_reads_as_empty(self) -> None:
        ideas, current = den.read_backlog(self.den)
        self.assertEqual(ideas, [])
        self.assertEqual(current, "")

    def test_an_idea_is_added_to_a_backlog_that_did_not_exist(self) -> None:
        _ideas, current = den.read_backlog(self.den)
        after, _later = den.change_backlog(
            self.den, current, lambda text: den.add_idea(text, "learn welding")
        )
        self.assertEqual([idea.text for idea in after], ["learn welding"])
        self.assertTrue(den.backlog_path(self.den).is_file())

    def test_ideas_keep_their_order_and_their_state(self) -> None:
        den.backlog_path(self.den).write_text(
            "# Backlog\n\n- [ ] one\n- [x] two\n", encoding="utf-8"
        )
        ideas, _current = den.read_backlog(self.den)
        self.assertEqual(
            [(idea.index, idea.text, idea.done) for idea in ideas],
            [(1, "one", False), (2, "two", True)],
        )

    def test_an_idea_moved_onto_a_day_leaves_the_backlog(self) -> None:
        _ideas, backlog = den.change_backlog(
            self.den, "", lambda text: den.add_idea(text, "learn welding")
        )
        _day, current = den.read_day(self.den, date(2026, 8, 31))
        day, _after, left, _later = den.promote(
            self.den, 1, date(2026, 8, 31), backlog, current
        )
        self.assertEqual([task.text for task in day.tasks], ["learn welding"])
        self.assertEqual(left, [])

    def test_a_stale_backlog_is_refused(self) -> None:
        _ideas, backlog = den.change_backlog(
            self.den, "", lambda text: den.add_idea(text, "one")
        )
        den.change_backlog(self.den, backlog, lambda text: den.add_idea(text, "two"))
        with self.assertRaises(den.Conflict):
            den.change_backlog(
                self.den, backlog, lambda text: den.add_idea(text, "three")
            )


class Queries(Den):
    def setUp(self) -> None:
        super().setUp()
        self.write(date(2026, 8, 20), "# T\n\n### Tasks\n\n- [ ] long overdue\n")
        self.write(date(2026, 8, 30), "# S\n\n### Tasks\n\n- [x] done\n- [ ] left\n")
        self.write(date(2026, 9, 2), "# W\n\n### Tasks\n\n- [ ] later\n")

    def test_upcoming_names_unfinished_tasks_after_a_day(self) -> None:
        found = den.upcoming(self.den, date(2026, 8, 31))
        self.assertEqual([(task.date, task.text) for task in found], [("2026-09-02", "later")])

    def test_restant_names_what_was_left_behind(self) -> None:
        found = den.restant(self.den, date(2026, 8, 31), horizon=5)
        self.assertEqual([(task.date, task.text) for task in found], [("2026-08-30", "left")])

    def test_restant_stops_at_the_horizon(self) -> None:
        near = den.restant(self.den, date(2026, 8, 31), horizon=5)
        far = den.restant(self.den, date(2026, 8, 31), horizon=30)
        self.assertEqual([task.text for task in near], ["left"])
        self.assertEqual([task.text for task in far], ["long overdue", "left"])

    def test_a_query_creates_nothing(self) -> None:
        before = sorted(path.name for path in (self.den / "Daily").glob("*.md"))
        den.restant(self.den, date(2026, 8, 31), horizon=30)
        den.upcoming(self.den, date(2026, 8, 31))
        after = sorted(path.name for path in (self.den / "Daily").glob("*.md"))
        self.assertEqual(before, after)

    def test_weight_history_holds_only_the_days_that_were_weighed(self) -> None:
        self.write(date(2026, 8, 29), "# F\n\n### Weight\n\n71.5 kg\n")
        self.write(date(2026, 8, 31), "# M\n\n### Weight\n\n71.2 kg\n")
        self.assertEqual(
            den.weight_history(self.den, date(2026, 8, 31), 7),
            [("2026-08-29", 71.5), ("2026-08-31", 71.2)],
        )


class Workouts(Den):
    def setUp(self) -> None:
        super().setUp()
        self.write(
            date(2026, 8, 28),
            "# F\n\n### Workout\n\nsplit,exercise,weight,reps\nPull,barbell row,50,10\n",
        )
        self.write(
            date(2026, 8, 31),
            "# M\n\n### Workout\n\nsplit,exercise,weight,reps\n"
            "Push,bench press,60,8\nPush,overhead press,35,10\n",
        )
        self.history = den.lift_history(self.den, date(2026, 8, 31), 7)

    def test_history_is_newest_first_and_skips_rest_days(self) -> None:
        self.assertEqual([day for day, _lifts in self.history], ["2026-08-31", "2026-08-28"])

    def test_splits_are_offered_most_recent_first(self) -> None:
        self.assertEqual(den.splits_used(self.history), ["Push", "Pull"])

    def test_exercises_can_be_narrowed_to_one_split(self) -> None:
        self.assertEqual(
            den.exercises_used(self.history), ["bench press", "overhead press", "barbell row"]
        )
        self.assertEqual(den.exercises_used(self.history, "pull"), ["barbell row"])

    def test_the_last_set_of_a_movement_is_what_the_next_is_judged_on(self) -> None:
        found = den.last_lift(self.history, "Bench Press")
        assert found is not None
        self.assertEqual((found.weight, found.reps), (60.0, 8))
        self.assertIsNone(den.last_lift(self.history, "deadlift"))


class Foods(unittest.TestCase):
    def setUp(self) -> None:
        self.room = tempfile.TemporaryDirectory()
        self.addCleanup(self.room.cleanup)
        self.path = Path(self.room.name) / "macros.csv"
        self.path.write_text(
            "chicken breast 100g,31,0,3.6\negg 1pc,6,0,5\nnonsense\n", encoding="utf-8"
        )
        self.database = den.Database.load(self.path)

    def test_rows_that_do_not_parse_are_passed_over(self) -> None:
        self.assertEqual(
            sorted(self.database.items), ["chicken breast:g", "egg:pc"]
        )

    def test_a_query_matches_letters_in_order_and_ranks_prefixes_first(self) -> None:
        found = self.database.query("chbr")
        self.assertEqual([item.id for item in found], ["chicken breast:g"])
        self.assertEqual(found[0].label, "chicken breast (g)")

    def test_an_empty_query_is_refused(self) -> None:
        with self.assertRaises(ValueError):
            self.database.query("  ")

    def test_a_food_is_scaled_to_an_amount(self) -> None:
        found = self.database.calculate("chicken breast:g", 150)
        self.assertAlmostEqual(float(found.to_dict()["protein"]), 46.5)
        self.assertEqual(found.to_row(), "chicken breast 150g,46.5,0,5.4")

    def test_an_unknown_food_is_refused(self) -> None:
        with self.assertRaises(ValueError):
            self.database.calculate("tofu:g", 100)

    def test_a_food_is_appended_in_canonical_form(self) -> None:
        den.insert_item(self.path, "Apple 1 pc,0.3,25,0.2".replace(" pc", "pc"))
        rows = self.path.read_text(encoding="utf-8").splitlines()
        self.assertEqual(rows[-1], "Apple 1pc,0.3,25,0.2")

    def test_a_database_that_does_not_end_in_a_newline_still_appends(self) -> None:
        self.path.write_text("egg 1pc,6,0,5", encoding="utf-8")
        den.insert_item(self.path, "oats 100g,13,68,7")
        self.assertEqual(
            self.path.read_text(encoding="utf-8").splitlines(),
            ["egg 1pc,6,0,5", "oats 100g,13,68,7"],
        )


if __name__ == "__main__":
    unittest.main()
