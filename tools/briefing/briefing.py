"""One morning briefing, assembled from whatever the projects declare.

A briefing is a run, not a message. Every run lives in one directory named for
its local date and holds everything the day was built from: the manifest, one
file for each source that answered, the prose Scufris wrote from them, and the
page rendered from the same run. Chat and the page are two readings of one
artifact, so neither can say something the other does not.

A source is a project that declares `[briefings.<profile>]` in its own
`.scufris.toml`. Nothing here knows what any of them report. The project owns
the guidance, the paths, and the meaning; this owns the deadline, the shape of
the answer, and the record.

A source answers with one JSON envelope carrying a Markdown body. Free Markdown
would read well and lay out badly: the page needs a title, a status, and a
handful of values it can put in a row without a model in the loop. A source
that answers with anything else is recorded as failed, with what it did say
kept, and is named in the briefing rather than quietly dropped.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from datetime import date as Date
from datetime import datetime
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

import page  # noqa: E402

DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
PROFILE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]*$")

#: What a source may say about itself. `failed` is not among them: only the
#: runner writes that, about a source that could not answer.
REPORTED = ("ok", "attention", "stale")
STATUSES = (*REPORTED, "failed")

#: A run's states. `collecting` survives a crash, so a directory left in it is
#: an incomplete run and not a delivered one.
RUN_STATES = ("collecting", "collected", "delivered", "failed")

MAX_FACTS = 6
MAX_TITLE = 80
MAX_HEADLINE = 200
MAX_LABEL = 40
MAX_VALUE = 80
MAX_BODY = 16 * 1024
MAX_OUTPUT = 512 * 1024
MAX_PROSE = 64 * 1024
KEEP_RUNS = 30

SOURCE_DEADLINE = 900.0
RUN_DEADLINE = 1800.0

# A source reads its project and reports. The edit tools are off because
# nothing here asks for a change, not because this is a sandbox: a source that
# runs a refresh command runs it with the owner's own hands, exactly as the
# review workspace does. The project's guidance is what keeps it honest.
PI_TOOLS = "read,grep,find,ls,bash"
CLAUDE_TOOLS = "Read,Glob,Grep,Bash"
CLAUDE_DENIED_TOOLS = "Edit,Write,NotebookEdit,Task"

JOBS_HELPER = Path(__file__).resolve().parents[1] / "jobs" / "scufris-jobs"


class Unusable(Exception):
    """A source answered with something that is not a contribution."""


class Refused(Exception):
    """The caller asked for something this cannot do."""


def state_root() -> Path:
    base = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))
    return base / "scufris" / "briefings"


def validated_date(value: Any) -> str:
    if not isinstance(value, str) or not DATE.fullmatch(value):
        raise Refused("a briefing date is YYYY-MM-DD")
    try:
        Date.fromisoformat(value)
    except ValueError:
        raise Refused(f"no such date: {value}") from None
    return value


def validated_profile(value: Any) -> str:
    if not isinstance(value, str) or not PROFILE.fullmatch(value):
        raise Refused("a briefing profile is a simple name")
    return value


def local_date() -> str:
    """Today where the machine is.

    The schedule is the owner's morning, so the date that names a run is the
    host's own. Nothing here converts between zones: a run belongs to the day
    the person woke up in.
    """
    return datetime.now().astimezone().date().isoformat()


def run_dir(date: str) -> Path:
    return state_root() / validated_date(date)


def slug(project: str) -> str:
    """A project ID as one path component."""
    return project.replace("/", "-")


def atomic_write(path: Path, data: str) -> None:
    handle, temporary = tempfile.mkstemp(dir=str(path.parent), prefix=".briefing-")
    try:
        with os.fdopen(handle, "w", encoding="utf-8") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    except BaseException:
        Path(temporary).unlink(missing_ok=True)
        raise


def read_manifest(date: str) -> dict[str, Any]:
    path = run_dir(date) / "manifest.json"
    try:
        found = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise Refused(f"no briefing run for {date}") from None
    except (OSError, json.JSONDecodeError) as trouble:
        raise Refused(f"the {date} run is unreadable: {trouble}") from None
    if not isinstance(found, dict) or found.get("version") != 1:
        raise Refused(f"the {date} run is not a briefing manifest")
    return found


def write_manifest(manifest: dict[str, Any]) -> None:
    directory = run_dir(manifest["date"])
    atomic_write(
        directory / "manifest.json", json.dumps(manifest, indent=2, sort_keys=True)
    )


def declared_sources(profile: str) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    """Ask the jobs helper which projects declare this profile.

    Project discovery and `.scufris.toml` belong to one reader. A second
    implementation of either would be a second answer to what a project is.
    """
    done = subprocess.run(
        [sys.executable, str(JOBS_HELPER), "briefings"],
        input=json.dumps({"profile": profile}),
        text=True,
        capture_output=True,
        check=False,
    )
    try:
        envelope = json.loads(done.stdout)
    except json.JSONDecodeError:
        raise Refused(
            done.stderr.strip() or "the project reader gave no answer"
        ) from None
    if not envelope.get("ok"):
        raise Refused(str(envelope.get("error", "the project reader refused")))
    result = envelope["result"]
    return result["sources"], result["diagnostics"]


def contribution_prompt(source: dict[str, Any], profile: str, date: str) -> str:
    """What one source is asked.

    The project's own guidance is the middle of this and the only part that
    says what to look at. Everything around it is the shape of the answer,
    which is this program's business because it is what the page reads.
    """
    facts = MAX_FACTS
    return f"""# Scufris {profile} briefing for {date}

You are one source in the {profile} briefing. Report on this project only, from
data you read in it during this run. This is a read: report what is there and
change nothing.

## Source

{source["project"]}, at {source["project_root"]}.

{source["description"]}

## Guidance

{source["guidance"]}

## Answer

Reply with exactly one fenced `json` block and nothing outside it:

```json
{{
  "title": "Short name for this source, as a person would say it",
  "status": "ok",
  "headline": "One sentence: the thing to know this morning",
  "facts": [{{ "label": "Short label", "value": "Short value" }}],
  "body": "Markdown. What you found, with the paths and numbers behind it."
}}
```

- `status` is `ok` when nothing needs the owner, `attention` when something
  does, or `stale` when the data you needed is missing or too old to trust.
- `facts` is at most {facts} entries, each a measured value with a short label.
  Leave it empty rather than filling it with prose.
- `body` is Markdown: headings, paragraphs, lists, links and code. Keep it to
  what a person reads over coffee.
- Every claim comes from data you read in this run. If something is missing,
  say it is missing and set `status` to `stale`. Never estimate a number you
  did not measure, and never carry a value over from another day.
"""


def harness_argv(source: dict[str, Any], prompt: str) -> list[str]:
    """The one-shot command for a source.

    Not a job. A job is a tmux pane bound to an owner session that can be
    steered and landed; a morning source answers once and is gone, so it keeps
    no session and leaves nothing to recover.
    """
    if source["harness"] == "pi":
        return [
            "pi",
            "--print",
            "--approve",
            "--no-extensions",
            "--no-session",
            "--model",
            source["model"],
            "--thinking",
            source["thinking"],
            "--tools",
            PI_TOOLS,
            prompt,
        ]
    return [
        "claude",
        "--print",
        "--model",
        source["model"],
        "--effort",
        source["thinking"],
        "--permission-mode",
        "dontAsk",
        "--tools",
        CLAUDE_TOOLS,
        "--disallowed-tools",
        CLAUDE_DENIED_TOOLS,
        "--disable-slash-commands",
        prompt,
    ]


def fenced(text: str) -> str | None:
    """The last fenced json block, if the answer has one."""
    blocks = re.findall(r"```(?:json)?\s*\n(.*?)```", text, re.DOTALL)
    return blocks[-1] if blocks else None


def short(value: Any, limit: int, what: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise Unusable(f"{what} must be text")
    text = " ".join(value.split())
    if len(text) > limit:
        raise Unusable(f"{what} is longer than {limit} characters")
    return text


def parse_contribution(text: str) -> dict[str, Any]:
    """One source's answer, or a refusal naming what was wrong with it."""
    block = fenced(text)
    try:
        found = json.loads(block if block is not None else text)
    except json.JSONDecodeError as trouble:
        raise Unusable(f"the answer is not one JSON envelope: {trouble}") from None
    if not isinstance(found, dict):
        raise Unusable("the answer is not one JSON envelope")
    unexpected = set(found) - {"title", "status", "headline", "facts", "body"}
    if unexpected:
        raise Unusable(f"unexpected keys: {', '.join(sorted(unexpected))}")
    status = found.get("status")
    if status not in REPORTED:
        raise Unusable(f"status must be one of {', '.join(REPORTED)}")
    raw_facts = found.get("facts", [])
    if not isinstance(raw_facts, list) or len(raw_facts) > MAX_FACTS:
        raise Unusable(f"facts must be a list of at most {MAX_FACTS} entries")
    facts = []
    for fact in raw_facts:
        if not isinstance(fact, dict) or set(fact) - {"label", "value"}:
            raise Unusable("a fact is one label and one value")
        facts.append(
            {
                "label": short(fact.get("label"), MAX_LABEL, "a fact label"),
                "value": short(fact.get("value"), MAX_VALUE, "a fact value"),
            }
        )
    body = found.get("body", "")
    if not isinstance(body, str):
        raise Unusable("body must be Markdown text")
    if len(body) > MAX_BODY:
        raise Unusable(f"body is longer than {MAX_BODY} characters")
    return {
        "title": short(found.get("title"), MAX_TITLE, "title"),
        "status": status,
        "headline": short(found.get("headline"), MAX_HEADLINE, "headline"),
        "facts": facts,
        "body": body.strip(),
    }


def ask(source: dict[str, Any], profile: str, date: str, deadline: float) -> dict[str, Any]:
    """Run one source and read what it answered.

    Every way this can go wrong ends in a contribution that says so. A morning
    with one silent project is still a morning; a morning that stops because
    one project could not answer is not.
    """
    prompt = contribution_prompt(source, profile, date)
    started = time.monotonic()
    if deadline <= 0:
        return failed_contribution(source, "the run was out of time before this source")
    try:
        done = subprocess.run(
            harness_argv(source, prompt),
            cwd=source["project_root"],
            text=True,
            capture_output=True,
            check=False,
            timeout=deadline,
        )
    except subprocess.TimeoutExpired:
        return failed_contribution(
            source,
            f"the source did not answer within {int(deadline)} seconds",
            seconds=time.monotonic() - started,
        )
    except OSError as trouble:
        return failed_contribution(source, f"the harness would not run: {trouble}")
    seconds = time.monotonic() - started
    answer = done.stdout[:MAX_OUTPUT]
    if done.returncode != 0:
        detail = " ".join(done.stderr.split())[:MAX_HEADLINE]
        return failed_contribution(
            source,
            f"the harness exited {done.returncode}: {detail}" if detail else f"the harness exited {done.returncode}",
            raw=answer,
            seconds=seconds,
        )
    try:
        contribution = parse_contribution(answer)
    except Unusable as trouble:
        return failed_contribution(source, str(trouble), raw=answer, seconds=seconds)
    return {**stamp(source), **contribution, "seconds": round(seconds, 1), "raw": None}


def stamp(source: dict[str, Any]) -> dict[str, Any]:
    """What the runner knows about a source, which the source never says."""
    return {
        "project": source["project"],
        "slug": slug(source["project"]),
        "harness": source["harness"],
        "model": source["model"],
    }


def failed_contribution(
    source: dict[str, Any],
    why: str,
    *,
    raw: str | None = None,
    seconds: float = 0.0,
) -> dict[str, Any]:
    return {
        **stamp(source),
        "title": source["project"],
        "status": "failed",
        "headline": why,
        "facts": [],
        "body": "",
        "seconds": round(seconds, 1),
        "raw": raw,
    }


def index_entry(contribution: dict[str, Any]) -> dict[str, Any]:
    """What the manifest keeps about a contribution.

    The manifest is an index, so it holds what a reader chooses by and leaves
    the body in the contribution file beside it.
    """
    return {
        key: contribution[key]
        for key in ("project", "slug", "title", "status", "headline", "facts", "harness", "model", "seconds")
    }


def collect(
    profile: str = "morning",
    date: str | None = None,
    *,
    source_deadline: float | None = None,
    run_deadline: float | None = None,
) -> dict[str, Any]:
    """Ask every declared source at once and write the run.

    Sources start together and are bounded separately. One project that hangs
    costs the run its own deadline and nothing else: the manifest publishes
    with what came back, and the source that did not answer is in it by name.
    """
    profile = validated_profile(profile)
    date = local_date() if date is None else validated_date(date)
    source_deadline = (
        environment_seconds("SCUFRIS_BRIEFING_SOURCE_DEADLINE", SOURCE_DEADLINE)
        if source_deadline is None
        else source_deadline
    )
    run_deadline = (
        environment_seconds("SCUFRIS_BRIEFING_DEADLINE", RUN_DEADLINE)
        if run_deadline is None
        else run_deadline
    )
    sources, diagnostics = declared_sources(profile)
    directory = run_dir(date)
    (directory / "contributions").mkdir(parents=True, exist_ok=True)
    directory.chmod(0o700)
    started = datetime.now().astimezone()
    manifest: dict[str, Any] = {
        "version": 1,
        "profile": profile,
        "date": date,
        "state": "collecting",
        "started": started.isoformat(timespec="seconds"),
        "finished": None,
        "sources": [],
        "diagnostics": diagnostics,
    }
    write_manifest(manifest)
    if not sources:
        return finish(manifest, [])
    clock = time.monotonic()

    def bounded(source: dict[str, Any]) -> dict[str, Any]:
        left = run_deadline - (time.monotonic() - clock)
        return ask(source, profile, date, min(source_deadline, left))

    with ThreadPoolExecutor(max_workers=len(sources)) as pool:
        contributions = list(pool.map(bounded, sources))
    return finish(manifest, contributions)


def finish(manifest: dict[str, Any], contributions: list[dict[str, Any]]) -> dict[str, Any]:
    directory = run_dir(manifest["date"])
    for contribution in contributions:
        atomic_write(
            directory / "contributions" / f"{contribution['slug']}.json",
            json.dumps(contribution, indent=2, sort_keys=True),
        )
    answered = [item for item in contributions if item["status"] != "failed"]
    manifest = {
        **manifest,
        "state": "collected" if answered or not contributions else "failed",
        "finished": datetime.now().astimezone().isoformat(timespec="seconds"),
        "sources": [index_entry(item) for item in contributions],
    }
    write_manifest(manifest)
    prune()
    return manifest


def environment_seconds(name: str, fallback: float) -> float:
    raw = os.environ.get(name)
    if raw is None:
        return fallback
    try:
        seconds = float(raw)
    except ValueError:
        return fallback
    return seconds if seconds > 0 else fallback


def prune(keep: int = KEEP_RUNS) -> None:
    """Keep the last runs and drop what is older.

    A briefing is read on the morning it is for, and once in a while a few days
    back. Nothing here is a record worth keeping a year of.
    """
    root = state_root()
    if not root.is_dir():
        return
    runs = sorted(
        (path for path in root.iterdir() if path.is_dir() and DATE.fullmatch(path.name)),
        reverse=True,
    )
    for old in runs[keep:]:
        for path in sorted(old.rglob("*"), reverse=True):
            if path.is_dir():
                path.rmdir()
            else:
                path.unlink()
        old.rmdir()


def run_state(date: str) -> str:
    """What the run directory for this date says, or `none` when there is not one.

    A caller deciding whether the morning still needs doing asks this. It
    answers `none` only when nothing was ever started, so a run left
    `collecting` by a crash is told apart from a morning nobody began.
    """
    if not (run_dir(date) / "manifest.json").is_file():
        return "none"
    state = read_manifest(date)["state"]
    return state if state in RUN_STATES else "failed"


def read_run(date: str) -> dict[str, Any]:
    """The whole run, for whoever writes the briefing from it."""
    manifest = read_manifest(date)
    directory = run_dir(date) / "contributions"
    contributions = []
    for entry in manifest["sources"]:
        path = directory / f"{entry['slug']}.json"
        try:
            contributions.append(json.loads(path.read_text(encoding="utf-8")))
        except (OSError, json.JSONDecodeError):
            contributions.append({**entry, "body": "", "raw": None})
    prose_path = run_dir(date) / "briefing.md"
    prose = prose_path.read_text(encoding="utf-8") if prose_path.is_file() else None
    return {"manifest": manifest, "contributions": contributions, "prose": prose}


def publish(date: str, prose: str) -> dict[str, Any]:
    """Keep the prose Scufris wrote and render the page from the same run."""
    manifest = read_manifest(date)
    if not isinstance(prose, str) or not prose.strip():
        raise Refused("a briefing needs its prose")
    if len(prose) > MAX_PROSE:
        raise Refused(f"the prose is longer than {MAX_PROSE} characters")
    directory = run_dir(date)
    atomic_write(directory / "briefing.md", prose.strip() + "\n")
    manifest = {**manifest, "state": "delivered"}
    write_manifest(manifest)
    run = read_run(date)
    atomic_write(directory / "briefing.html", page.render_page(run))
    return {
        "date": date,
        "state": manifest["state"],
        "markdown": str(directory / "briefing.md"),
        "page": str(directory / "briefing.html"),
    }


def render(date: str) -> str:
    """Write the page from a run that already exists."""
    run = read_run(date)
    path = run_dir(date) / "briefing.html"
    atomic_write(path, page.render_page(run))
    return str(path)


def delivered(date: str) -> bool:
    """Whether this date already has a briefing the owner has been given."""
    try:
        return read_manifest(date)["state"] == "delivered"
    except Refused:
        return False
