"""Own a list of tasks, and report what is on it.

The one backend so far that writes as well as reads. The file is the truth: an
editor, a script, or the widget's own ticks all change the same lines, and the
backend reports what is there rather than a copy it keeps. So a task typed into
the file and a task ticked off in the panel travel the same loop.

The list is plain text, one task per line, `x ` in front of a finished one:

    Buy milk
    x Call the dentist
    Water the plants

Plain text because a person has to be able to write it. The path comes from the
spawn payload as `file`, and otherwise from `XDG_DATA_HOME` or `HOME`.

Each line written is an object:

    {"items": [{"at": 0, "text": "Buy milk", "done": false}], "path": "..."}

`at` is the line the task is on. Every action carries it together with the text
the panel is showing:

    {"action": "done", "at": 1, "text": "Call the dentist"}
    {"action": "drop", "at": 0, "text": "Buy milk"}

Both are needed because the file can change under the panel. A line that no
longer says what the person clicked on is left alone rather than acted on
blindly, which is the whole reason the text rides along.
"""

import json
import os
import sys
import threading
import time

#: How often the file is looked at. A task list is not a meter.
BEAT = 1.0

#: What marks a finished task, at the start of its line.
DONE = "x "


def default_path() -> str:
    """Returns where the list lives when the spawn payload does not say."""
    data = os.environ.get("XDG_DATA_HOME")
    if not data:
        data = os.path.join(os.environ.get("HOME", "."), ".local", "share")
    return os.path.join(data, "scufris", "tasks.txt")


def parse(lines: list[str]) -> list[dict[str, object]]:
    """Reads the file's lines as a list of tasks.

    A blank line is passed over but still counted, because `at` is the line the
    task is on and an action has to land on that line and no other.
    """
    items: list[dict[str, object]] = []
    for at, line in enumerate(lines):
        line = line.strip()
        if not line:
            continue
        done = line.startswith(DONE)
        items.append(
            {"at": at, "text": line[len(DONE) :].strip() if done else line, "done": done}
        )
    return items


def render(lines: list[str]) -> str:
    """Writes the file's text back, with the trailing newline a file wants."""
    return "".join(f"{line}\n" for line in lines)


class Tasks:
    """The file, and what was last read out of it."""

    def __init__(self, path: str) -> None:
        self.path = path
        self.lines: list[str] = []
        self.stamp: tuple[float, int] | None = None
        self.lock = threading.Lock()

    def look(self) -> None:
        """Re-reads the file, but only when it has changed since the last look."""
        try:
            status = os.stat(self.path)
            stamp = (status.st_mtime, status.st_size)
        except OSError:
            # No file yet is an empty list, not an error. It appears the moment
            # anything writes to it.
            with self.lock:
                self.lines = []
                self.stamp = None
            return
        if stamp == self.stamp:
            return
        try:
            with open(self.path, encoding="utf-8") as file:
                text = file.read()
        except OSError:
            return
        with self.lock:
            self.lines = text.splitlines()
            self.stamp = stamp

    def reading(self) -> dict[str, object]:
        """Returns what to report."""
        with self.lock:
            return {"items": parse(self.lines), "path": self.path}

    def act(self, action: dict[str, object]) -> None:
        """Carries out one action, if the line still says what was clicked on."""
        name = action.get("action")
        at = action.get("at")
        text = action.get("text")
        if name not in ("done", "drop") or not isinstance(at, int):
            return
        with self.lock:
            if not 0 <= at < len(self.lines):
                return
            line = self.lines[at].strip()
            was_done = line.startswith(DONE)
            said = line[len(DONE) :].strip() if was_done else line
            # The file moved under the panel. What the person clicked on is not
            # what is there, so nothing is done to it.
            if isinstance(text, str) and said != text:
                return
            if name == "drop":
                del self.lines[at]
            else:
                self.lines[at] = said if was_done else f"{DONE}{said}"
            self.save()

    def save(self) -> None:
        """Writes the list back, whole, and never half-written.

        Held under the same lock as the change that caused it. A temporary file
        beside the real one and a rename, so a reader never sees a half-written
        list and a crash mid-write leaves the previous one intact.
        """
        directory = os.path.dirname(self.path) or "."
        try:
            os.makedirs(directory, exist_ok=True)
            temporary = f"{self.path}.{os.getpid()}.tmp"
            with open(temporary, "w", encoding="utf-8") as file:
                file.write(render(self.lines))
            os.replace(temporary, self.path)
            status = os.stat(self.path)
            # Recorded here so the next look does not read back the write that
            # was just made and call it a change.
            self.stamp = (status.st_mtime, status.st_size)
        except OSError as error:
            print(f"the task list could not be written: {error}", file=sys.stderr)


def deaf() -> None:
    """Points standard output at nothing.

    Catching the broken pipe is not enough on its own: the interpreter flushes
    standard output again on the way out and raises there too, past any handler,
    and prints the complaint on standard error - which the companion is reading
    and logging. Redirecting the descriptor makes the last flush a no-op.
    """
    devnull = os.open(os.devnull, os.O_WRONLY)
    os.dup2(devnull, sys.stdout.fileno())


def listen(tasks: Tasks) -> None:
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
            tasks.act(action)


def main() -> None:
    spawn = json.loads(sys.stdin.readline() or "null") or {}
    path = spawn.get("file")
    tasks = Tasks(path if isinstance(path, str) and path else default_path())

    # A daemon thread, so the process ends when the reporting does rather than
    # waiting on a read that only the companion can end.
    threading.Thread(target=listen, args=(tasks,), daemon=True).start()

    while True:
        tasks.look()
        try:
            print(json.dumps(tasks.reading()), flush=True)
        except BrokenPipeError:
            # The companion took the panel down. There is nobody to report to.
            deaf()
            return
        time.sleep(BEAT)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
