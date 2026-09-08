"""One morning briefing, assembled from whatever the projects declare.

A briefing is a run, not a message. Every run lives in one directory named for
its local date and its profile, and holds everything the day was built from:
the manifest, one file for each source that answered, the prose Scufris wrote
from them, and the page rendered from the same run. Chat and the page are two
readings of one artifact, so neither can say something the other does not.

The date and the profile together name a run, so a morning and an evening on
one day are two runs and never one that overwrites the other.

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
from typing import Any, NamedTuple

sys.path.insert(0, str(Path(__file__).resolve().parent))

import page

DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
PROFILE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]*$")

#: The profile a caller that names none is asking about.
DEFAULT_PROFILE = "morning"

#: The custom message type a briefing wake carries, so the extension that hides
#: unprompted machinery from the transcript keeps matching it.
BRIEFING_WAKE = "scufris-briefing"

#: The control client that carries a wake to the foreground conversation.
CTL = "scufris-ctl"

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

#: A source that answered badly is asked once more, with its own answer and
#: the one reason it could not be used. The repair reads nothing and runs
#: nothing, so it is short; below the floor there is no time to try.
REPAIR_DEADLINE = 300.0
REPAIR_FLOOR = 45.0
REPAIR_THINKING = "low"
MAX_QUOTED = 32 * 1024

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


def run_dir(date: str, profile: str = DEFAULT_PROFILE) -> Path:
    """Where one run lives.

    The date names the day and the profile names the briefing, so two profiles
    on one date are two directories and neither can write over the other.
    """
    return state_root() / validated_date(date) / validated_profile(profile)


def profiles_for(date: str) -> list[str]:
    """Every profile that has a run for this date, by name.

    The order is the name's and not the clock's. Nothing chooses between two
    runs by which is newer: a caller that means one of them says which.
    """
    directory = state_root() / validated_date(date)
    if not directory.is_dir():
        return []
    found = []
    for path in sorted(directory.iterdir()):
        if not path.is_dir() or not PROFILE.fullmatch(path.name):
            continue
        try:
            read_manifest(date, path.name)
        except Refused:
            continue
        found.append(path.name)
    return found


def collected_runs(date: str) -> list[dict[str, Any]]:
    """Every run for this date that was gathered and never written up.

    A collection whose wake was refused leaves the run here, so a briefing
    gathered while the agent was down is still found later.
    """
    runs = []
    for profile in profiles_for(date):
        manifest = read_manifest(date, profile)
        if manifest["state"] != "collected":
            continue
        # The state is the record, and the prose beside it is the second
        # reading of the same thing: a run that has one was written up even if
        # the manifest was never brought up to date.
        if (run_dir(date, profile) / "briefing.md").is_file():
            continue
        runs.append(manifest)
    return runs


def pending(date: str) -> list[dict[str, Any]]:
    """Every gathered run for this date that still needs its prose said.

    A run no project contributed to is not one of them. A morning nothing
    declared is not an event, so nothing is woken for it.
    """
    return [manifest for manifest in collected_runs(date) if manifest["sources"]]


def ambiguous(date: str, names: list[str]) -> Refused:
    said = ", ".join(names)
    return Refused(f"name one of the briefings for {date} with --profile: {said}")


def resolve(date: str, profile: str | None = None, *, undelivered: bool = False) -> str:
    """Which run the caller means when it named a date and no profile.

    Naming no profile means the one obvious run: the briefing that was
    gathered and is still waiting for its prose. Two of those are two
    briefings, and this refuses rather than guesses between them. A wake that
    published into another profile's run would put one briefing's prose on
    another's page, and picking the newer of two would do exactly that on the
    day both were collected in the same minute.
    """
    if profile is not None:
        return validated_profile(profile)
    date = validated_date(date)
    waiting = [str(manifest["profile"]) for manifest in collected_runs(date)]
    if len(waiting) == 1:
        return waiting[0]
    if waiting:
        raise ambiguous(date, waiting)
    if undelivered:
        raise Refused(f"no gathered briefing for {date} is waiting to be written")
    # Nothing is waiting, so this is a reader looking at a day that is done.
    found = profiles_for(date)
    if len(found) == 1:
        return found[0]
    if found:
        raise ambiguous(date, found)
    return DEFAULT_PROFILE


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


def read_manifest(date: str, profile: str = DEFAULT_PROFILE) -> dict[str, Any]:
    named = f"{profile} run for {date}"
    path = run_dir(date, profile) / "manifest.json"
    try:
        found = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise Refused(f"no {named}") from None
    except (OSError, json.JSONDecodeError) as trouble:
        raise Refused(f"the {named} is unreadable: {trouble}") from None
    if not isinstance(found, dict) or found.get("version") != 1:
        raise Refused(f"the {named} is not a briefing manifest")
    return found


def write_manifest(manifest: dict[str, Any]) -> None:
    directory = run_dir(manifest["date"], manifest["profile"])
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
data you read in it during this run. Change nothing and run nothing that costs
anything, unless the guidance below names it, and then only what it names.

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
- `title` is at most {MAX_TITLE} characters, and `headline` is one plain
  sentence of at most {MAX_HEADLINE} characters. An answer over either limit
  is dropped whole, so put the detail in `body` instead.
- `facts` is at most {facts} entries, each a measured value with a label of at
  most {MAX_LABEL} characters and a value of at most {MAX_VALUE} characters.
  Leave it empty rather than filling it with prose.
- `body` is Markdown of at most {MAX_BODY} characters: headings, paragraphs,
  lists, links, and fenced code. Keep it to what a person reads over coffee.
- Every claim comes from data you read in this run. If something is missing,
  say it is missing and set `status` to `stale`. Never estimate a number you
  did not measure, and never carry a value over from another day.
"""


def repair_prompt(
    source: dict[str, Any], profile: str, trouble: str, answer: str
) -> str:
    """Ask a source to say again, correctly, what it already found.

    The work is done by the time this is asked. This run reads nothing, spends
    nothing, and gathers nothing: it is given the source's own words and the
    one reason they could not be used, and it may change only that.
    """
    quoted = answer[:MAX_QUOTED]
    cut = (
        "\n\n(The rest of your answer was too long to quote back.)"
        if len(answer) > MAX_QUOTED
        else ""
    )
    return f"""# Scufris {profile} briefing: your answer could not be read

You reported on {source["project"]} and the report was rejected:

    {trouble}

Send the same report again as exactly one fenced `json` block and nothing
else. Change only what the rejection names, and keep every finding, number and
path you already wrote.

Read nothing, run nothing, and look nothing up. Everything you need is below.
If a value is too long, shorten that value; do not go and measure it again.

```json
{{
  "title": "Short name for this source, as a person would say it",
  "status": "ok",
  "headline": "One sentence: the thing to know this morning",
  "facts": [{{ "label": "Short label", "value": "Short value" }}],
  "body": "Markdown. What you found, with the paths and numbers behind it."
}}
```

- `status` is one of {", ".join(REPORTED)}.
- `title` is at most {MAX_TITLE} characters, and `headline` is one plain
  sentence of at most {MAX_HEADLINE} characters.
- `facts` is at most {MAX_FACTS} entries, each with a label of at most
  {MAX_LABEL} characters and a value of at most {MAX_VALUE} characters.
- `body` is Markdown of at most {MAX_BODY} characters, carried as one JSON
  string. Escape every quotation mark and newline inside it. Fenced code
  inside the body is fine.

## What you answered

{quoted}{cut}
"""


def harness_argv(
    source: dict[str, Any], prompt: str, *, tools: bool = True
) -> list[str]:
    """The one-shot command for a source.

    Not a job. A job is a tmux pane bound to an owner session that can be
    steered and landed; a morning source answers once and is gone, so it keeps
    no session and leaves nothing to recover.

    Both harnesses run without asking. A source is answering a question this
    program put to it, in its own project, with nobody watching, so a prompt
    it cannot answer is the same as a refusal. `claude` is given
    `bypassPermissions` to match `pi --approve`: under `dontAsk` its shell
    runs sandboxed, and a source that needed `gh` or `python3` reported the
    denial instead of the data. What a source may reach is decided by the tool
    list and by its own guidance, not by a sandbox it cannot see.
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
            source["thinking"] if tools else REPAIR_THINKING,
            *(["--tools", PI_TOOLS] if tools else ["--no-tools"]),
            prompt,
        ]
    return [
        "claude",
        "--print",
        "--model",
        source["model"],
        "--effort",
        source["thinking"] if tools else REPAIR_THINKING,
        "--permission-mode",
        "bypassPermissions",
        "--tools",
        CLAUDE_TOOLS if tools else "",
        *(["--disallowed-tools", CLAUDE_DENIED_TOOLS] if tools else []),
        "--disable-slash-commands",
        prompt,
    ]


def envelope(text: str) -> Any:
    """The JSON value an answer carries, fenced or bare.

    A body is Markdown and may fence code of its own, so the closing fence is
    not the first ``` after the opening one and no pair of fences marks the
    block out. Each block is read for its own end instead: the decoder stops
    where the value stops, and a fence inside that value is inside a string.
    A block that starts inside one already read was quoted by it rather than
    answered with it. The last block left is the answer, so a source may still
    correct itself.
    """
    decoder = json.JSONDecoder()
    values: list[Any] = []
    first_trouble: Exception | None = None
    read_to = -1
    for opener in re.finditer(r"```[ \t]*(?:json)?[ \t]*\r?\n", text, re.IGNORECASE):
        start = text.find("{", opener.end())
        if start == -1 or start < read_to:
            continue
        try:
            value, read_to = decoder.raw_decode(text, start)
        except (ValueError, RecursionError) as trouble:
            first_trouble = first_trouble or trouble
            continue
        values.append(value)
    if values:
        return values[-1]
    try:
        return json.loads(text)
    except (ValueError, RecursionError) as trouble:
        raise Unusable(
            f"the answer is not one JSON envelope: {first_trouble or trouble}"
        ) from None


def short(value: Any, limit: int, what: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise Unusable(f"{what} must be text")
    text = " ".join(value.split())
    if len(text) > limit:
        raise Unusable(f"{what} is longer than {limit} characters")
    return text


def parse_contribution(text: str) -> dict[str, Any]:
    """One source's answer, or a refusal naming what was wrong with it."""
    found = envelope(text)
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


class Attempt(NamedTuple):
    """One run of one source: what it gave, or why that is not a contribution.

    `spoke` separates a source that answered badly from one that never
    answered. Only the first is worth asking again: it did the work and
    mis-said it, and everything needed to fix that is already in its words.
    """

    contribution: dict[str, Any] | None
    trouble: str
    raw: str | None
    spoke: bool
    seconds: float


def attempt(
    source: dict[str, Any], prompt: str, deadline: float, *, tools: bool = True
) -> Attempt:
    """Run the harness once and read what came back.

    Nothing here raises. Every way a source can fail is a sentence about why,
    because the caller is assembling a morning and not debugging a project.
    """
    started = time.monotonic()
    try:
        done = subprocess.run(
            harness_argv(source, prompt, tools=tools),
            cwd=source["project_root"],
            text=True,
            errors="replace",
            capture_output=True,
            check=False,
            timeout=deadline,
        )
    except subprocess.TimeoutExpired:
        return Attempt(
            None,
            f"the source did not answer within {int(deadline)} seconds",
            None,
            False,
            time.monotonic() - started,
        )
    except (OSError, ValueError) as trouble:
        return Attempt(
            None,
            f"the harness would not run: {trouble}",
            None,
            False,
            time.monotonic() - started,
        )
    seconds = time.monotonic() - started
    answer = (done.stdout or "")[:MAX_OUTPUT]
    if done.returncode != 0:
        detail = " ".join((done.stderr or "").split())[:MAX_HEADLINE]
        said = f": {detail}" if detail else ""
        return Attempt(
            None, f"the harness exited {done.returncode}{said}", answer, False, seconds
        )
    try:
        return Attempt(parse_contribution(answer), "", answer, True, seconds)
    except Unusable as trouble:
        return Attempt(None, str(trouble), answer, True, seconds)


def ask(
    source: dict[str, Any], profile: str, date: str, deadline: float
) -> dict[str, Any]:
    """Run one source and read what it answered.

    A source that answered badly is asked once more. It has already read its
    project and spent whatever its guidance allowed; the report exists and was
    mis-said. Handing it back its own words and the one reason they could not
    be used costs one short run with nothing to read and nothing to run, and
    it has saved a whole morning from a stray quotation mark.

    Every way this can go wrong ends in a contribution that says so. A morning
    with one silent project is still a morning; a morning that stops because
    one project could not answer is not.
    """
    if deadline <= 0:
        return failed_contribution(source, "the run was out of time before this source")
    first = attempt(source, contribution_prompt(source, profile, date), deadline)
    if first.contribution is not None:
        return contributed(source, first.contribution, first.seconds)
    left = deadline - first.seconds
    if not (first.spoke and first.raw and left >= REPAIR_FLOOR):
        return failed_contribution(
            source, first.trouble, raw=first.raw, seconds=first.seconds
        )
    second = attempt(
        source,
        repair_prompt(source, profile, first.trouble, first.raw),
        min(
            left,
            environment_seconds("SCUFRIS_BRIEFING_REPAIR_DEADLINE", REPAIR_DEADLINE),
        ),
        tools=False,
    )
    spent = first.seconds + second.seconds
    if second.contribution is not None:
        return contributed(source, second.contribution, spent)
    if second.spoke:
        return failed_contribution(
            source,
            f"{second.trouble}, and again when asked to correct it",
            raw=second.raw,
            seconds=spent,
        )
    # The repair run never happened. What is worth reporting is the answer the
    # source did give, not the second command that would not start.
    return failed_contribution(source, first.trouble, raw=first.raw, seconds=spent)


def contributed(
    source: dict[str, Any], contribution: dict[str, Any], seconds: float
) -> dict[str, Any]:
    return {**stamp(source), **contribution, "seconds": round(seconds, 1), "raw": None}


def stamp(source: dict[str, Any]) -> dict[str, Any]:
    """What the runner knows about a source, which the source never says.

    Read rather than indexed. This is also what a failure is recorded with, so
    a source that reached here malformed must still be nameable.
    """
    project = str(source.get("project") or "unknown")
    return {
        "project": project,
        "slug": slug(project),
        "harness": str(source.get("harness") or ""),
        "model": str(source.get("model") or ""),
    }


def failed_contribution(
    source: dict[str, Any],
    why: str,
    *,
    raw: str | None = None,
    seconds: float = 0.0,
) -> dict[str, Any]:
    marked = stamp(source)
    return {
        **marked,
        "title": marked["project"][:MAX_TITLE],
        "status": "failed",
        # Bounded like any other headline: this one is written from a harness
        # message, and the page lays out a sentence rather than a log line.
        "headline": " ".join(why.split())[:MAX_HEADLINE]
        or "the source could not answer",
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
        for key in (
            "project",
            "slug",
            "title",
            "status",
            "headline",
            "facts",
            "harness",
            "model",
            "seconds",
        )
    }


def collect(
    date: str | None = None,
    profile: str = DEFAULT_PROFILE,
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
    directory = run_dir(date, profile)
    (directory / "contributions").mkdir(parents=True, exist_ok=True)
    directory.parent.chmod(0o700)
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
        try:
            return ask(source, profile, date, min(source_deadline, left))
        except Exception as trouble:  # noqa: BLE001
            # The last line between one source and the whole morning. `ask`
            # answers rather than raises, so reaching here means a way to fail
            # nobody has thought of yet; the run still publishes with the rest
            # and this source is named.
            return failed_contribution(
                source, f"the runner could not ask this source: {trouble!r}"
            )

    with ThreadPoolExecutor(max_workers=len(sources)) as pool:
        contributions = list(pool.map(bounded, sources))
    return finish(manifest, contributions)


def finish(
    manifest: dict[str, Any], contributions: list[dict[str, Any]]
) -> dict[str, Any]:
    """Write what came back, and the page for it.

    The page is rendered here and not only at publish, so the day has one as
    soon as the sources have answered. Everything on it is measured by the
    sources; the prose Scufris writes is added on top later. A morning nobody
    wrote up is then still a morning the owner can read.
    """
    directory = run_dir(manifest["date"], manifest["profile"])
    kept = []
    for contribution in contributions:
        try:
            atomic_write(
                directory / "contributions" / f"{contribution['slug']}.json",
                json.dumps(contribution, indent=2, sort_keys=True),
            )
            kept.append(contribution)
        except (OSError, TypeError, ValueError, KeyError) as trouble:
            # One contribution that cannot be written down is one source lost,
            # not a morning lost. What it said is gone, so the manifest carries
            # the reason in its place.
            manifest = {
                **manifest,
                "diagnostics": [
                    *manifest["diagnostics"],
                    {
                        "project": str(contribution.get("project", "")),
                        "diagnostic": f"the contribution could not be kept: {trouble!r}",
                    },
                ],
            }
    contributions = kept
    answered = [item for item in contributions if item["status"] != "failed"]
    manifest = {
        **manifest,
        "state": "collected" if answered or not contributions else "failed",
        "finished": datetime.now().astimezone().isoformat(timespec="seconds"),
        "sources": [index_entry(item) for item in contributions],
    }
    write_manifest(manifest)
    # The record is written before the page and the pruning, and neither of
    # them may take it back. A morning that was collected stays collected even
    # if it cannot be laid out or the old runs cannot be swept.
    try:
        atomic_write(
            directory / "briefing.html",
            page.render_page(read_run(manifest["date"], manifest["profile"])),
        )
    except Exception as trouble:  # noqa: BLE001
        manifest = {
            **manifest,
            "diagnostics": [
                *manifest["diagnostics"],
                {
                    "project": "",
                    "diagnostic": f"the page could not be rendered: {trouble!r}",
                },
            ],
        }
        write_manifest(manifest)
    try:
        prune()
    except OSError:
        pass
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
        (
            path
            for path in root.iterdir()
            if path.is_dir() and DATE.fullmatch(path.name)
        ),
        reverse=True,
    )
    for old in runs[keep:]:
        for path in sorted(old.rglob("*"), reverse=True):
            if path.is_dir():
                path.rmdir()
            else:
                path.unlink()
        old.rmdir()


def run_state(date: str, profile: str = DEFAULT_PROFILE) -> str:
    """What the run directory for this date and profile says.

    It answers `none` only when nothing was ever started, so a run left
    `collecting` by a crash is told apart from a morning nobody began.
    """
    if not (run_dir(date, profile) / "manifest.json").is_file():
        return "none"
    state = read_manifest(date, profile)["state"]
    return state if state in RUN_STATES else "failed"


def read_run(date: str, profile: str = DEFAULT_PROFILE) -> dict[str, Any]:
    """The whole run, for whoever writes the briefing from it."""
    manifest = read_manifest(date, profile)
    directory = run_dir(date, profile) / "contributions"
    contributions = []
    for entry in manifest["sources"]:
        path = directory / f"{entry['slug']}.json"
        try:
            contributions.append(json.loads(path.read_text(encoding="utf-8")))
        except (OSError, json.JSONDecodeError):
            contributions.append({**entry, "body": "", "raw": None})
    prose_path = run_dir(date, profile) / "briefing.md"
    prose = prose_path.read_text(encoding="utf-8") if prose_path.is_file() else None
    return {"manifest": manifest, "contributions": contributions, "prose": prose}


def publish(date: str, profile: str, prose: str) -> dict[str, Any]:
    """Keep the prose Scufris wrote and render the page over again with it.

    Collection already wrote a page from the contributions alone. This adds the
    prose to the same run and renders it once more, so the page the owner opens
    is never behind what Scufris said.
    """
    manifest = read_manifest(date, profile)
    if not isinstance(prose, str) or not prose.strip():
        raise Refused("a briefing needs its prose")
    if len(prose) > MAX_PROSE:
        raise Refused(f"the prose is longer than {MAX_PROSE} characters")
    directory = run_dir(date, profile)
    atomic_write(directory / "briefing.md", prose.strip() + "\n")
    manifest = {**manifest, "state": "delivered"}
    write_manifest(manifest)
    run = read_run(date, profile)
    atomic_write(directory / "briefing.html", page.render_page(run))
    return {
        "date": date,
        "profile": profile,
        "state": manifest["state"],
        "markdown": str(directory / "briefing.md"),
        "page": str(directory / "briefing.html"),
    }


def render(date: str, profile: str = DEFAULT_PROFILE) -> str:
    """Write the page from a run that already exists."""
    run = read_run(date, profile)
    path = run_dir(date, profile) / "briefing.html"
    atomic_write(path, page.render_page(run))
    return str(path)


def delivered(date: str, profile: str = DEFAULT_PROFILE) -> bool:
    """Whether this run already has a briefing the owner has been given."""
    try:
        return read_manifest(date, profile)["state"] == "delivered"
    except Refused:
        return False


def wake_message(manifest: dict[str, Any]) -> str:
    """What the foreground is told when a run is gathered.

    One owner for these words. The timer reaches the conversation through
    `scufris-ctl` and a tool call reaches it in process, and a briefing that
    was asked for by hand must be asked for in the same words as one the clock
    asked for.
    """
    answered = len([item for item in manifest["sources"] if item["status"] != "failed"])
    failed = len(manifest["sources"]) - answered
    missing = f", {failed} could not answer" if failed else ""
    plural = "" if answered == 1 else "s"
    return (
        f"The {manifest['profile']} briefing for {manifest['date']} is collected: "
        f"{answered} source{plural} answered{missing}. Read it with "
        f"scufris_briefing_show for date {manifest['date']} and profile "
        f"{manifest['profile']}, then write the briefing yourself: one short "
        "piece in your own voice that says what today needs, built from what "
        "the sources actually reported. Do not read the sources out one after "
        "another, and claim nothing none of them measured. Name any source "
        "that could not answer. Call scufris_briefing_publish with that prose "
        "and the same date and profile, then tell the user the same briefing "
        "in the same words."
    )


def wake(date: str, profile: str, *, ctl: str | None = None) -> dict[str, Any]:
    """Carry one gathered run to the foreground conversation.

    The run on disk is the durable half and this is the delivery. A refused
    wake therefore leaves the run exactly as it was: the session-start read
    finds it later, and a morning gathered while the agent was down is still
    written up.
    """
    manifest = read_manifest(date, profile)
    answer = {"date": date, "profile": profile, "woken": False, "reason": ""}
    if manifest["state"] != "collected":
        return {**answer, "reason": f"the run is {manifest['state']}"}
    if not manifest["sources"]:
        return {**answer, "reason": "no project declared this briefing"}
    command = [
        ctl or os.environ.get("SCUFRIS_CTL") or CTL,
        "wake",
        wake_message(manifest),
        "--custom-type",
        BRIEFING_WAKE,
        "--details",
        json.dumps(
            {"date": date, "profile": profile, "sources": len(manifest["sources"])},
            sort_keys=True,
        ),
    ]
    try:
        done = subprocess.run(command, capture_output=True, text=True, check=False)
    except OSError as trouble:
        return {**answer, "reason": f"{command[0]} could not be run: {trouble}"}
    if done.returncode != 0:
        return {**answer, "reason": done.stderr.strip() or f"{command[0]} exited {done.returncode}"}
    return {**answer, "woken": True}
