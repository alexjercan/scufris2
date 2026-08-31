"""Report the day the panel is looking at, out of the-den.

Three panels read this backend and each opens it with a different `view`, so
each is a process of its own: a backend is keyed by the payload it was opened
with, and `agenda`, `macros` and `notes` are three different questions.

The journal is read here, in this process. There used to be a `today` command
in front of it, and a panel that had to find a program on the path was a panel
that went blank when the path changed - which is how these three spent their
last weeks. The whole journal parses in under fifty milliseconds, so a click
costs a read rather than a process.

What the panel is compiled with is `tools/den/den.py`, named in `prelude`
beside this file. Its names are this file's names: there is no import, because
a backend is one program text handed to `python3 -c`.

The first line on standard input is the spawn payload:

    {"view": "agenda", "ahead": 5, "horizon": 60}
    {"view": "macros", "days": 30}
    {"view": "notes"}

Every line after it is an action:

    {"action": "select", "date": "2026-09-01"}
    {"action": "refresh"}
    {"action": "habit", "name": "Gym"}
    {"action": "task", "index": 2}
    {"action": "untask", "index": 2}
    {"action": "add", "text": "Call the dentist"}
    {"action": "idea", "text": "learn to weld"}
    {"action": "promote", "index": 1}
    {"action": "weight", "value": "81.4"}
    {"action": "food", "name": "chicken breast:g", "amount": "150"}
    {"action": "refood", "index": 1, "what": "rice 100g", "protein": "7", ...}
    {"action": "unfood", "index": 1}
    {"action": "lift", "exercise": "bench press", "sets": "60x8 60x8 60x6"}
    {"action": "relift", "was": "bench press", "exercise": "bench press", "sets": "60x8"}
    {"action": "split", "split": "push"}
    {"action": "note", "heading": "Standup", "body": "..."}
    {"action": "edit", "index": 2, "heading": "Standup", "body": "..."}
    {"action": "unnote", "index": 2}
    {"action": "search", "name": "chick"}

The ones that write are the panel's ticks and the words the person typed into
the form box the companion raises for them; a panel has no keyboard of its own.

Every write carries the revision the panel's reading was built from, so a day
that moved under the panel is refused and shown again rather than written over.

`search`, `splits` and `moves` are the odd ones: they write nothing and answer
in `choices`, which is what fills the list under a field in the form box while
it is typed.

Each line written is an object carrying `view`, the `date` it is about, the
real `today`, whether the entry `exists`, and `trouble` - a sentence when
something went wrong, and null when nothing did. Trouble arrives beside the day
rather than instead of it, because a habit that would not toggle is no reason
to blank a panel that was reading fine a moment ago.

A view carries what its panel draws and nothing else. The notes panel is sent
no macros, and the macros panel is sent no notes: a reading is an answer to one
question, not a copy of the journal.
"""

import json
import os
import queue
import sys
import threading
import time
from datetime import date, datetime

#: How often the selected day's file is looked at. An idle panel costs one
#: `stat` this often and nothing else.
BEAT = 5.0

#: How long a panel keeps a reading before building it again whatever the day's
#: file says. The day is watched by its own timestamp; what comes after it is
#: spread over as many files as there are days, and watching all of them to
#: save one pass a minute would be the wrong trade.
FLOOR = 60.0

#: How many later tasks the agenda names under the day's own.
AHEAD = 5

#: How far back the agenda looks for what was left undone.
BEHIND = 60

#: How far back the weight trend reaches.
DAYS = 30

#: How far back the workout suggestions reach. A movement not done in three
#: months is not one to offer first.
TRAINED = 90

#: How many candidates a field offers before the panel is too small for them.
CHOICES = 8

#: What a panel may ask to be. Anything else is an agenda, because a panel that
#: opened wrong is better than a panel that stayed empty.
VIEWS = ("agenda", "macros", "notes")


def deaf() -> None:
    """Points standard output at nothing.

    Catching the broken pipe is not enough on its own: the interpreter flushes
    standard output again on the way out and raises there too, past any handler,
    and prints the complaint on standard error - which the companion is reading
    and logging. Redirecting the descriptor makes the last flush a no-op.
    """
    devnull = os.open(os.devnull, os.O_WRONLY)
    os.dup2(devnull, sys.stdout.fileno())


class Refused(Exception):
    """Something the panel should say in a sentence rather than swallow."""


def whole(value: object, fallback: int, low: int, high: int) -> int:
    """Reads one bounded count out of the spawn payload."""
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return fallback
    return min(max(int(value), low), high)


def words(value: object) -> str:
    """Reads one line a person typed, which is a string or nothing at all."""
    return value.strip() if isinstance(value, str) else ""


def _merge(first: list[str], second: list[str]) -> list[str]:
    """One list after another, without repeating what is already in the first.

    Order is the whole point: what was trained recently comes before what the
    database merely knows about.
    """
    found = list(first)
    seen = {value.lower() for value in found}
    for value in second:
        if value.lower() not in seen:
            seen.add(value.lower())
            found.append(value)
    return found


def counted(value: object) -> int | None:
    """Reads a positive whole number out of an action, typed or clicked."""
    if isinstance(value, bool) or not isinstance(value, int) or value < 1:
        return None
    return value


class Panel:
    """The journal, the day the panel is on, and the last reading of it."""

    def __init__(self, spawn: dict[str, object]) -> None:
        self.den = resolve_den(None)
        self.database = resolve_database(None, self.den)
        self.exercises = resolve_exercises(None, self.den)
        self.ahead = whole(spawn.get("ahead"), AHEAD, 1, 50)
        self.behind = whole(spawn.get("horizon"), BEHIND, 1, 365)
        self.days = whole(spawn.get("days"), DAYS, 2, 365)
        #: The day asked for, or None while the panel is following the date.
        self.picked: str | None = None
        #: What each day was last read at, so a write can say what it expected.
        self.seen: dict[str, str] = {}
        self.backlog_at = ""
        self.stamp: float | None = None
        self.built = 0.0
        self.frame: dict[str, object] | None = None
        #: What the last suggestion matched, for the list under a field.
        self.choices: list[dict[str, str]] = []

    def now(self) -> str:
        """The real date, read again each time so a panel left up rolls over."""
        return datetime.now().astimezone().date().isoformat()

    def chosen(self) -> str:
        return self.picked or self.now()

    def path(self, selected: str) -> object:
        return entry_path(self.den, date.fromisoformat(selected))

    def moved(self, selected: str) -> bool:
        """Whether the day's entry changed since it was last looked at."""
        try:
            stamp: float | None = os.stat(self.path(selected)).st_mtime
        except OSError:
            stamp = None
        if stamp == self.stamp:
            return False
        self.stamp = stamp
        return True

    def forget(self) -> None:
        """Drops what was read, so the next beat reads again."""
        self.stamp = None
        self.frame = None

    def select(self, asked: object) -> None:
        """Puts the panel on one day, or back on the day it is."""
        if asked is None:
            self.picked = None
        elif isinstance(asked, str) and len(asked) == 10:
            try:
                date.fromisoformat(asked)
            except ValueError:
                return
            self.picked = asked
        # A suggestion was made for a box open over the day that was showing,
        # so moving the day drops its answer rather than offering it against
        # another one.
        self.choices = []
        self.forget()

    def revision_of(self, selected: str) -> str:
        """What the panel last saw this day at, reading it if it never has."""
        held = self.seen.get(selected)
        if held is not None:
            return held
        _day, current = read_day(self.den, date.fromisoformat(selected), create=False)
        self.seen[selected] = current
        return current

    def on_day(self, selected: str, rewrite) -> None:
        """Applies one change to the selected day, if it has not moved.

        The revision is the one the reading on screen was built from. A day
        that changed under the panel is refused rather than written over: the
        index that was clicked counted the tasks as they were, and they are no
        longer as they were.
        """
        target = date.fromisoformat(selected)
        try:
            _day, after = change(self.den, target, self.revision_of(selected), rewrite)
        except Conflict:
            self.seen.pop(selected, None)
            raise Refused("the day changed elsewhere - showing it again") from None
        except (LookupError, IndexError, ValueError) as trouble:
            raise Refused(str(trouble)) from None
        self.seen[selected] = after

    def act(self, action: dict[str, object]) -> str | None:
        """Carries out one action and returns what went wrong, if anything."""
        name = action.get("action")
        if name == "select":
            self.select(action.get("date"))
            return None
        if name == "refresh":
            self.forget()
            return None
        if name in ("search", "splits", "moves"):
            # Beside the writes rather than among them: they change nothing,
            # and the day they would otherwise read again has not moved.
            try:
                self.suggest(name, action)
            except Refused as refused:
                return str(refused)
            return None
        selected = self.chosen()
        try:
            self.write(name, selected, action)
        except Refused as refused:
            # The reading is kept. A habit that would not toggle is no reason
            # to blank a panel that was reading fine a moment ago, and the
            # sentence goes out beside the day rather than instead of it.
            self.forget()
            return str(refused)
        self.forget()
        return None

    def write(self, name: object, selected: str, action: dict[str, object]) -> None:
        """Dispatches one action that changes something."""
        if name == "habit":
            who = words(action.get("name"))
            if who:
                self.on_day(selected, lambda text: toggle_habit(text, who))
        elif name == "task":
            index = counted(action.get("index"))
            if index is not None:
                self.on_day(selected, lambda text: toggle_task(text, index))
        elif name == "add":
            text = words(action.get("text"))
            if text:
                self.on_day(selected, lambda held: add_task(held, text))
        elif name == "weight":
            self.weigh(selected, action)
        elif name == "food":
            self.eat(selected, action)
        elif name == "lift":
            self.train(selected, action)
        elif name == "split":
            self.name_split(selected, action)
        elif name == "relift":
            self.retrain(selected, action)
        elif name == "unlift":
            index = counted(action.get("index"))
            if index is not None:
                self.on_day(selected, lambda text: remove_row(text, "workout", index))
        elif name == "untask":
            index = counted(action.get("index"))
            if index is not None:
                self.on_day(selected, lambda text: remove_task(text, index))
        elif name == "refood":
            self.relabel(selected, action)
        elif name == "unfood":
            index = counted(action.get("index"))
            if index is not None:
                self.on_day(selected, lambda text: remove_row(text, "macros", index))
        elif name == "note":
            self.remember(selected, action)
        elif name == "edit":
            self.rewrite(selected, action)
        elif name == "unnote":
            index = counted(action.get("index"))
            if index is not None:
                self.on_day(selected, lambda text: remove_note(text, index))
        elif name == "idea":
            self.imagine(action)
        elif name == "promote":
            self.pull(selected, action)
        elif name == "unidea":
            index = counted(action.get("index"))
            if index is not None:
                self.backlog(lambda text: remove_idea(text, index))

    def weigh(self, selected: str, action: dict[str, object]) -> None:
        """Logs the day's weight.

        An empty field is the person changing their mind after the box was
        already up, so it writes nothing. Words that are not a number are a
        mistake worth saying out loud.
        """
        said = words(action.get("value"))
        if not said:
            return
        try:
            value = normalize_weight(said)
        except ValueError:
            raise Refused("a weight is a number of kilograms") from None
        self.on_day(selected, lambda text: set_weight(text, value))

    def remember(self, selected: str, action: dict[str, object]) -> None:
        """Keeps one structured note, with a heading when there is one."""
        body = words(action.get("body"))
        if not body:
            return
        try:
            heading = note_heading(words(action.get("heading")) or None, datetime.now())
        except ValueError as trouble:
            raise Refused(str(trouble)) from None
        self.on_day(selected, lambda text: add_note(text, heading, body))

    def rewrite(self, selected: str, action: dict[str, object]) -> None:
        """Replaces one structured note that is already in the day.

        An empty heading keeps the one the note has: the box opens on the note
        as it stands, so an empty heading is a note that never had one.
        """
        index = counted(action.get("index"))
        if index is None:
            return
        body = words(action.get("body"))
        if not body:
            raise Refused("a note with nothing in it is a note to remove")
        heading = words(action.get("heading"))
        if not heading:
            held = self.frame or {}
            notes = held.get("notes") if isinstance(held, dict) else None
            found = notes[index - 1] if isinstance(notes, list) and index <= len(notes) else {}
            heading = words(found.get("heading")) if isinstance(found, dict) else ""
        if not heading:
            raise Refused("a note keeps its heading")
        self.on_day(selected, lambda text: edit_note(text, index, heading, body))

    def imagine(self, action: dict[str, object]) -> None:
        """Puts one idea in the backlog, which is what has no day yet."""
        text = words(action.get("text"))
        if text:
            self.backlog(lambda held: add_idea(held, text))

    def backlog(self, rewrite) -> None:
        """Applies one change to the backlog, if it has not moved."""
        try:
            _ideas, after = change_backlog(self.den, self.backlog_at, rewrite)
        except Conflict:
            self.backlog_at = ""
            raise Refused("the backlog changed elsewhere - showing it again") from None
        except (IndexError, ValueError) as trouble:
            raise Refused(str(trouble)) from None
        self.backlog_at = after

    def pull(self, selected: str, action: dict[str, object]) -> None:
        """Moves one backlog item onto the day that is showing."""
        index = counted(action.get("index"))
        if index is None:
            return
        try:
            _day, after, _ideas, left = promote(
                self.den,
                index,
                date.fromisoformat(selected),
                self.backlog_at,
                self.revision_of(selected),
            )
        except Conflict as trouble:
            self.backlog_at = ""
            self.seen.pop(selected, None)
            raise Refused(f"{trouble} - showing it again") from None
        except (IndexError, ValueError) as trouble:
            raise Refused(str(trouble)) from None
        self.seen[selected] = after
        self.backlog_at = left

    def suggest(self, name: object, action: dict[str, object]) -> None:
        """Answers what a field could be, from what has been typed so far.

        The only actions here that change nothing. What they produce is the
        list under a field in the form box, so they hold the candidates and let
        the next reading carry them - the same road every other answer takes.

        A food comes from the database, because that is what knows the numbers.
        A split and a movement come from the journal first and the exercise
        database second: what was trained recently is what is most likely
        wanted, and the database is what can answer before anything has been
        trained at all.
        """
        self.choices = []
        if name == "search":
            found = [item.choice() for item in self.foods(words(action.get("name")))]
            self.choices = found[:CHOICES]
            return
        history = lift_history(self.den, date.fromisoformat(self.chosen()), TRAINED)
        # Each field is typed into on its own, and the form box lays only that
        # field's own text into the action, so a movement is narrowed by what
        # was typed into its own field and by nothing else on the form.
        known = Exercises.load(self.exercises)
        if name == "splits":
            typed = words(action.get("split"))
            found = _merge(splits_used(history), known.splits())
        else:
            typed = words(action.get("exercise"))
            # The split comes from the day on screen rather than from the
            # form, so a pull day offers pulling movements first. A caller that
            # knows better may name one instead.
            under = words(action.get("split")) or self.showing_split()
            found = _merge(exercises_used(history, under or None), known.names(under))
        wanted = typed.lower()
        self.choices = [
            {"id": value, "label": value}
            for value in found
            if not wanted or wanted in value.lower()
        ][:CHOICES]

    def showing_split(self) -> str:
        """The split the day on screen is, out of the reading it was drawn from.

        Read off the last frame rather than off the file: it is what the person
        is looking at, and a suggestion costs no read of its own.
        """
        said = self.frame or {}
        written = said.get("split") if said.get("date") == self.chosen() else None
        return written if isinstance(written, str) else ""

    def foods(self, typed: str) -> list[object]:
        """The database rows one query matched, in the order it ranked them."""
        if not typed:
            return []
        try:
            return Database.load(self.database).query(typed)
        except ValueError as trouble:
            raise Refused(str(trouble)) from None

    def eat(self, selected: str, action: dict[str, object]) -> None:
        """Logs one food, by database id or by words that name exactly one.

        The name is a database id rather than a row, because the row is a thing
        to compose rather than to type. A taken candidate answers with an id and
        goes straight through. Words the person typed and never picked from are
        looked up, and a search matching more than one row is said out loud
        rather than guessed at: the list was under the field the whole time.
        """
        self.choices = []
        typed = words(action.get("name"))
        if not typed:
            return
        amount = words(action.get("amount")) or action.get("amount")
        try:
            size = float(amount) if amount is not None else 0.0
        except (TypeError, ValueError):
            size = 0.0
        if not size > 0:
            raise Refused("an amount is a number of grams or pieces")
        found = self.foods(typed)
        wanted = typed.lower()
        exact = [item for item in found if item.id == wanted]
        if exact:
            chosen = exact[0]
        elif len(found) == 1:
            chosen = found[0]
        elif not found:
            raise Refused(f"no food matching {typed}")
        else:
            raise Refused(f"{len(found)} foods match {typed} - pick one")
        try:
            row = Database.load(self.database).calculate(chosen.id, size).to_row()
        except ValueError as trouble:
            raise Refused(str(trouble)) from None
        self.on_day(selected, lambda text: add_row(text, "macros", normalize_food(row)))

    def relabel(self, selected: str, action: dict[str, object]) -> None:
        """Writes one food row over the one that is there.

        The four cells the row is, rather than a name and an amount: a row that
        was logged is a row that may have been scaled from a food the database
        no longer holds, and a correction has to reach it either way.
        """
        self.choices = []
        index = counted(action.get("index"))
        if index is None:
            return
        cells = [words(action.get(cell)) for cell in ("what", "protein", "carbs", "fat")]
        if not cells[0]:
            raise Refused("a food row keeps its name")
        try:
            row = normalize_food(",".join(cells))
        except ValueError as trouble:
            raise Refused(str(trouble)) from None
        self.on_day(selected, lambda text: edit_row(text, "macros", index, row))

    def train(self, selected: str, action: dict[str, object]) -> None:
        """Logs every set of one movement, in one edit.

        `60x8 60x8 60x7` is three sets of the same movement, which is how a
        workout is done and how the panel prints it back. A split named beside
        them is the day's, and is written only when the day has none: a day is
        one split, and the box asks for it only on the first set of the day.
        """
        self.choices = []
        exercise = words(action.get("exercise"))
        if not exercise:
            return
        try:
            sets = parse_sets(words(action.get("sets")))
            rows = [normalize_lift(exercise, load, reps) for load, reps in sets]
            named = words(action.get("split"))
            split = normalize_split(named) if named else ""
        except ValueError as trouble:
            raise Refused(str(trouble)) from None

        def rewrite(text: str) -> str:
            # Named or left alone. A split written over one the day already had
            # is a day that changed under the box, which the revision the write
            # carries is what refuses.
            written = set_split(text, split) if split else text
            for row in rows:
                written = add_row(written, "workout", row)
            return written

        self.on_day(selected, rewrite)

    def retrain(self, selected: str, action: dict[str, object]) -> None:
        """Writes over every set of one movement, in one edit.

        The panel prints a movement as one line and the box asks for it back
        the same way, so a set added, a weight corrected and a set dropped are
        the same answer. An empty one removes the movement: the sets were in
        the field to be cleared, which no accident does. `was` is the movement
        as it was drawn, so renaming it in the box is a rename rather than a
        second movement.
        """
        self.choices = []
        was = words(action.get("was"))
        if not was:
            return
        typed = words(action.get("sets"))
        named = words(action.get("exercise")) or was
        try:
            rows = [
                normalize_lift(named, load, reps) for load, reps in parse_sets(typed)
            ] if typed else []
        except ValueError as trouble:
            raise Refused(str(trouble)) from None
        self.on_day(selected, lambda text: set_rows(text, "workout", was, rows))

    def name_split(self, selected: str, action: dict[str, object]) -> None:
        """Names the split the day is, which every set of it belongs to."""
        self.choices = []
        named = words(action.get("split"))
        if not named:
            return
        try:
            split = normalize_split(named)
        except ValueError as trouble:
            raise Refused(str(trouble)) from None
        self.on_day(selected, lambda text: set_split(text, split))

    def read(self, view: str) -> dict[str, object]:
        """The reading for this beat, built again only when it has to be."""
        selected = self.chosen()
        said = self.frame or {}
        stale = (
            self.frame is None
            or said.get("date") != selected
            or said.get("today") != self.now()
            or time.monotonic() - self.built > FLOOR
        )
        if self.moved(selected) or stale:
            self.built = time.monotonic()
            try:
                self.frame = self.build(view, selected)
            except OSError as trouble:
                self.frame = self.bare(view, selected, str(trouble))
        reading = dict(self.frame or {})
        if view == "macros":
            # Laid on the reading rather than built into it. A suggestion is
            # not part of the day, and one that made the day stale would cost a
            # whole reading per keystroke.
            reading["choices"] = self.choices
        return reading

    def bare(self, view: str, selected: str, trouble: str | None) -> dict[str, object]:
        """What every reading says, whether or not the day could be read."""
        return {
            "view": view,
            "date": selected,
            "today": self.now(),
            "exists": False,
            "trouble": trouble,
        }

    def build(self, view: str, selected: str) -> dict[str, object]:
        """Reads one day and shapes it for one panel, and nothing besides.

        Nothing is created by a reading. A panel browsing a month must not
        leave a month of empty entries behind it, so the day is read as it is
        and an absent one is an empty day rather than a new file.
        """
        frame = self.bare(view, selected, None)
        target = date.fromisoformat(selected)
        day, current = read_day(self.den, target, create=False)
        self.seen[selected] = current
        frame["exists"] = current != ""
        if view == "agenda":
            frame.update(self.agenda(target, day))
        elif view == "macros":
            frame.update(self.macros(target, day))
        else:
            frame["notes"] = [note.to_dict() for note in day.notes]
        return frame

    def agenda(self, target: date, day: object) -> dict[str, object]:
        """The day in full, then what is late, then what is coming.

        Both lists are asked from the earlier of the day and the date, so one
        pass serves the marks on the calendar and the lists under it, and a day
        in the past still shows what stands between it and now.
        """
        edge = min(target, date.fromisoformat(self.now()))
        late = restant(self.den, edge, self.behind)
        later = upcoming(self.den, edge)
        ideas, self.backlog_at = read_backlog(self.den)
        selected = target.isoformat()
        marks = {task.date for task in late} | {task.date for task in later}
        if any(not task.done for task in day.tasks):
            marks.add(selected)
        return {
            "habits": [habit.to_dict() for habit in day.habits],
            "tasks": [task.to_dict() for task in day.tasks],
            "restant": [task.to_dict() for task in late if task.date != selected],
            "ahead": [
                task.to_dict() for task in later if task.date > selected
            ][: self.ahead],
            "backlog": [idea.to_dict() for idea in ideas if not idea.done],
            "marks": sorted(marks),
        }

    def macros(self, target: date, day: object) -> dict[str, object]:
        """What the day was eaten, weighed and lifted.

        One day, three things that belong to it. The weight is the one figure
        that means nothing alone, so a month of them comes with it.
        """
        trend = weight_history(self.den, target, self.days)
        return {
            "macros": day.macros.to_dict(),
            "foods": [food.to_dict() for food in day.foods],
            "weight": day.weight,
            "change": round(trend[-1][1] - trend[0][1], 1) if len(trend) > 1 else None,
            "recent": [{"date": when, "weight": value} for when, value in trend],
            "lifts": [lift.to_dict() for lift in day.lifts],
            "split": day.split,
            "volume": round(sum(lift.volume for lift in day.lifts), 1),
        }


class Mail:
    """Actions from the widget, and a nudge to stop waiting for them.

    Every read happens on the one thread that reports, so nothing here holds a
    lock across a reading: a click hands over a line and wakes the reporter,
    which is what makes a tick land at once instead of on the beat.
    """

    def __init__(self) -> None:
        self.posted: queue.Queue[dict[str, object]] = queue.Queue()
        self.wake = threading.Event()

    def post(self, action: dict[str, object]) -> None:
        self.posted.put(action)
        self.wake.set()

    def take(self) -> list[dict[str, object]]:
        """Everything asked for since the last beat, in the order asked."""
        actions: list[dict[str, object]] = []
        while True:
            try:
                actions.append(self.posted.get_nowait())
            except queue.Empty:
                return actions

    def pause(self, seconds: float) -> None:
        """Waits out a beat, or until something is asked for."""
        self.wake.wait(seconds)
        self.wake.clear()


def listen(mail: Mail) -> None:
    """Reads actions until standard input ends."""
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            action = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(action, dict):
            mail.post(action)


def main() -> None:
    spawn = json.loads(sys.stdin.readline() or "null") or {}
    view = spawn.get("view")
    view = view if view in VIEWS else "agenda"
    panel = Panel(spawn)
    mail = Mail()

    # A daemon thread, so a panel that is taken down ends here rather than
    # waiting on a read only the companion can end.
    threading.Thread(target=listen, args=(mail,), daemon=True).start()

    while True:
        refused = [panel.act(action) for action in mail.take()]
        reading = panel.read(view)
        for trouble in refused:
            if trouble:
                reading["trouble"] = trouble
        try:
            print(json.dumps(reading, ensure_ascii=False), flush=True)
        except BrokenPipeError:
            # The companion took the panel down. There is nobody to read for.
            deaf()
            return
        mail.pause(BEAT)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
