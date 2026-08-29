"""Report how much of the Codex subscription is spent, one JSON line per poll.

The numbers behind the `codex` widget, and the same shape the `claude` backend
reports so the two panels read alike. It uses the token the Codex CLI already
keeps on this machine and asks for the usage of the account that token belongs
to. A machine where `codex` has never signed in has no token, and the widget
says so.

The credentials are read again on every poll rather than held, because the CLI
refreshes that file in place.

The first line on standard input is the spawn payload. One key is read from it,
`every`, the interval in seconds. Every line after it is an action:

    {"action": "refresh"}

which takes the next reading now instead of at the end of the interval.

Each line written is an object:

    {"plan": "pro", "error": null,
     "windows": [{"label": "weekly", "percent": 15.0, "resets": 438382.0}]}

`percent` is how much of that window is spent. `resets` is the seconds until it
starts over, or null for a window that does not say. `error` is a short phrase
when the reading could not be taken, and `windows` is then empty.

The two windows of the subscription itself are reported and nothing else. The
answer also carries who the account belongs to, including an email address;
none of it is read past the plan, and none of it reaches the panel or the log.
"""

import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request

#: Where the account's usage is reported.
USAGE = "https://chatgpt.com/backend-api/codex/usage"

#: The interval is clamped into this range, for the reason the Claude backend
#: clamps it: a subscription window is hours long.
FLOOR = 15.0
CEILING = 3600.0

#: How long one request has to answer.
PATIENCE = 10.0

#: Who this says it is. The service refuses the interpreter's own default agent
#: with a 403 whatever the token is, so this has to say something, and it says
#: what it is rather than dressing up as the CLI.
AGENT = "scufris-widget"

#: Window lengths that have a name worth using instead of a count of hours.
NAMES = {604800: "weekly", 86400: "daily", 3600: "hourly"}


def deaf() -> None:
    """Points standard output at nothing.

    Catching the broken pipe is not enough on its own: the interpreter flushes
    standard output again on the way out and raises there too, past any handler,
    and prints the complaint on standard error - which the companion is reading
    and logging. Redirecting the descriptor makes the last flush a no-op.
    """
    devnull = os.open(os.devnull, os.O_WRONLY)
    os.dup2(devnull, sys.stdout.fileno())


def home() -> str:
    """Returns the directory the CLI keeps its credentials in."""
    return os.environ.get("CODEX_HOME") or os.path.expanduser("~/.codex")


def credentials() -> dict[str, object] | None:
    """Returns the tokens the CLI signed in with, or nothing if it never did."""
    try:
        with open(os.path.join(home(), "auth.json"), encoding="utf-8") as file:
            held = json.load(file)
    except (OSError, json.JSONDecodeError):
        return None
    tokens = held.get("tokens") if isinstance(held, dict) else None
    return tokens if isinstance(tokens, dict) else None


def span(seconds: object) -> str:
    """Returns what to call a window of a given length.

    The service reports a length rather than a name. A week and a day have one
    worth using; anything else is said in hours, which is how the limit is
    described everywhere it is described at all.
    """
    if not isinstance(seconds, (int, float)) or isinstance(seconds, bool):
        return "limit"
    whole = int(seconds)
    if whole in NAMES:
        return NAMES[whole]
    hours = whole / 3600.0
    if hours >= 1:
        return f"{hours:g}h"
    return f"{max(whole // 60, 1)}m"


def resets(window: dict[str, object]) -> float | None:
    """Returns the seconds until one window starts over."""
    at = window.get("reset_at")
    if isinstance(at, (int, float)) and not isinstance(at, bool):
        # The instant, when it is given: it survives a reading that sat in a
        # queue, which a countdown measured on the server does not.
        return round(max(float(at) - time.time(), 0.0), 1)
    after = window.get("reset_after_seconds")
    if isinstance(after, (int, float)) and not isinstance(after, bool):
        return round(max(float(after), 0.0), 1)
    return None


def length(window: dict[str, object]) -> float:
    """Returns how long one window is, in seconds."""
    seconds = window.get("limit_window_seconds")
    if isinstance(seconds, (int, float)) and not isinstance(seconds, bool):
        return float(seconds)
    return 0.0


def windows(answer: object) -> list[dict[str, object]]:
    """Returns the usage windows of one answer, the shortest one first.

    Which of `primary` and `secondary` is the short window depends on the plan,
    so they are sorted by length rather than taken in the order given: the
    window that bites soonest is the one to read first.
    """
    limit = answer.get("rate_limit") if isinstance(answer, dict) else None
    if not isinstance(limit, dict):
        return []
    found: list[tuple[float, dict[str, object]]] = []
    for name in ("primary_window", "secondary_window"):
        window = limit.get(name)
        if not isinstance(window, dict):
            continue
        percent = window.get("used_percent")
        if not isinstance(percent, (int, float)) or isinstance(percent, bool):
            continue
        found.append(
            (
                length(window),
                {
                    "label": span(window.get("limit_window_seconds")),
                    "percent": round(float(percent), 1),
                    "resets": resets(window),
                },
            )
        )
    found.sort(key=lambda pair: pair[0])
    return [window for _, window in found]


def reading() -> dict[str, object]:
    """Takes one reading, as the line to write."""
    tokens = credentials()
    if tokens is None:
        return {"plan": None, "windows": [], "error": "not signed in"}
    token = tokens.get("access_token")
    account = tokens.get("account_id")
    if not isinstance(token, str) or not token:
        return {"plan": None, "windows": [], "error": "not signed in"}

    headers = {"Authorization": f"Bearer {token}", "User-Agent": AGENT}
    if isinstance(account, str) and account:
        headers["chatgpt-account-id"] = account
    request = urllib.request.Request(USAGE, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=PATIENCE) as answer:
            payload = json.load(answer)
    except urllib.error.HTTPError as refusal:
        trouble = "signed out" if refusal.code in (401, 403) else f"http {refusal.code}"
        return {"plan": None, "windows": [], "error": trouble}
    except (urllib.error.URLError, OSError, json.JSONDecodeError):
        return {"plan": None, "windows": [], "error": "no answer"}

    plan = payload.get("plan_type") if isinstance(payload, dict) else None
    plan = plan if isinstance(plan, str) and plan else None
    return {"plan": plan, "windows": windows(payload), "error": None}


def listen(wake: threading.Event) -> None:
    """Reads actions until standard input ends, waking the poll on each one."""
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            action = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(action, dict) and action.get("action") == "refresh":
            wake.set()


def main() -> None:
    spawn = json.loads(sys.stdin.readline() or "null") or {}
    every = spawn.get("every", 60)
    every = float(every) if isinstance(every, (int, float)) else 60.0
    every = min(max(every, FLOOR), CEILING)

    wake = threading.Event()
    # A daemon thread, so the process ends when the companion closes the pipe
    # rather than waiting on a read only the companion can end.
    threading.Thread(target=listen, args=(wake,), daemon=True).start()

    while True:
        try:
            print(json.dumps(reading()), flush=True)
        except BrokenPipeError:
            # The companion took the panel down. There is nobody to report to.
            deaf()
            return
        wake.wait(every)
        wake.clear()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
