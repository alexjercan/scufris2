"""Report how much of the Claude subscription is spent, one JSON line per poll.

The numbers behind the `claude` widget. It reads the OAuth token the Claude
Code CLI already keeps on this machine and asks Anthropic for the usage of the
account that token belongs to. Nothing is stored and nothing is signed in to:
a machine where `claude` has never run has no token, and the widget says so.

The credentials are read again on every poll rather than held. The CLI
refreshes that file in place, and a token this process cached at start would
stop working part way through the day.

The first line on standard input is the spawn payload. One key is read from it,
`every`, the interval in seconds. Every line after it is an action:

    {"action": "refresh"}

which takes the next reading now instead of at the end of the interval.

Each line written is an object:

    {"plan": "max", "error": null,
     "windows": [{"label": "session", "percent": 11.0, "resets": 8412.0}, ...]}

`percent` is how much of that window is spent. `resets` is the seconds until it
starts over, or null for a window that does not say. `error` is a short phrase
when the reading could not be taken, and `windows` is then empty; the process
stays up and tries again, because a poll that failed is usually a network that
came back.

Only the label, the percentage, and the reset reach the panel. The rest of the
answer - who the account belongs to and what it is called - is read past here.
"""

import json
import os
import sys
import threading
import urllib.error
import urllib.request
from datetime import datetime, timezone

#: Where the account's usage is reported.
USAGE = "https://api.anthropic.com/api/oauth/usage"

#: The header that says this token is an OAuth one rather than an API key.
BETA = "oauth-2025-04-20"

#: The interval is clamped into this range. A subscription window is hours
#: long, so polling faster than the floor only spends requests, and past the
#: ceiling the panel is a screenshot.
FLOOR = 15.0
CEILING = 3600.0

#: How long one request has to answer.
PATIENCE = 10.0


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
    return os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")


def credentials() -> dict[str, object] | None:
    """Returns what the CLI signed in as, or nothing if it never did."""
    try:
        with open(os.path.join(home(), ".credentials.json"), encoding="utf-8") as file:
            held = json.load(file)
    except (OSError, json.JSONDecodeError):
        return None
    oauth = held.get("claudeAiOauth") if isinstance(held, dict) else None
    return oauth if isinstance(oauth, dict) else None


def remaining(when: object) -> float | None:
    """Returns the seconds until an ISO 8601 instant, floored at zero."""
    if not isinstance(when, str):
        return None
    try:
        at = datetime.fromisoformat(when)
    except ValueError:
        return None
    if at.tzinfo is None:
        at = at.replace(tzinfo=timezone.utc)
    left = (at - datetime.now(timezone.utc)).total_seconds()
    return round(max(left, 0.0), 1)


def label(limit: dict[str, object]) -> str:
    """Returns what to call one limit on the panel.

    A scoped weekly limit is named after the model it scopes, because "weekly"
    twice on one panel says nothing about which of them is about to bite.
    """
    kind = limit.get("kind")
    if kind == "session":
        return "session"
    if kind == "weekly_all":
        return "weekly"
    scope = limit.get("scope")
    model = scope.get("model") if isinstance(scope, dict) else None
    name = model.get("display_name") if isinstance(model, dict) else None
    if isinstance(name, str) and name:
        return name.lower()
    return str(kind) if isinstance(kind, str) else "limit"


def windows(answer: object) -> list[dict[str, object]]:
    """Returns the usage windows of one answer, in the order it gave them.

    `limits` is read rather than the named fields beside it: the named ones
    come and go with whatever is being offered that month, and every one of
    them appears here as well.
    """
    limits = answer.get("limits") if isinstance(answer, dict) else None
    if not isinstance(limits, list):
        return []
    found: list[dict[str, object]] = []
    for limit in limits:
        if not isinstance(limit, dict):
            continue
        percent = limit.get("percent")
        if not isinstance(percent, (int, float)) or isinstance(percent, bool):
            continue
        found.append(
            {
                "label": label(limit),
                "percent": round(float(percent), 1),
                "resets": remaining(limit.get("resets_at")),
            }
        )
    return found


def reading() -> dict[str, object]:
    """Takes one reading, as the line to write."""
    oauth = credentials()
    if oauth is None:
        return {"plan": None, "windows": [], "error": "not signed in"}
    token = oauth.get("accessToken")
    if not isinstance(token, str) or not token:
        return {"plan": None, "windows": [], "error": "not signed in"}
    plan = oauth.get("subscriptionType")
    plan = plan if isinstance(plan, str) and plan else None

    request = urllib.request.Request(
        USAGE,
        headers={"Authorization": f"Bearer {token}", "anthropic-beta": BETA},
    )
    try:
        with urllib.request.urlopen(request, timeout=PATIENCE) as answer:
            payload = json.load(answer)
    except urllib.error.HTTPError as refusal:
        # A token the CLI has not refreshed yet, rather than a broken machine.
        trouble = "signed out" if refusal.code in (401, 403) else f"http {refusal.code}"
        return {"plan": plan, "windows": [], "error": trouble}
    except (urllib.error.URLError, OSError, json.JSONDecodeError):
        return {"plan": plan, "windows": [], "error": "no answer"}

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
        # The first reading goes out at once: unlike a rate, a percentage is
        # whole on its own and the panel would otherwise open empty.
        wake.wait(every)
        wake.clear()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
