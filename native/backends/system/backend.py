"""Report what this machine is doing, one JSON line per interval.

The shared backend behind the machine-status widgets. It reads `/proc` and
nothing else: no package to install, no daemon to talk to, and nothing that
fails differently on somebody else's system.

The first line on standard input is the spawn payload the open carried. One
key is read from it, `every`, the interval in seconds. Everything after it is
an action, and this backend has none, so it is ignored.

Each line written is an object:

    {"cpu": 12.4, "memory": 41.0, "cores": [8.0, 15.2, ...],
     "temperature": 54.0, "load": 1.24, "uptime": 9312.7}

`cpu` and `memory` are percentages. `cores` is the same measure per core.
`temperature` is the processor package in degrees Celsius, or null on a machine
that does not report one. `load` is the one-minute load average. The first
reading is taken one interval after the start, because a CPU percentage is a
difference between two samples and there is nothing to subtract from yet.
"""

import glob
import json
import os
import sys
import time

#: The interval is clamped into this range. Below the floor the reading is
#: noise and the panel cannot keep up; above the ceiling nothing on screen
#: would look alive.
FLOOR = 0.25
CEILING = 60.0

#: The hwmon chips that report a processor temperature, best first. Anything
#: else on the bus is a disk, a battery, or a board sensor.
CHIPS = ("k10temp", "zenpower", "coretemp")

#: What the chips call the whole package rather than one core. A single core
#: running hot is not what the panel is asking about.
PACKAGE = ("tctl", "tdie", "package id 0")


def deaf() -> None:
    """Points standard output at nothing.

    Catching the broken pipe is not enough on its own: the interpreter flushes
    standard output again on the way out and raises there too, past any handler,
    and prints the complaint on standard error - which the companion is reading
    and logging. Redirecting the descriptor makes the last flush a no-op.
    """
    devnull = os.open(os.devnull, os.O_WRONLY)
    os.dup2(devnull, sys.stdout.fileno())


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


def load() -> float:
    """Returns the one-minute load average."""
    with open("/proc/loadavg", encoding="ascii") as averages:
        return round(float(averages.readline().split()[0]), 2)


def contents(path: str) -> str | None:
    """Returns what one sysfs file says, or nothing if it will not say."""
    try:
        with open(path, encoding="ascii") as sensor:
            return sensor.read().strip()
    except OSError:
        return None


def sensor() -> str | None:
    """Returns the file that reports the processor temperature, if any.

    Looked up once at the start rather than per reading: the chips are on the
    bus before this runs and stay there. A machine with none - a virtual one,
    or an architecture whose driver names differ - reports no temperature
    rather than a wrong one.
    """
    for chip in CHIPS:
        for hwmon in sorted(glob.glob("/sys/class/hwmon/hwmon*")):
            if contents(os.path.join(hwmon, "name")) != chip:
                continue
            inputs = sorted(glob.glob(os.path.join(hwmon, "temp*_input")))
            for path in inputs:
                label = contents(path.removesuffix("input") + "label") or ""
                if label.lower() in PACKAGE:
                    return path
            if inputs:
                return inputs[0]
    for zone in sorted(glob.glob("/sys/class/thermal/thermal_zone*")):
        if contents(os.path.join(zone, "type")) == "x86_pkg_temp":
            return os.path.join(zone, "temp")
    return None


def temperature(path: str | None) -> float | None:
    """Returns degrees Celsius from one sysfs sensor file."""
    if path is None:
        return None
    reading = contents(path)
    if reading is None:
        return None
    try:
        # Every sensor here reports thousandths of a degree.
        return round(int(reading) / 1000.0, 1)
    except ValueError:
        return None


def main() -> None:
    spawn = json.loads(sys.stdin.readline() or "null") or {}
    every = float(spawn.get("every", 1))
    every = min(max(every, FLOOR), CEILING)

    thermometer = sensor()
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
            "temperature": temperature(thermometer),
            "load": load(),
            "uptime": uptime(),
        }
        previous = current
        try:
            print(json.dumps(reading), flush=True)
        except BrokenPipeError:
            # The companion took the panel down. There is nobody to write to.
            deaf()
            return


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
