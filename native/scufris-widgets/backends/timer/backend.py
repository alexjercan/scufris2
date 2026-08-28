"""Count down, and report where the count is.

One process per timer. The widget declares `shared = false`, so two timers of
the same length are two timers rather than one counted twice.

The first line on standard input is the spawn payload: `{"seconds": 300}`, the
length. Every line after it is an action:

    {"action": "pause"}
    {"action": "resume"}
    {"action": "reset"}
    {"action": "add", "seconds": 60}

Each line written is an object:

    {"left": 284.2, "of": 300.0, "running": true, "done": false}

`left` is what remains, in seconds. It stops at zero and `done` stays true from
then on, because a timer that has finished has to keep saying so: the panel may
have been on another workspace at the moment it ran out.
"""

import json
import os
import sys
import threading
import time

#: How often the count is reported. Fine enough that the seconds do not stutter,
#: coarse enough that the coalescer is dropping nothing worth keeping.
BEAT = 0.2

#: The longest timer, one day. Past this a timer is a calendar entry.
CEILING = 86400.0


def deaf() -> None:
    """Points standard output at nothing.

    Catching the broken pipe is not enough on its own: the interpreter flushes
    standard output again on the way out and raises there too, past any handler,
    and prints the complaint on standard error - which the companion is reading
    and logging. Redirecting the descriptor makes the last flush a no-op.
    """
    devnull = os.open(os.devnull, os.O_WRONLY)
    os.dup2(devnull, sys.stdout.fileno())


class Countdown:
    """A length, a running flag, and what remains of the length."""

    def __init__(self, seconds: float) -> None:
        self.of = min(max(seconds, 0.0), CEILING)
        self.left = self.of
        self.running = True
        self.lock = threading.Lock()

    def spend(self, elapsed: float) -> dict[str, object]:
        """Takes elapsed time off the count and returns what to report."""
        with self.lock:
            if self.running and self.left > 0.0:
                self.left = max(self.left - elapsed, 0.0)
            if self.left <= 0.0:
                self.running = False
            return {
                "left": round(self.left, 1),
                "of": round(self.of, 1),
                "running": self.running,
                "done": self.left <= 0.0,
            }

    def act(self, action: dict[str, object]) -> None:
        """Carries out one action from the widget."""
        name = action.get("action")
        with self.lock:
            if name == "pause":
                self.running = False
            elif name == "resume":
                # Resuming a timer that has run out starts it over, because
                # there is nothing else resuming could mean.
                if self.left <= 0.0:
                    self.left = self.of
                self.running = True
            elif name == "reset":
                self.left = self.of
                self.running = False
            elif name == "add":
                seconds = action.get("seconds", 60)
                if isinstance(seconds, (int, float)):
                    self.left = min(self.left + float(seconds), CEILING)
                    self.of = max(self.of, self.left)


def listen(count: Countdown) -> None:
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
            count.act(action)


def main() -> None:
    spawn = json.loads(sys.stdin.readline() or "null") or {}
    seconds = spawn.get("seconds", 300)
    count = Countdown(float(seconds) if isinstance(seconds, (int, float)) else 300.0)

    # A daemon thread, so the process ends when the count does rather than
    # waiting on a read that only the companion can end.
    threading.Thread(target=listen, args=(count,), daemon=True).start()

    last = time.monotonic()
    while True:
        time.sleep(BEAT)
        now = time.monotonic()
        # Measured rather than assumed, for the reason the companion's own
        # clocks measure: a machine that was busy did not spend one beat.
        elapsed = now - last
        last = now
        try:
            print(json.dumps(count.spend(elapsed)), flush=True)
        except BrokenPipeError:
            # The companion took the panel down. There is nobody to count for.
            deaf()
            return


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
