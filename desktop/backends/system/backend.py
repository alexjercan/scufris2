"""Report what this machine is doing, one JSON line per interval.

The shared backend behind the machine-status widgets. It reads `/proc` and
nothing else: no package to install, no daemon to talk to, and nothing that
fails differently on somebody else's system.

The first line on standard input is the spawn payload the open carried. One
key is read from it, `every`, the interval in seconds. Everything after it is
an action, and this backend has none, so it is ignored.

Each line written is an object:

    {"cpu": 12.4, "memory": 41.0, "cores": [8.0, 15.2, ...], "uptime": 9312.7}

`cpu` and `memory` are percentages. `cores` is the same measure per core. The
first reading is taken one interval after the start, because a CPU percentage
is a difference between two samples and there is nothing to subtract from yet.
"""

import json
import sys
import time

#: The interval is clamped into this range. Below the floor the reading is
#: noise and the panel cannot keep up; above the ceiling nothing on screen
#: would look alive.
FLOOR = 0.25
CEILING = 60.0


def cpu_samples() -> list[tuple[int, int]]:
    """Returns (total, idle) jiffies for the machine and then for each core."""
    samples: list[tuple[int, int]] = []
    with open("/proc/stat", encoding="ascii") as stat:
        for line in stat:
            if not line.startswith("cpu"):
                break
            fields = [int(field) for field in line.split()[1:]]
            # user nice system idle iowait irq softirq steal ...
            idle = fields[3] + (fields[4] if len(fields) > 4 else 0)
            samples.append((sum(fields), idle))
    return samples


def busy(before: tuple[int, int], after: tuple[int, int]) -> float:
    """Returns the percentage of the interval between two samples spent busy."""
    total = after[0] - before[0]
    if total <= 0:
        return 0.0
    idle = after[1] - before[1]
    return round(100.0 * (total - idle) / total, 1)


def memory() -> float:
    """Returns the percentage of memory in use, the way `free` counts it."""
    total = 0
    available = 0
    with open("/proc/meminfo", encoding="ascii") as info:
        for line in info:
            name, _, rest = line.partition(":")
            if name == "MemTotal":
                total = int(rest.split()[0])
            elif name == "MemAvailable":
                available = int(rest.split()[0])
                break
    if total <= 0:
        return 0.0
    return round(100.0 * (total - available) / total, 1)


def uptime() -> float:
    """Returns how long the machine has been up, in seconds."""
    with open("/proc/uptime", encoding="ascii") as up:
        return round(float(up.readline().split()[0]), 1)


def main() -> None:
    spawn = json.loads(sys.stdin.readline() or "null") or {}
    every = float(spawn.get("every", 1))
    every = min(max(every, FLOOR), CEILING)

    previous = cpu_samples()
    while True:
        time.sleep(every)
        current = cpu_samples()
        if not current or not previous:
            continue
        reading = {
            "cpu": busy(previous[0], current[0]),
            "cores": [
                busy(before, after) for before, after in zip(previous[1:], current[1:])
            ],
            "memory": memory(),
            "uptime": uptime(),
        }
        previous = current
        try:
            print(json.dumps(reading), flush=True)
        except BrokenPipeError:
            # The companion took the panel down. There is nobody to write to.
            return


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
