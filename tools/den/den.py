"""The-den journal, read and written in this process.

The-den is a directory of Markdown: one file per day under
`Daily/YYYY-MM-DD-Weekday.md`, and one `Backlog.md` beside it. This module is
the only thing that understands that shape. Everything that reads or writes the
journal - the desktop panels, the agent's command line - goes through here, so
the format lives in one place and a half-written entry fails in one place.

There is no external program behind this. A panel that had to find a command on
the path was a panel that went blank when the path changed, and one subprocess
per click cost more than parsing the whole journal does.

A day is five sections, and a missing one reads as empty:

    # Monday, August 31, 2026

    ### Tasks
    - [ ] call the dentist

    ### Habits
    - [x] Learn

    ### Macros
    what,protein,carbs,fat
    chicken breast 150g,46.5,0,5.4

    ### Weight
    71.2 kg

    ### Workout
    split,exercise,weight,reps
    Push,bench press,60,8

    ### Notes
    #### 22:04 - standup
    what was said

Writes take an exclusive lock on the journal and replace the file whole, and
every write says which revision it expected to change. A panel that read a day,
waited, and then ticked a habit is told the day moved rather than writing over
what moved it.
"""

from __future__ import annotations

import fcntl
import math
import os
import re
import tempfile
from contextlib import contextmanager
from dataclasses import dataclass, field
from datetime import date, datetime, timedelta
from pathlib import Path
from typing import Callable, Iterator

#: Where the journal is when nobody says otherwise.
DEFAULT_DEN = Path.home() / "personal" / "the-den"

#: Where the food database is when nobody says otherwise. It is macros.nvim's
#: own file, so a food added in the editor is a food the panel can log.
DEFAULT_DATABASE = Path.home() / ".local" / "share" / "nvim" / "macros.csv"

#: How far back an unfinished task is still worth naming. A task from last
#: spring is not restant, it is abandoned, and a panel that says so every
#: morning is a panel that gets ignored.
HORIZON = 60

#: The food database inside the den, which is macros.nvim's format.
FOODS = "Foods.csv"

#: The exercise database inside the den: `split,exercise` rows.
EXERCISES = "Exercises.csv"

#: The sections a day has, in the order they are written. A file missing one
#: reads as empty, and a write that needs one puts it back in this order.
SECTIONS = ("tasks", "habits", "macros", "weight", "workout", "notes")

#: The header line each table carries, which is not a row.
TABLES = {"macros": "what,protein,carbs,fat", "workout": "exercise,weight,reps"}

_H1 = re.compile(r"^#\s+(.+?)\s*$")
_H3 = re.compile(r"^###\s+(.+?)\s*$")
_H4 = re.compile(r"^####\s+(.+?)\s*$")
_CHECK = re.compile(r"^\s*-\s+\[([ xX])\]\s+(.+?)\s*$")
_BOX = re.compile(r"\[[ xX~]\]")
_KILOS = re.compile(r"^([0-9]+(?:\.[0-9]+)?)\s*kg$", re.IGNORECASE)
_STEM = re.compile(r"^(\d{4}-\d{2}-\d{2})-[A-Za-z]+$")
_SET = re.compile(r"([0-9]+(?:\.[0-9]+)?)\s*[xX]\s*([0-9]+)")


class Conflict(RuntimeError):
    """The entry moved after the caller read it."""


@dataclass(frozen=True)
class Habit:
    name: str
    done: bool

    def to_dict(self) -> dict[str, object]:
        return {"name": self.name, "done": self.done}


@dataclass(frozen=True)
class Task:
    index: int
    text: str
    done: bool

    def to_dict(self) -> dict[str, object]:
        return {"index": self.index, "text": self.text, "done": self.done}


@dataclass(frozen=True)
class Food:
    index: int
    name: str
    protein: float
    carbs: float
    fat: float

    def to_dict(self) -> dict[str, object]:
        return {
            "index": self.index,
            "name": self.name,
            "protein": self.protein,
            "carbs": self.carbs,
            "fat": self.fat,
        }


@dataclass(frozen=True)
class Macros:
    protein: float = 0.0
    carbs: float = 0.0
    fat: float = 0.0
    calories: int = 0

    def to_dict(self) -> dict[str, object]:
        return {
            "protein": self.protein,
            "carbs": self.carbs,
            "fat": self.fat,
            "calories": self.calories,
        }


@dataclass(frozen=True)
class Lift:
    """One set: the movement, the load, the reps.

    A set rather than an exercise, because a set is what is done and what is
    written down at the moment it is done. An exercise with three sets is three
    rows, and the panel groups them back together.

    No split here. A day is one split and every set of it belongs to that, so
    the split is written once at the top of the section - see `Day.split`.
    """

    index: int
    exercise: str
    weight: float
    reps: int

    @property
    def volume(self) -> float:
        return self.weight * self.reps

    def to_dict(self) -> dict[str, object]:
        return {
            "index": self.index,
            "exercise": self.exercise,
            "weight": self.weight,
            "reps": self.reps,
        }


@dataclass(frozen=True)
class Note:
    index: int
    heading: str
    body: str

    def to_dict(self) -> dict[str, object]:
        return {"index": self.index, "heading": self.heading, "body": self.body}


@dataclass(frozen=True)
class Idea:
    """One line of the backlog: something to do, with no day to do it on."""

    index: int
    text: str
    done: bool

    def to_dict(self) -> dict[str, object]:
        return {"index": self.index, "text": self.text, "done": self.done}


@dataclass(frozen=True)
class Dated:
    """One task read out of a day that is not the day being looked at."""

    date: str
    index: int
    text: str

    def to_dict(self) -> dict[str, object]:
        return {"date": self.date, "index": self.index, "text": self.text}


@dataclass(frozen=True)
class Session:
    """One day of training: the split it was, and the sets done on it."""

    date: str
    split: str
    lifts: list[Lift]

    @property
    def volume(self) -> float:
        return sum(lift.volume for lift in self.lifts)

    def to_dict(self) -> dict[str, object]:
        return {
            "date": self.date,
            "split": self.split,
            "lifts": [lift.to_dict() for lift in self.lifts],
        }


@dataclass
class Day:
    date: str
    file: str
    title: str
    habits: list[Habit] = field(default_factory=list)
    tasks: list[Task] = field(default_factory=list)
    foods: list[Food] = field(default_factory=list)
    lifts: list[Lift] = field(default_factory=list)
    notes: list[Note] = field(default_factory=list)
    macros: Macros = field(default_factory=Macros)
    weight: float | None = None
    split: str = ""

    def to_dict(self) -> dict[str, object]:
        return {
            "date": self.date,
            "file": self.file,
            "title": self.title,
            "habits": [habit.to_dict() for habit in self.habits],
            "tasks": [task.to_dict() for task in self.tasks],
            "foods": [food.to_dict() for food in self.foods],
            "lifts": [lift.to_dict() for lift in self.lifts],
            "notes": [note.to_dict() for note in self.notes],
            "macros": self.macros.to_dict(),
            "weight": self.weight,
            "split": self.split,
        }


def _sections(lines: list[str]) -> dict[str, list[str]]:
    """Splits a day into its `### ` sections, keyed by the header word."""
    found: dict[str, list[str]] = {}
    current: str | None = None
    for line in lines:
        header = _H3.match(line)
        if header:
            name = header.group(1).strip().lower()
            current = name if name in SECTIONS else None
            if current is not None:
                found.setdefault(current, [])
            continue
        if current is not None:
            found[current].append(line)
    return found


def _boxes(lines: list[str]) -> list[tuple[str, bool]]:
    return [
        (found.group(2).strip(), found.group(1) in "xX")
        for line in lines
        if (found := _CHECK.match(line))
    ]


def _cells(line: str, table: str) -> list[str] | None:
    """Splits one table row, or nothing if the line is not one.

    The header line is the table's own and is not a row. So is anything with
    the wrong number of cells: a table is hand-edited, and half a row is a
    mistake to pass over rather than to guess at.
    """
    row = line.strip()
    if not row or row.startswith(TABLES[table].split(",", 1)[0] + ","):
        return None
    cells = [cell.strip() for cell in row.split(",")]
    return cells if len(cells) == len(TABLES[table].split(",")) else None


def _number(cell: str) -> float | None:
    try:
        value = float(cell)
    except ValueError:
        return None
    return value if math.isfinite(value) else None


def _parse_foods(lines: list[str]) -> tuple[list[Food], Macros]:
    foods: list[Food] = []
    protein = carbs = fat = 0.0
    for line in lines:
        cells = _cells(line, "macros")
        if cells is None:
            continue
        values = [_number(cell) for cell in cells[1:]]
        if any(value is None for value in values):
            continue
        row = [value for value in values if value is not None]
        foods.append(Food(len(foods) + 1, cells[0], row[0], row[1], row[2]))
        protein, carbs, fat = protein + row[0], carbs + row[1], fat + row[2]
    total = protein * 4 + carbs * 4 + fat * 9
    calories = round(total) if math.isfinite(total) else 0
    return foods, Macros(protein, carbs, fat, calories)


def _parse_lifts(lines: list[str]) -> list[Lift]:
    lifts: list[Lift] = []
    for line in lines:
        cells = _cells(line, "workout")
        if cells is None:
            continue
        weight = _number(cells[1])
        reps = _number(cells[2])
        if weight is None or weight < 0 or reps is None or reps < 1:
            continue
        lifts.append(Lift(len(lifts) + 1, cells[0], weight, int(reps)))
    return lifts


def _heading(table: str) -> str:
    """The start of a table's header line, which is not a row of it."""
    return TABLES[table].split(",", 1)[0] + ","


def _parse_split(lines: list[str]) -> str:
    """The split the day was, which is the one line that is not the table.

    A day is one split - push, pull, legs - and every set of it belongs to
    that, so it is written once above the rows rather than on each of them.
    """
    head = _heading("workout")
    for line in lines:
        written = line.strip()
        if not written or written.startswith(head):
            continue
        if _cells(line, "workout") is not None:
            continue
        return written
    return ""


def _parse_weight(lines: list[str]) -> float | None:
    """The one weight the section holds, or nothing.

    Two lines is not two weighings, it is a file somebody was editing. Nothing
    is the honest answer, and the panel says so rather than picking one.
    """
    values = [line.strip() for line in lines if line.strip()]
    if len(values) != 1:
        return None
    found = _KILOS.fullmatch(values[0])
    return float(found.group(1)) if found else None


def _parse_notes(lines: list[str]) -> list[Note]:
    notes: list[Note] = []
    heading: str | None = None
    body: list[str] = []

    def keep() -> None:
        if heading is None:
            return
        while body and not body[0].strip():
            body.pop(0)
        while body and not body[-1].strip():
            body.pop()
        notes.append(Note(len(notes) + 1, heading, "\n".join(body)))

    for line in lines:
        found = _H4.match(line)
        if found:
            keep()
            heading = found.group(1).strip()
            body = []
        elif heading is not None:
            body.append(line)
    keep()
    return notes


def parse_day(path: Path) -> Day:
    """Reads one day's entry."""
    return parse_text(path.read_text(encoding="utf-8"), path.stem, str(path))


def parse_text(text: str, stem: str = "", file: str = "") -> Day:
    """Reads one day out of the text of an entry.

    Where `parse_day` reads a file, this reads what a write is about to make
    one: the same day, before it is on disk.
    """
    lines = text.splitlines()
    title = next(
        (found.group(1).strip() for line in lines if (found := _H1.match(line))),
        "",
    )
    sections = _sections(lines)
    foods, macros = _parse_foods(sections.get("macros", []))
    return Day(
        date=stem,
        file=file,
        title=title,
        tasks=[
            Task(number + 1, text, done)
            for number, (text, done) in enumerate(_boxes(sections.get("tasks", [])))
        ],
        habits=[Habit(name, done) for name, done in _boxes(sections.get("habits", []))],
        foods=foods,
        lifts=_parse_lifts(sections.get("workout", [])),
        split=_parse_split(sections.get("workout", [])),
        notes=_parse_notes(sections.get("notes", [])),
        macros=macros,
        weight=_parse_weight(sections.get("weight", [])),
    )


def parse_backlog(path: Path) -> list[Idea]:
    """Reads the backlog, which is a file that may not be there yet."""
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except FileNotFoundError:
        return []
    return [
        Idea(number + 1, text, done)
        for number, (text, done) in enumerate(_boxes(lines))
    ]


def _newline(lines: list[str]) -> str:
    for line in lines:
        if line.endswith("\r\n"):
            return "\r\n"
        if line.endswith("\n"):
            return "\n"
        if line.endswith("\r"):
            return "\r"
    return "\n"


def _header(lines: list[str], name: str) -> int | None:
    for index, line in enumerate(lines):
        found = _H3.match(line.strip())
        if found and found.group(1).strip().lower() == name:
            return index
    return None


def _region(lines: list[str], name: str) -> tuple[int, int]:
    """Where one section's body starts and ends."""
    header = _header(lines, name)
    if header is None:
        raise LookupError(f"no {name.title()} section")
    end = len(lines)
    for index in range(header + 1, len(lines)):
        if _H3.match(lines[index].strip()):
            end = index
            break
    return header + 1, end


def ensure_section(text: str, name: str) -> str:
    """Puts a missing section back, in the order the day is written in.

    Every entry written before a section existed is missing it, and there are
    years of them. A write into one of those is not an error - the section is
    added where it belongs, above the first section that comes after it, so a
    file that grows one is still a file in canonical order.
    """
    lines = text.splitlines(keepends=True)
    if _header(lines, name) is not None:
        return text
    newline = _newline(lines)
    later = SECTIONS[SECTIONS.index(name) + 1 :]
    position = next(
        (found for after in later if (found := _header(lines, after)) is not None),
        len(lines),
    )
    block = [f"### {name.title()}{newline}", newline]
    if name in TABLES:
        block += [TABLES[name] + newline, newline]
    if position > 0 and not lines[position - 1].endswith(("\n", "\r")):
        lines[position - 1] += newline
    lines[position:position] = block
    return "".join(lines)


def _last(lines: list[str], start: int, end: int) -> int:
    while end > start and not lines[end - 1].strip():
        end -= 1
    return end


def _append(lines: list[str], name: str, value: str) -> None:
    start, end = _region(lines, name)
    newline = _newline(lines)
    position = _last(lines, start, end)
    if position > start and not lines[position - 1].endswith(("\n", "\r")):
        lines[position - 1] += newline
    lines.insert(position, value + newline)


def _checked(lines: list[str], name: str) -> list[int]:
    start, end = _region(lines, name)
    return [index for index in range(start, end) if _CHECK.match(lines[index])]


def _flip(line: str, done: bool) -> str:
    return _BOX.sub("[ ]" if done else "[x]", line, count=1)


def add_task(text: str, item: str) -> str:
    lines = text.splitlines(keepends=True)
    _append(lines, "tasks", f"- [ ] {item}")
    return "".join(lines)


def toggle_task(text: str, index: int) -> str:
    lines = text.splitlines(keepends=True)
    found = _checked(lines, "tasks")
    if index < 1 or index > len(found):
        raise IndexError(f"task {index} not found")
    target = found[index - 1]
    box = _CHECK.match(lines[target])
    assert box is not None
    lines[target] = _flip(lines[target], box.group(1) in "xX")
    return "".join(lines)


def remove_task(text: str, index: int) -> str:
    lines = text.splitlines(keepends=True)
    found = _checked(lines, "tasks")
    if index < 1 or index > len(found):
        raise IndexError(f"task {index} not found")
    del lines[found[index - 1]]
    return "".join(lines)


def toggle_habit(text: str, name: str) -> str:
    """Ticks one habit, named with or without the icon it is written with."""
    lines = text.splitlines(keepends=True)
    wanted = name.strip().lower()
    for target in _checked(lines, "habits"):
        box = _CHECK.match(lines[target])
        assert box is not None
        written = box.group(2).strip()
        bare = written.split(maxsplit=1)[-1] if written else written
        if wanted not in {written.lower(), bare.lower()}:
            continue
        lines[target] = _flip(lines[target], box.group(1) in "xX")
        return "".join(lines)
    raise LookupError(f"habit not found: {name}")


def set_weight(text: str, value: str) -> str:
    lines = text.splitlines(keepends=True)
    start, end = _region(lines, "weight")
    newline = _newline(lines)
    written = f"{value} kg{newline}"
    filled = [index for index in range(start, end) if lines[index].strip()]
    if filled:
        lines[filled[0]] = written
        for index in reversed(filled[1:]):
            del lines[index]
    else:
        lines.insert(start, newline)
        lines.insert(start + 1, written)
    return "".join(lines)


def set_split(text: str, value: str) -> str:
    """Names the split the day is, on its own line over the table of sets."""
    lines = ensure_section(text, "workout").splitlines(keepends=True)
    start, end = _region(lines, "workout")
    newline = _newline(lines)
    head = _heading("workout")
    written = value + newline
    for index in range(start, end):
        bare = lines[index].strip()
        if not bare or bare.startswith(head) or _cells(lines[index], "workout"):
            continue
        lines[index] = written
        return "".join(lines)
    # None yet, so it goes above the table: the split is read before the sets.
    lines.insert(start, newline)
    lines.insert(start + 1, written)
    return "".join(lines)


def _rows(lines: list[str], table: str) -> list[int]:
    start, end = _region(lines, table)
    return [index for index in range(start, end) if _cells(lines[index], table)]


def add_row(text: str, table: str, row: str) -> str:
    lines = ensure_section(text, table).splitlines(keepends=True)
    _append(lines, table, row)
    return "".join(lines)


def remove_row(text: str, table: str, index: int) -> str:
    lines = text.splitlines(keepends=True)
    found = _rows(lines, table)
    if index < 1 or index > len(found):
        raise IndexError(f"row {index} not found in {table}")
    del lines[found[index - 1]]
    return "".join(lines)


def set_rows(text: str, table: str, name: str, rows: list[str]) -> str:
    """Writes over every row of one thing, where the first of them was.

    A movement is three lines of the file and one line of a training log, so it
    is edited whole: what is written now replaces what is there. No rows at all
    removes it. The place is kept, because the order of a workout is the order
    it was done in.
    """
    lines = text.splitlines(keepends=True)
    wanted = name.strip().lower()
    found: list[int] = []
    for index in _rows(lines, table):
        cells = _cells(lines[index], table)
        if cells is not None and cells[0].lower() == wanted:
            found.append(index)
    if not found:
        raise LookupError(f"no {table} rows named {name}")
    newline = _newline(lines)
    for index in reversed(found):
        del lines[index]
    for offset, row in enumerate(rows):
        lines.insert(found[0] + offset, row + newline)
    return "".join(lines)


def _blocks(lines: list[str]) -> list[tuple[int, int]]:
    start, end = _region(lines, "notes")
    headings = [index for index in range(start, end) if _H4.match(lines[index].strip())]
    return [
        (heading, headings[position + 1] if position + 1 < len(headings) else end)
        for position, heading in enumerate(headings)
    ]


def _block(heading: str, body: str, newline: str) -> list[str]:
    return [f"#### {heading}{newline}", newline] + [
        line + newline for line in body.splitlines()
    ]


def add_note(text: str, heading: str, body: str) -> str:
    lines = text.splitlines(keepends=True)
    newline = _newline(lines)
    start, end = _region(lines, "notes")
    position = _last(lines, start, end)
    block = _block(heading, body, newline)
    if position > start and lines[position - 1].strip():
        block.insert(0, newline)
    lines[position:position] = block
    return "".join(lines)


def edit_note(text: str, index: int, heading: str, body: str) -> str:
    lines = text.splitlines(keepends=True)
    found = _blocks(lines)
    if index < 1 or index > len(found):
        raise IndexError(f"note {index} not found")
    start, end = found[index - 1]
    newline = _newline(lines)
    block = _block(heading, body, newline)
    if end < len(lines) and block and block[-1].strip():
        block.append(newline)
    lines[start:end] = block
    return "".join(lines)


def remove_note(text: str, index: int) -> str:
    lines = text.splitlines(keepends=True)
    found = _blocks(lines)
    if index < 1 or index > len(found):
        raise IndexError(f"note {index} not found")
    start, end = found[index - 1]
    del lines[start:end]
    return "".join(lines)


def resolve_den(given: str | None = None) -> Path:
    """Where the journal is: what was asked for, then `DEN_PATH`, then home."""
    if given:
        return Path(given).expanduser()
    found = os.environ.get("DEN_PATH")
    return Path(found).expanduser() if found else DEFAULT_DEN


def resolve_date(value: str | None = None, offset: int | None = None) -> date:
    """One explicit date, or a number of days from the date it is now."""
    if value is not None:
        if offset is not None:
            raise ValueError("a date and an offset are two ways to say one day")
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", value):
            raise ValueError(f"invalid date: {value}")
        try:
            return date.fromisoformat(value)
        except ValueError:
            raise ValueError(f"invalid date: {value}") from None
    return date.today() + timedelta(days=offset or 0)


def stem_for(day: date) -> str:
    return day.strftime("%Y-%m-%d-%A")


def title_for(day: date) -> str:
    return day.strftime("%A, %B %d, %Y")


def entry_path(den: Path, day: date) -> Path:
    return den / "Daily" / f"{stem_for(day)}.md"


def backlog_path(den: Path) -> Path:
    return den / "Backlog.md"


def date_of(path: Path) -> date | None:
    """The day one entry is about, or nothing if the name is not one."""
    found = _STEM.fullmatch(path.stem)
    if found is None:
        return None
    try:
        day = date.fromisoformat(found.group(1))
    except ValueError:
        return None
    return day if path.stem == stem_for(day) else None


def _fsync_dir(directory: Path) -> None:
    try:
        handle = os.open(directory, os.O_RDONLY)
    except OSError:
        return
    try:
        os.fsync(handle)
    except OSError:
        pass
    finally:
        os.close(handle)


def atomic_write(path: Path, text: str) -> None:
    """Replaces one file whole, or leaves it as it was."""
    directory = path.parent
    handle, temporary = tempfile.mkstemp(
        dir=directory, prefix=f".{path.name}.", suffix=".tmp"
    )
    try:
        with os.fdopen(handle, "w", encoding="utf-8", newline="") as writing:
            writing.write(text)
            writing.flush()
            os.fsync(writing.fileno())
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise
    _fsync_dir(directory)


@contextmanager
def _lock(den: Path) -> Iterator[None]:
    """Holds the journal against other writers for the length of one change."""
    daily = den / "Daily"
    daily.mkdir(parents=True, exist_ok=True)
    with (daily / ".den.lock").open("a", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


def revision(path: Path) -> str:
    """What a caller compares to know whether the day moved under it."""
    try:
        found = path.stat()
    except FileNotFoundError:
        return ""
    return f"{found.st_ino}:{found.st_mtime_ns}:{found.st_size}"


def template(den: Path) -> str:
    found = den / "Templates" / "daily.md"
    if found.is_file():
        return found.read_text(encoding="utf-8")
    body = ""
    for name in SECTIONS:
        # The same shape `ensure_section` builds, table header and all, so a
        # den with no template of its own is not a den with a different format.
        body += f"### {name.title()}\n\n"
        if name in TABLES:
            body += TABLES[name] + "\n\n"
    return f"# {{{{title}}}}\n\n{body}"


def _create(path: Path, text: str) -> bool:
    """Links a finished file into place, and loses to whoever got there first."""
    handle, temporary = tempfile.mkstemp(
        dir=path.parent, prefix=f".{path.name}.", suffix=".new"
    )
    try:
        os.fchmod(handle, 0o644)
        with os.fdopen(handle, "w", encoding="utf-8", newline="") as writing:
            writing.write(text)
            writing.flush()
            os.fsync(writing.fileno())
        try:
            os.link(temporary, path)
        except FileExistsError:
            return False
        _fsync_dir(path.parent)
        return True
    finally:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass


def _ensure_locked(den: Path, day: date) -> tuple[Path, bool]:
    path = entry_path(den, day)
    if path.is_file():
        return path, False
    return path, _create(path, template(den).replace("{{title}}", title_for(day)))


def ensure_day(den: Path, day: date) -> Path:
    """Makes the day's entry if it is not there, and returns where it is."""
    with _lock(den):
        path, _made = _ensure_locked(den, day)
        return path


def read_day(den: Path, day: date, *, create: bool = True) -> tuple[Day, str]:
    """Reads one day, with the revision that reading was of.

    Read twice and compared, because a read that spanned a write is a day that
    was never on disk. Three tries, and then it is something worse than a race.
    """
    path = ensure_day(den, day) if create else entry_path(den, day)
    if not path.is_file():
        return Day(date=stem_for(day), file=str(path), title=title_for(day)), ""
    for _try in range(3):
        before = revision(path)
        found = parse_day(path)
        if before == revision(path):
            return found, before
    raise RuntimeError(f"entry kept changing while it was read: {path}")


def change(
    den: Path,
    day: date,
    expected: str,
    rewrite: Callable[[str], str],
) -> tuple[Day, str]:
    """Applies one change to one day, if the day is where the caller left it.

    An empty expected revision is a caller that read the day and found no file.
    That is honoured only when this call is what made it: if the entry appeared
    in between, somebody else wrote the day and the caller has not seen it.
    """
    with _lock(den):
        path, made = _ensure_locked(den, day)
        if not (made and not expected) and revision(path) != expected:
            raise Conflict(f"the entry changed: {day.isoformat()}")
        text = path.read_text(encoding="utf-8")
        written = rewrite(text)
        if written != text:
            atomic_write(path, written)
        return parse_day(path), revision(path)


def normalize_weight(value: str) -> str:
    """Reads a weight as the journal writes it: kilograms, one decimal place."""
    cleaned = re.sub(r"(?i)\s*kg\s*$", "", value.strip()).strip()
    written = cleaned if "." in cleaned else f"{cleaned}.0"
    if not re.fullmatch(r"[0-9]+(?:\.[0-9]+)?", written):
        raise ValueError(f"a weight is a number of kilograms: {value}")
    return written


def _plain(cell: str, what: str) -> str:
    cleaned = cell.strip()
    if not cleaned:
        raise ValueError(f"{what} must not be empty")
    if "," in cleaned or "\n" in cleaned or "\r" in cleaned:
        raise ValueError(f"{what} must be one line and hold no comma")
    return cleaned


def normalize_food(row: str) -> str:
    """Checks one `what,protein,carbs,fat` row before it is written."""
    cells = [cell.strip() for cell in row.split(",")]
    if len(cells) != 4:
        raise ValueError("a food row is what,protein,carbs,fat")
    name = _plain(cells[0], "a food name")
    if name == "what":
        raise ValueError("a food cannot be called what: that is the table header")
    for label, cell in zip(("protein", "carbs", "fat"), cells[1:], strict=True):
        if _number(cell) is None:
            raise ValueError(f"{label} is not a number: {cell}")
    return ",".join([name] + cells[1:])


def normalize_split(value: str) -> str:
    """Checks the name of a day's split before it is written.

    A comma is refused by `_plain`, which is what keeps a split off the table
    below it: a line with no comma is never read back as a set.
    """
    return _plain(value, "a split")


def normalize_lift(exercise: str, weight: str, reps: str) -> str:
    """Checks one `exercise,weight,reps` row before it is written.

    A weight of zero is a pull-up, not a mistake. Reps are whole: half a
    repetition is a thing people say and not a thing to record.
    """
    movement = _plain(exercise, "an exercise")
    if movement == "exercise":
        raise ValueError(
            "an exercise cannot be called exercise: that is the table header"
        )
    load = _number(str(weight).strip())
    if load is None or load < 0:
        raise ValueError(f"a weight is a number of kilograms: {weight}")
    count = _number(str(reps).strip())
    if count is None or count < 1 or count != int(count):
        raise ValueError(f"reps are a whole number above zero: {reps}")
    return f"{movement},{_figure(load)},{int(count)}"


def parse_sets(text: str) -> list[tuple[str, str]]:
    """Reads `60x8 60x8 60x7` as three sets, in the notation the panel prints.

    Written the way the sets are read back, so logging an exercise is one
    answer rather than one answer per set. The halves come back as they were
    typed and `normalize_lift` is what decides whether they are a set.
    """
    found: list[tuple[str, str]] = []
    for token in text.split():
        matched = _SET.fullmatch(token)
        if matched is None:
            raise ValueError(f"a set is weight x reps, like 60x8: {token}")
        found.append((matched.group(1), matched.group(2)))
    if not found:
        raise ValueError("name at least one set, like 60x8")
    return found


def _figure(value: float) -> str:
    """Writes a number without the noise: 60, not 60.0."""
    return str(int(value)) if value == int(value) else format(value, ".15g")


def add_food(den: Path, day: date, row: str, expected: str) -> tuple[Day, str]:
    written = normalize_food(row)
    return change(den, day, expected, lambda text: add_row(text, "macros", written))


def remove_food(den: Path, day: date, index: int, expected: str) -> tuple[Day, str]:
    return change(den, day, expected, lambda text: remove_row(text, "macros", index))


def add_lifts(
    den: Path,
    day: date,
    exercise: str,
    sets: list[tuple[str, str]],
    expected: str,
) -> tuple[Day, str]:
    """Writes every set of one movement in one edit.

    Checked before anything is written, so three sets with a bad one in the
    middle leave the day as it was rather than half logged.
    """
    rows = [normalize_lift(exercise, weight, reps) for weight, reps in sets]

    def rewrite(text: str) -> str:
        for row in rows:
            text = add_row(text, "workout", row)
        return text

    return change(den, day, expected, rewrite)


def remove_lift(den: Path, day: date, index: int, expected: str) -> tuple[Day, str]:
    return change(den, day, expected, lambda text: remove_row(text, "workout", index))


def edit_lifts(
    den: Path,
    day: date,
    was: str,
    exercise: str,
    sets: list[tuple[str, str]],
    expected: str,
) -> tuple[Day, str]:
    """Writes over every set of one movement, in one edit.

    No sets removes the movement. An exercise named is a movement renamed, and
    an empty one is the name it already had. Every row is checked before any of
    them is written, so a bad set leaves the day as it was.
    """
    named = exercise.strip() or was
    rows = [normalize_lift(named, weight, reps) for weight, reps in sets]
    return change(den, day, expected, lambda text: set_rows(text, "workout", was, rows))


def note_heading(title: str | None, now: datetime) -> str:
    """A note is stamped with the time it was written, and titled if it has one."""
    stamp = now.strftime("%H:%M")
    if title is None or not title.strip():
        return stamp
    cleaned = title.strip()
    if "\n" in cleaned or "\r" in cleaned:
        raise ValueError("a note title is one line")
    return f"{stamp} - {cleaned}"


BACKLOG_TITLE = "# Backlog"


def read_backlog(den: Path) -> tuple[list[Idea], str]:
    """Everything with no day to be done on, and the revision it was read at."""
    path = backlog_path(den)
    return parse_backlog(path), revision(path)


def change_backlog(
    den: Path, expected: str, rewrite: Callable[[str], str]
) -> tuple[list[Idea], str]:
    """Applies one change to the backlog, if it is where the caller left it.

    The backlog is one file rather than one a day, so it has no template and no
    creation to race over: a den that never had one starts with the heading and
    nothing under it.
    """
    path = backlog_path(den)
    with _lock(den):
        if revision(path) != expected:
            raise Conflict("the backlog changed")
        try:
            text = path.read_text(encoding="utf-8")
        except FileNotFoundError:
            text = f"{BACKLOG_TITLE}\n\n"
        written = rewrite(text)
        if written != text:
            atomic_write(path, written)
        return parse_backlog(path), revision(path)


def _backlog_lines(text: str) -> list[str]:
    return text.splitlines(keepends=True)


def _ideas(lines: list[str]) -> list[int]:
    return [index for index, line in enumerate(lines) if _CHECK.match(line)]


def add_idea(text: str, item: str) -> str:
    lines = _backlog_lines(text)
    newline = _newline(lines)
    position = _last(lines, 0, len(lines))
    if position > 0 and not lines[position - 1].endswith(("\n", "\r")):
        lines[position - 1] += newline
    lines.insert(position, f"- [ ] {item}{newline}")
    return "".join(lines)


def toggle_idea(text: str, index: int) -> str:
    lines = _backlog_lines(text)
    found = _ideas(lines)
    if index < 1 or index > len(found):
        raise IndexError(f"backlog item {index} not found")
    target = found[index - 1]
    box = _CHECK.match(lines[target])
    assert box is not None
    lines[target] = _flip(lines[target], box.group(1) in "xX")
    return "".join(lines)


def remove_idea(text: str, index: int) -> str:
    lines = _backlog_lines(text)
    found = _ideas(lines)
    if index < 1 or index > len(found):
        raise IndexError(f"backlog item {index} not found")
    del lines[found[index - 1]]
    return "".join(lines)


def promote(
    den: Path,
    index: int,
    day: date,
    backlog_revision: str,
    day_revision: str,
) -> tuple[Day, str, list[Idea], str]:
    """Moves one backlog item onto a day.

    Two files change, and there is no transaction across them. The task is
    written first, so the failure that can happen is an item on a day and still
    in the backlog - which somebody can see and fix - rather than an idea that
    was taken out of the backlog and landed nowhere.
    """
    ideas, current = read_backlog(den)
    if current != backlog_revision:
        raise Conflict("the backlog changed")
    if index < 1 or index > len(ideas):
        raise IndexError(f"backlog item {index} not found")
    wanted = ideas[index - 1].text
    written, after = change(den, day, day_revision, lambda text: add_task(text, wanted))
    remaining, left = change_backlog(
        den, backlog_revision, lambda text: remove_idea(text, index)
    )
    return written, after, remaining, left


def _dated(den: Path, day: date) -> list[Dated]:
    found, _current = read_day(den, day, create=False)
    return [
        Dated(day.isoformat(), task.index, task.text)
        for task in found.tasks
        if not task.done
    ]


def upcoming(den: Path, after: date) -> list[Dated]:
    """Unfinished tasks on days later than one, oldest first.

    Days ahead are few and are found by listing the directory. Days behind are
    many, which is why `restant` counts back instead.
    """
    daily = den / "Daily"
    if not daily.is_dir():
        return []
    days = sorted(
        day
        for path in daily.glob("*.md")
        if (day := date_of(path)) is not None and day > after
    )
    return [task for day in days for task in _dated(den, day)]


def restant(den: Path, before: date, horizon: int = HORIZON) -> list[Dated]:
    """Unfinished tasks left behind on days earlier than one, oldest first.

    Bounded on purpose. Everything before the horizon is not late, it is over,
    and a list that says otherwise every morning is a list nobody reads.
    """
    if horizon < 1:
        raise ValueError("a horizon is at least one day")
    days = [before - timedelta(days=step) for step in range(horizon, 0, -1)]
    return [
        task
        for day in days
        if entry_path(den, day).is_file()
        for task in _dated(den, day)
    ]


def weight_history(den: Path, end: date, days: int) -> list[tuple[str, float]]:
    """Every weighing in a window, oldest first. Days without one are absent."""
    if days < 1:
        raise ValueError("a window is at least one day")
    found: list[tuple[str, float]] = []
    for step in range(days - 1, -1, -1):
        day = end - timedelta(days=step)
        if not entry_path(den, day).is_file():
            continue
        written, _current = read_day(den, day, create=False)
        if written.weight is not None:
            found.append((day.isoformat(), written.weight))
    return found


def lift_history(den: Path, end: date, days: int) -> list[Session]:
    """Every day of training in a window, newest first.

    What the panel offers under a field starts here: the movements worth
    suggesting first are the ones already done, and the den is the only place
    that knows when they were done. The database beside it knows the rest.
    """
    if days < 1:
        raise ValueError("a window is at least one day")
    found: list[Session] = []
    for step in range(days):
        day = end - timedelta(days=step)
        if not entry_path(den, day).is_file():
            continue
        written, _current = read_day(den, day, create=False)
        if written.lifts:
            found.append(Session(day.isoformat(), written.split, written.lifts))
    return found


def splits_used(history: list[Session]) -> list[str]:
    """The splits trained in a window, most recent first."""
    seen: list[str] = []
    for session in history:
        if session.split and session.split not in seen:
            seen.append(session.split)
    return seen


def exercises_used(history: list[Session], split: str | None = None) -> list[str]:
    """The movements done in a window, most recent first.

    A split narrows it to the days that were that split, because the movements
    worth offering under today's split are the ones done on days like it.
    """
    wanted = split.strip().lower() if split and split.strip() else None
    seen: list[str] = []
    for session in history:
        if wanted and session.split.lower() != wanted:
            continue
        for lift in session.lifts:
            if lift.exercise and lift.exercise not in seen:
                seen.append(lift.exercise)
    return seen


def last_lift(history: list[Session], exercise: str) -> Lift | None:
    """The last set of one movement, which is what the next one is judged on."""
    wanted = exercise.strip().lower()
    for session in history:
        for lift in reversed(session.lifts):
            if lift.exercise.lower() == wanted:
                return lift
    return None


# The two databases the den keeps beside the days: the foods that can be
# logged and the movements that can be trained. Both are the person's own
# reference data rather than a day of it, both are plain CSV they can edit by
# hand, and both live in the den so they travel with the journal they describe.
#
# The food file keeps macros.nvim's own row format, so the editor and the panel
# read and write the same foods.

#: What a quantity can be called, and what it is called here.
UNITS = {
    "g": "g",
    "gr": "g",
    "gram": "g",
    "grams": "g",
    "p": "pc",
    "pc": "pc",
    "pcs": "pc",
    "piece": "pc",
    "pieces": "pc",
}

_MEASURED = re.compile(
    r"^(?P<name>.+?)\s+(?P<amount>(?:\d+(?:\.\d*)?|\.\d+))(?P<unit>[A-Za-z]+)$"
)


@dataclass(frozen=True)
class Item:
    """One database row: a food, the amount it is described by, and its macros."""

    name: str
    amount: float
    unit: str
    protein: float
    carbs: float
    fat: float

    @property
    def id(self) -> str:
        return f"{self.name.lower()}:{self.unit}"

    @property
    def label(self) -> str:
        """What one row reads as in a list under a field.

        The unit belongs on the label because it is the whole difference
        between two rows that are otherwise the same word: an egg by the piece
        and an egg by the gram are two foods, and the amount that follows means
        something different for each.
        """
        return f"{self.name} ({self.unit})"

    def choice(self) -> dict[str, str]:
        return {"id": self.id, "label": self.label}

    def to_dict(self) -> dict[str, object]:
        return {
            "food": self.name,
            "amount": self.amount,
            "unit": self.unit,
            "protein": self.protein,
            "carbs": self.carbs,
            "fat": self.fat,
        }

    def to_row(self) -> str:
        return (
            f"{self.name} {_figure(self.amount)}{self.unit},"
            f"{_figure(self.protein)},{_figure(self.carbs)},{_figure(self.fat)}"
        )


def resolve_database(given: str | None = None, den: Path | None = None) -> Path:
    """The food database: what was asked for, then the variable, then the den.

    Neovim's path is the last answer rather than the first, and only when the
    den holds no file of its own: a den that has one owns its foods, and a den
    that has not yet been given one keeps working off what macros.nvim wrote.
    """
    chosen = given or os.environ.get("MACROS_DATABASE")
    if chosen:
        return Path(chosen).expanduser()
    if den is not None:
        held = den / FOODS
        if held.is_file():
            return held
    return DEFAULT_DATABASE


def parse_item(row: str) -> Item:
    """Reads one macros.nvim row and puts its unit in canonical form."""
    if "\n" in row or "\r" in row:
        raise ValueError("a food row is one line")
    cells = row.split(",")
    if len(cells) != 4:
        raise ValueError("a food row is food,protein,carbs,fat")
    measured = _MEASURED.fullmatch(cells[0].strip())
    if measured is None:
        raise ValueError("a food ends with an amount and a unit")
    name = _plain(measured.group("name"), "a food name")
    unit = UNITS.get(measured.group("unit").lower())
    if unit is None:
        raise ValueError(f"unknown unit: {measured.group('unit')}")
    amount = _number(measured.group("amount"))
    if amount is None or amount <= 0:
        raise ValueError("a food amount is above zero")
    values: list[float] = []
    for label, cell in zip(("protein", "carbs", "fat"), cells[1:], strict=True):
        value = _number(cell.strip())
        if value is None or value < 0:
            raise ValueError(f"{label} is not a number at or above zero: {cell}")
        values.append(value)
    return Item(name, amount, unit, *values)


class Database:
    """Every food that could be read, by its identifier."""

    def __init__(self, items: dict[str, Item] | None = None) -> None:
        self.items = items or {}

    @classmethod
    def load(cls, path: Path) -> Database:
        """Reads what parses and passes over what does not.

        A row that does not parse is a row somebody typed by hand, and one bad
        line is no reason to refuse the whole database. As in macros.nvim, a
        later row with the same identifier replaces the earlier one.
        """
        try:
            rows = path.read_text(encoding="utf-8").splitlines()
        except FileNotFoundError:
            raise ValueError(f"no food database at {path}") from None
        found = cls()
        for row in rows:
            try:
                item = parse_item(row)
            except ValueError:
                continue
            found.items[item.id] = item
        return found

    def query(self, words: str) -> list[Item]:
        """Everything the letters typed so far appear in, in order.

        A subsequence rather than a prefix, so `chbr` finds chicken breast, and
        what starts with the letters is offered before what merely holds them.
        """
        wanted = words.strip().lower()
        if not wanted:
            raise ValueError("a query is not empty")
        found = [item for item in self.items.values() if _within(wanted, item.id)]
        found.sort(key=lambda item: (not item.id.startswith(wanted), item.id))
        return found

    def calculate(self, identifier: str, amount: float) -> Item:
        """Scales one row to an amount."""
        if not math.isfinite(amount) or amount <= 0:
            raise ValueError("an amount is a number above zero")
        item = self.items.get(identifier.lower())
        if item is None:
            raise ValueError(f"unknown food: {identifier}")
        ratio = amount / item.amount
        scaled = (item.protein * ratio, item.carbs * ratio, item.fat * ratio)
        if not all(math.isfinite(value) for value in scaled):
            raise ValueError("the scaled macros are not numbers")
        return Item(item.name, amount, item.unit, *scaled)


def _within(wanted: str, candidate: str) -> bool:
    at = 0
    for letter in wanted:
        at = candidate.find(letter, at)
        if at < 0:
            return False
        at += 1
    return True


def insert_item(path: Path, row: str) -> Item:
    """Adds one food to the database, under a lock, in canonical form."""
    item = parse_item(row)
    path.parent.mkdir(parents=True, exist_ok=True)
    with (path.parent / f".{path.name}.lock").open("a", encoding="utf-8") as guard:
        fcntl.flock(guard.fileno(), fcntl.LOCK_EX)
        try:
            with path.open("a+", encoding="utf-8") as handle:
                handle.seek(0, os.SEEK_END)
                if handle.tell():
                    handle.seek(handle.tell() - 1)
                    tail = handle.read(1)
                    handle.seek(0, os.SEEK_END)
                    if tail not in ("\n", "\r"):
                        handle.write("\n")
                handle.write(item.to_row() + "\n")
                handle.flush()
                os.fsync(handle.fileno())
        finally:
            fcntl.flock(guard.fileno(), fcntl.LOCK_UN)
    return item


# The exercise database. What it is for is the field under the panel's set box:
# a movement can be offered before it has ever been trained, and the split it
# usually belongs to is what puts today's movements at the top of that list.


@dataclass(frozen=True)
class Move:
    """One movement the database knows, and the split it belongs to."""

    split: str
    name: str

    def choice(self) -> dict[str, str]:
        return {"id": self.name, "label": f"{self.name} ({self.split})"}

    def to_dict(self) -> dict[str, object]:
        return {"split": self.split, "name": self.name}


class Exercises:
    """Every movement that could be read, in the order they were written."""

    def __init__(self, moves: list[Move] | None = None) -> None:
        self.moves = moves or []

    @classmethod
    def load(cls, path: Path) -> Exercises:
        """Reads what parses and passes over what does not.

        A den with no file yet is an empty database rather than a refusal: the
        panel falls back on what has been trained, which is where every one of
        these rows came from in the first place.
        """
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except (OSError, UnicodeDecodeError):
            return cls()
        moves: list[Move] = []
        seen: set[str] = set()
        for line in lines:
            cells = [cell.strip() for cell in line.split(",")]
            if len(cells) != 2 or not cells[0] or not cells[1]:
                continue
            if (cells[0].lower(), cells[1].lower()) == ("split", "exercise"):
                continue
            if cells[1].lower() in seen:
                continue
            seen.add(cells[1].lower())
            moves.append(Move(cells[0], cells[1]))
        return cls(moves)

    def names(self, split: str | None = None) -> list[str]:
        """The movements, the ones for a split first when one is named."""
        wanted = split.strip().lower() if split and split.strip() else None
        if wanted is None:
            return [move.name for move in self.moves]
        fits = [move for move in self.moves if move.split.lower() == wanted]
        rest = [move for move in self.moves if move.split.lower() != wanted]
        return [move.name for move in fits + rest]

    def splits(self) -> list[str]:
        """The splits the database names, in the order they were written."""
        found: list[str] = []
        for move in self.moves:
            if move.split not in found:
                found.append(move.split)
        return found

    def split_of(self, name: str) -> str | None:
        """Which split one movement belongs to, if the database knows it."""
        wanted = name.strip().lower()
        for move in self.moves:
            if move.name.lower() == wanted:
                return move.split
        return None

    def query(self, words: str) -> list[Move]:
        """The movements a few typed letters could mean, best first."""
        wanted = words.strip().lower()
        if not wanted:
            return list(self.moves)
        starts = [m for m in self.moves if m.name.lower().startswith(wanted)]
        holds = [m for m in self.moves if wanted in m.name.lower() and m not in starts]
        loose = [
            m
            for m in self.moves
            if _within(wanted, m.name.lower()) and m not in starts and m not in holds
        ]
        return starts + holds + loose


def resolve_exercises(given: str | None = None, den: Path | None = None) -> Path:
    """The exercise database: what was asked for, the variable, then the den."""
    chosen = given or os.environ.get("EXERCISES_DATABASE")
    if chosen:
        return Path(chosen).expanduser()
    return (den or resolve_den()) / EXERCISES


def normalize_move(split: str, name: str) -> str:
    """Checks one `split,exercise` row before it is written."""
    return f"{_plain(split, 'a split')},{_plain(name, 'an exercise')}"


def learn_move(path: Path, split: str, name: str) -> Move:
    """Adds one movement to the database, under a lock, keeping the header."""
    row = normalize_move(split, name)
    known = Exercises.load(path)
    if known.split_of(name) is not None:
        raise ValueError(f"already known: {name.strip()}")
    path.parent.mkdir(parents=True, exist_ok=True)
    with (path.parent / f".{path.name}.lock").open("a", encoding="utf-8") as guard:
        fcntl.flock(guard.fileno(), fcntl.LOCK_EX)
        try:
            held = path.read_text(encoding="utf-8") if path.is_file() else ""
            if not held.strip():
                held = "split,exercise\n"
            elif not held.endswith(("\n", "\r")):
                held += "\n"
            atomic_write(path, held + row + "\n")
        finally:
            fcntl.flock(guard.fileno(), fcntl.LOCK_UN)
    split_name, move_name = row.split(",", 1)
    return Move(split_name, move_name)


def forget_move(path: Path, name: str) -> Move:
    """Takes one movement out of the database. Trained days keep their sets."""
    wanted = name.strip().lower()
    if not wanted:
        raise ValueError("an exercise must not be empty")
    with (path.parent / f".{path.name}.lock").open("a", encoding="utf-8") as guard:
        fcntl.flock(guard.fileno(), fcntl.LOCK_EX)
        try:
            held = path.read_text(encoding="utf-8") if path.is_file() else ""
            kept: list[str] = []
            gone: Move | None = None
            for line in held.splitlines(keepends=True):
                cells = [cell.strip() for cell in line.split(",")]
                if len(cells) == 2 and cells[1].lower() == wanted and gone is None:
                    gone = Move(cells[0], cells[1])
                    continue
                kept.append(line)
            if gone is None:
                raise ValueError(f"unknown exercise: {name.strip()}")
            atomic_write(path, "".join(kept))
        finally:
            fcntl.flock(guard.fileno(), fcntl.LOCK_UN)
    return gone
