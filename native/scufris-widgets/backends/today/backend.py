"""Report the day the panel is looking at, out of the-den.

Three panels read this backend and each opens it with a different `view`, so
each is a process of its own: a backend is keyed by the payload it was opened
with, and `agenda`, `macros` and `notes` are three different questions.

Nothing here parses the-den. `today` is the only program that understands that
journal's shape, and it is asked rather than imitated - so a change to the
format is a change in one place, and a half-written entry is `today`'s problem
to fail on rather than this backend's to misread.

The first line on standard input is the spawn payload:

    {"view": "agenda", "ahead": 5}
    {"view": "macros", "days": 30}
    {"view": "notes"}

Every line after it is an action:

    {"action": "select", "date": "2026-09-01"}
    {"action": "select", "date": null}
    {"action": "refresh"}
    {"action": "habit", "name": "Gym"}
    {"action": "task", "index": 2}

Each line written is an object carrying `view`, the `date` it is about, the
real `today`, whether the entry `exists`, and `trouble` - a sentence when
something went wrong, and null when nothing did. Trouble arrives beside the
day rather than instead of it, because a habit that would not toggle is no
reason to blank the panel that was reading fine a moment ago.
"""

import json
import os
import queue
import subprocess
import sys
import threading
import time
from datetime import date, timedelta

#: How often the selected day's file is looked at. An idle panel costs one
#: `stat` this often and nothing else.
BEAT = 5.0

#: How long a panel keeps a reading before building it again whatever the day's
#: file says. The day is watched by its own timestamp; what comes after it is
#: spread over as many files as there are days, and watching all of them to
#: save one subprocess a minute would be the wrong trade.
FLOOR = 60.0

#: How long one `today` run is given. Generous: it takes a lock on the journal
#: and may be waiting for another writer.
DEADLINE = 10.0

#: How many later tasks the agenda names under the day's own.
AHEAD = 5

#: How far back the weight trend reaches.
DAYS = 30

#: What a panel may ask to be. Anything else is an agenda, because a panel
#: that opened wrong is better than a panel that stayed empty.
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


class Trouble(Exception):
    """Something the panel should say in a sentence rather than swallow."""


def whole(value: object, fallback: int, low: int, high: int) -> int:
    """Reads one bounded count out of the spawn payload."""
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return fallback
    return min(max(int(value), low), high)


def sentence(text: str) -> str:
    """Takes the first line of a program's complaint, without its prefix."""
    line = text.strip().splitlines()[0] if text.strip() else ""
    return line.split(": ", 1)[1] if line.startswith("today: ") else line


class Journal:
    """The `today` command, the day the panel is on, and the last reading."""

    def __init__(self, spawn: dict[str, object]) -> None:
        self.command = os.environ.get("SCUFRIS_TODAY_COMMAND") or "today"
        self.den = os.environ.get("DEN_PATH")
        self.ahead = whole(spawn.get("ahead"), AHEAD, 1, 50)
        self.days = whole(spawn.get("days"), DAYS, 2, 365)
        #: The day asked for, or None while the panel is following the date.
        self.picked: str | None = None
        self.entry: tuple[str, str] | None = None
        self.stamp: float | None = None
        self.built = 0.0
        self.frame: dict[str, object] | None = None

    def now(self) -> str:
        """The real date, read again each time so a panel left up rolls over."""
        return date.today().isoformat()

    def run(self, arguments: list[str]) -> object:
        """Runs one `today` subcommand and returns what its JSON said."""
        line = [self.command]
        if self.den:
            line += ["--den", self.den]
        line += arguments
        try:
            done = subprocess.run(
                line, capture_output=True, text=True, timeout=DEADLINE
            )
        except FileNotFoundError:
            raise Trouble(f"{self.command} is not on the path") from None
        except OSError as error:
            raise Trouble(f"{self.command} would not start: {error}") from None
        except subprocess.TimeoutExpired:
            raise Trouble(f"{self.command} did not answer") from None
        if done.returncode != 0:
            raise Trouble(sentence(done.stderr) or f"{self.command} refused")
        if not arguments or arguments[-1] != "--json":
            return done.stdout.strip()
        try:
            return json.loads(done.stdout or "null")
        except json.JSONDecodeError:
            raise Trouble(f"{self.command} answered with something else") from None

    def on(self, selected: str, arguments: list[str]) -> object:
        """Runs one subcommand against one date."""
        return self.run(["--date", selected, *arguments])

    def file(self, selected: str) -> str:
        """The selected day's entry, asked for rather than assembled.

        Asking is also the only way to look at the day without making it:
        `show` creates the entry it reads, so a panel that browsed a month
        with it would leave a month of empty files behind.
        """
        if self.entry is None or self.entry[0] != selected:
            found = self.on(selected, ["path"])
            self.entry = (selected, str(found))
        return self.entry[1]

    def moved(self, selected: str) -> bool:
        """Whether the day's entry changed since it was last read."""
        try:
            stamp: float | None = os.stat(self.file(selected)).st_mtime
        except (OSError, Trouble):
            stamp = None
        if stamp == self.stamp:
            return False
        self.stamp = stamp
        return True

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
        self.forget()

    def forget(self) -> None:
        """Drops what was read, so the next beat reads again."""
        self.stamp = None
        self.frame = None

    def act(self, action: dict[str, object]) -> str | None:
        """Carries out one action and returns what went wrong, if anything.

        A change is made through `today` and then read back rather than
        applied here as well. The journal is the record; this only reports it.
        """
        name = action.get("action")
        if name == "select":
            self.select(action.get("date"))
            return None
        if name == "refresh":
            self.forget()
            return None
        selected = self.picked or self.now()
        try:
            if name == "habit":
                who = action.get("name")
                if not isinstance(who, str) or not who:
                    return None
                self.on(selected, ["habit", "toggle", who])
            elif name == "task":
                index = action.get("index")
                if isinstance(index, bool) or not isinstance(index, int):
                    return None
                self.on(selected, ["task", "done", str(index)])
            elif name == "add":
                text = action.get("text")
                if not isinstance(text, str) or not text.strip():
                    return None
                self.on(selected, ["task", "add", text.strip()])
            else:
                return None
        except Trouble as trouble:
            return str(trouble)
        self.forget()
        return None

    def read(self, view: str) -> dict[str, object]:
        """The reading for this beat, built again only when it has to be."""
        selected = self.picked or self.now()
        said = self.frame or {}
        fresh = (
            self.frame is None
            or said.get("date") != selected
            or said.get("today") != self.now()
            or time.monotonic() - self.built > FLOOR
        )
        if not self.moved(selected) and not fresh:
            return dict(said)
        self.built = time.monotonic()
        try:
            self.frame = self.build(view, selected)
        except Trouble as trouble:
            self.frame = self.bare(view, selected, str(trouble))
        return dict(self.frame)

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
        """Asks `today` for one day and shapes it for one panel."""
        frame = self.bare(view, selected, None)
        # Not caught here. A day with no entry is `exists: false` and a full
        # frame, but a command that would not run is trouble, and the panel
        # that would say the least about it - notes, which asks nothing else -
        # is the one that must not report an empty day instead.
        here = os.path.isfile(self.file(selected))
        frame["exists"] = here
        day = self.on(selected, ["show", "--json"]) if here else {}
        if not isinstance(day, dict):
            day = {}
        if view == "agenda":
            frame.update(self.agenda(selected, day))
        elif view == "macros":
            frame.update(self.macros(selected, day))
        else:
            frame["notes"] = day.get("notes") or []
        return frame

    def agenda(self, selected: str, day: dict[str, object]) -> dict[str, object]:
        """The day in full, then the tasks that come after it.

        `upcoming` is asked from before the earlier of the day and the date,
        so one call serves both the marks on the calendar and the list under
        it, and a day in the past still shows what stands between it and now.
        """
        edge = min(selected, self.now())
        before = (date.fromisoformat(edge) - timedelta(days=1)).isoformat()
        later = self.on(before, ["upcoming", "--json"])
        later = later if isinstance(later, list) else []
        ahead = [task for task in later if task.get("date", "") > selected]
        return {
            "habits": day.get("habits") or [],
            "tasks": day.get("tasks") or [],
            "ahead": ahead[: self.ahead],
            "marks": sorted({str(task.get("date")) for task in later}),
        }

    def macros(self, selected: str, day: dict[str, object]) -> dict[str, object]:
        """What the day was eaten and weighed, and where the weight is going."""
        trend = self.on(selected, ["weight", "--days", str(self.days), "--json"])
        trend = trend if isinstance(trend, dict) else {}
        return {
            "macros": day.get("macros") or {},
            # `today` gained `foods` in the day it reports after these panels
            # were written. An older one is not an error, only a shorter panel.
            "foods": day.get("foods") or [],
            "weight": day.get("weight"),
            "change": trend.get("change"),
            "recent": trend.get("recent") or [],
        }


class Mail:
    """Actions from the widget, and a nudge to stop waiting for them.

    Every `today` run happens on the one thread that reports, so nothing here
    holds a lock across a subprocess: a click hands over a line and wakes the
    reporter, which is what makes a tick land at once instead of on the beat.
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
    journal = Journal(spawn)
    mail = Mail()

    # A daemon thread, so a panel that is taken down ends here rather than
    # waiting on a read only the companion can end.
    threading.Thread(target=listen, args=(mail,), daemon=True).start()

    while True:
        refused = [journal.act(action) for action in mail.take()]
        reading = journal.read(view)
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
