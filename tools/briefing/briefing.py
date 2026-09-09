"""One morning briefing, assembled from whatever the projects declare.

A briefing is a run, not a message. Every run lives in one directory named for
its local date and its profile, and holds everything the day was built from:
the manifest, one file for each source that answered, the prose Scufris wrote
from them, and the page rendered from the same run. Chat and the page are two
readings of one artifact, so neither can say something the other does not.

The date and the profile together name a run, so a morning and an evening on
one day are two runs and never one that overwrites the other.

A source declares `[briefings.<profile>]`: a project does it in its own
`.scufris.toml`, and the machine does it for itself in one user-level file, for
anything that belongs to no checkout. Nothing here knows what any of them
report. The source owns the guidance, the paths, and the meaning; this owns the
deadline, the shape of the answer, and the record.

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

#: A contribution is kept in a file named for its source, so a slug the reader
#: hands over is held to one path component before it becomes one.
SLUG = re.compile(r"^[@A-Za-z0-9][@A-Za-z0-9_.-]*$")

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
MAX_OFFERS = 3
MAX_OFFER_LABEL = 60
MAX_OFFER_DETAIL = 400
MAX_OUTPUT = 512 * 1024
MAX_PROSE = 64 * 1024
KEEP_DAYS = 30

SOURCE_DEADLINE = 900.0
RUN_DEADLINE = 1800.0

#: A source that answered badly is asked once more, with its own answer and
#: the one reason it could not be used. The repair reads nothing and runs
#: nothing, so it is short; below the floor there is no time to try.
REPAIR_DEADLINE = 300.0
REPAIR_FLOOR = 45.0
REPAIR_THINKING = "low"
#: The floor for what a repair is shown of its own answer. The real budget
#: follows `max_body()`, because quoting less than a source is allowed to write
#: and then asking it to keep every finding is how a repair loses the last
#: third of a report and still records `ok`.
MAX_QUOTED = 32 * 1024

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
    """Every run for this date that still owes the conversation something.

    A collection whose wake was refused leaves the run here, so a briefing
    gathered while the agent was down is still found later.

    A run where every source failed is one of them. The absence of a briefing
    is itself the news, and leaving `failed` out made a total failure quieter
    than a partial one: the 23:00 nightly with nothing connected was refused
    its wake, excluded from this list, and never mentioned again. Both states
    close the same way, by Scufris publishing the prose - which for a failed
    run is the sentence saying there is none.
    """
    runs = []
    for profile in profiles_for(date):
        manifest = read_manifest(date, profile)
        if manifest["state"] not in ("collected", "failed"):
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


def previous_started(date: str, profile: str) -> str | None:
    """When this profile last began a run, over the runs that are kept.

    A source is told this so it can report on a night, a week or whatever its
    own schedule turned out to be, with no window setting anywhere. The runs
    kept on disk are the record, so a profile that has never run says so rather
    than leaving a model to invent a period.

    The start and not the finish, because the start is what the last briefing
    actually covered. Its sources were asked at about that moment and reported
    the world as they found it then, so a collection that took half an hour
    would leave everything inside that half hour after what was said and before
    what this run is told to look at. Measuring from the start overlaps
    instead, and a job mentioned in two briefings is better than one mentioned
    in neither. It also gives a run that crashed while collecting a usable
    moment, which is the one it asked its sources at.

    Read before the run being collected writes its own manifest: a date and a
    profile name one directory, so a second collection on one day would
    otherwise read back what it is about to replace.
    """
    root = state_root()
    if not root.is_dir():
        return None
    days = sorted(
        (
            path.name
            for path in root.iterdir()
            if path.is_dir() and DATE.fullmatch(path.name) and path.name <= date
        ),
        reverse=True,
    )
    for day in days:
        try:
            manifest = read_manifest(day, profile)
        except Refused:
            continue
        started = manifest.get("started")
        if isinstance(started, str) and started:
            return started
    return None


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


def declared_sources(
    profile: str, config: str | None = None
) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    """Ask the jobs helper which sources declare this profile.

    Project discovery, `.scufris.toml` and the user-level file belong to one
    reader. A second implementation of any of them would be a second answer to
    what a source is. `config` names another user-level file; without one the
    reader resolves `SCUFRIS_CONFIG` and then its own default path.
    """
    request: dict[str, Any] = {"profile": profile}
    if config is not None:
        request["config"] = config
    done = subprocess.run(
        [sys.executable, str(JOBS_HELPER), "briefings"],
        input=json.dumps(request),
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


def since_last_run(profile: str, since: str | None) -> str:
    """What the source is told about the last time this profile ran.

    A fact, not an instruction. The guidance below it is what says what to
    read, and a source whose project names its own window - yesterday, the last
    three sessions, the last twelve commits - keeps that window. This only
    answers "since when" for a source that asks the question, so a weekly
    source reports on a week and a morning source on a night with no window
    setting anywhere.
    """
    if since is None:
        return (
            f"No earlier {profile} briefing was kept on this machine, so there "
            "is no previous run to measure against. Report where things stand "
            "now, and do not invent a period you cannot measure."
        )
    return (
        f"The last {profile} briefing asked its sources at {since}, so anything "
        "after that moment is what it did not cover. Where the guidance below "
        "asks what changed and names no window of its own, that is the moment "
        "to measure from. Where it names its own window, keep it."
    )


def contribution_prompt(
    source: dict[str, Any], profile: str, date: str, since: str | None = None
) -> str:
    """What one source is asked.

    The source's own guidance is the middle of this and the only part that says
    what to look at. Everything around it is the shape of the answer, which is
    this program's business because it is what the page reads.
    """
    facts = MAX_FACTS
    offers = max_offers()
    body = max_body()
    return f"""# Scufris {profile} briefing for {date}

You are one source in the {profile} briefing. Report on this source only, from
data you read during this run.

You get exactly one turn. This process ends when you stop writing, and anything
you started and did not wait for dies with it: a delegated agent, a background
command, a review lane. The envelope below is the only answer anyone will ever
see from you, so never end a turn intending to continue. A partial report that
names what you did not reach is a good answer; an answer that says you will
finish later is nothing at all.

You have every tool this harness has, including the ones that write. Nothing is
withheld from you and nothing is watching. What you may do is what the guidance
below tells you to do, and nothing else: change no file, stage nothing, commit
nothing and run nothing that costs anything unless that guidance names it, and
then only what it names. Read the guidance as the whole of your permission.

## Source

{source["project"]}, at {source["root"]}.

{source["description"]}

## Since

{since_last_run(profile, since)}

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
  "body": "Markdown. What you found, with the paths and numbers behind it.",
  "offers": [{{ "label": "What could be done", "detail": "What and why" }}]
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
- `body` is Markdown of at most {body} characters: headings, paragraphs,
  emphasis, code, links, lists, block quotes, fenced code, horizontal rules,
  and GitHub-style tables. Keep it to what a person reads over coffee.
- `offers` is at most {offers} things the owner could do next about what you
  found, each a `label` of at most {MAX_OFFER_LABEL} characters saying what to
  do and a `detail` of at most {MAX_OFFER_DETAIL} characters saying what and
  why. Offer only what this run's data supports, and leave it empty when
  nothing needs doing. Do not write instructions for anyone to run: say the
  thing, and whoever picks it writes the words for it then.
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
    offers = max_offers()
    body = max_body()
    # An envelope is JSON around a body, so the answer is always larger than
    # the body bound. Twice it is the smallest budget that holds one.
    budget = max(MAX_QUOTED, body * 2)
    quoted = answer[:budget]
    cut = (
        "\n\n(The rest of your answer was too long to quote back. Keep only "
        "what you can see here; do not invent the rest.)"
        if len(answer) > budget
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
  "body": "Markdown. What you found, with the paths and numbers behind it.",
  "offers": [{{ "label": "What could be done", "detail": "What and why" }}]
}}
```

- `status` is one of {", ".join(REPORTED)}.
- `title` is at most {MAX_TITLE} characters, and `headline` is one plain
  sentence of at most {MAX_HEADLINE} characters.
- `facts` is at most {MAX_FACTS} entries, each with a label of at most
  {MAX_LABEL} characters and a value of at most {MAX_VALUE} characters.
- `body` is Markdown of at most {body} characters, carried as one JSON
  string. Escape every quotation mark and newline inside it. Fenced code
  inside the body is fine.
- `offers` is at most {offers} entries, each with a label of at most
  {MAX_OFFER_LABEL} characters and a detail of at most {MAX_OFFER_DETAIL}
  characters. Keep the ones you already wrote.

## What you answered

{quoted}{cut}
"""


def harness_argv(
    source: dict[str, Any], prompt: str, *, tools: bool = True
) -> list[str]:
    """The one-shot command for a source.

    Not a job. A job is a tmux pane bound to an owner session that can be
    steered and landed; a source answers once and is gone, so it keeps no
    session and leaves nothing to recover.

    A source runs with the owner's own hands. Every tool the harness has is
    present, `claude` is given `bypassPermissions` and `pi` is given
    `--approve`, and nothing here narrows that. A source is answering a
    question this program put to it, in its own project, with nobody watching,
    so a prompt it cannot answer is the same as a refusal: under a sandbox,
    sources that needed `gh` or `python3` reported the denial instead of the
    data.

    There was a tool allowlist here once, and it was never the thing it looked
    like. `bash` was always in it, so a source that meant to write could always
    write, and the list only decided how awkwardly. What a source may do is
    what its guidance tells it to do. That is stated in the prompt, where the
    source can read it, rather than implied by flags it cannot see.

    The second asking is the exception, and it is a real one: it is handed the
    source's own answer to say again correctly, so it is given no tools at all.
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
            *([] if tools else ["--no-tools"]),
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
        *([] if tools else ["--tools", "", "--disable-slash-commands"]),
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


def parse_offers(raw: Any) -> list[dict[str, str]]:
    """What this source says could be done next.

    A label and a detail, and nothing else. A stored worker prompt would only
    make sense for a delegated coding job, so it would quietly restrict offers
    to code sources; a source reporting on a calendar or a house has a next
    step too and no prompt to give. Whoever acts on one writes the words for it
    then, knowing it was picked.
    """
    allowed = max_offers()
    if not isinstance(raw, list) or len(raw) > allowed:
        raise Unusable(f"offers must be a list of at most {allowed} entries")
    offers = []
    for offer in raw:
        if not isinstance(offer, dict) or set(offer) - {"label", "detail"}:
            raise Unusable("an offer is one label and one detail")
        offers.append(
            {
                "label": short(offer.get("label"), MAX_OFFER_LABEL, "an offer label"),
                "detail": short(
                    offer.get("detail"), MAX_OFFER_DETAIL, "an offer detail"
                ),
            }
        )
    return offers


def numbered_offers(contributions: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Every source's offers as one list, numbered in source order.

    Code and not a model. Merging a list is concatenation, and a harness in
    this seat would only add a way to reword an entry or lose one. The number
    is assigned here and stored, so a pick made hours later resolves from the
    file rather than from whatever the model still remembers saying.
    """
    numbered = []
    for contribution in contributions:
        for offer in contribution.get("offers", []):
            numbered.append(
                {
                    "number": len(numbered) + 1,
                    "project": contribution["project"],
                    "slug": contribution["slug"],
                    **offer,
                }
            )
    return numbered


def parse_contribution(text: str) -> dict[str, Any]:
    """One source's answer, or a refusal naming what was wrong with it."""
    found = envelope(text)
    if not isinstance(found, dict):
        raise Unusable("the answer is not one JSON envelope")
    unexpected = set(found) - {
        "title",
        "status",
        "headline",
        "facts",
        "body",
        "offers",
    }
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
    allowed = max_body()
    if len(body) > allowed:
        raise Unusable(f"body is longer than {allowed} characters")
    return {
        "title": short(found.get("title"), MAX_TITLE, "title"),
        "status": status,
        "headline": short(found.get("headline"), MAX_HEADLINE, "headline"),
        "facts": facts,
        "body": body.strip(),
        "offers": parse_offers(found.get("offers", [])),
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
            cwd=source["root"],
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
    source: dict[str, Any],
    profile: str,
    date: str,
    deadline: float,
    since: str | None = None,
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
    first = attempt(source, contribution_prompt(source, profile, date, since), deadline)
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

    The slug is the reader's and is not derived again here. A section the
    machine declares called `scufris2` would otherwise be written into the
    `scufris2` project's contribution file; the reader namespaces one of them,
    so the collision is not expressible rather than merely noticed. Anything
    that is not one safe path component is refused a name of its own, because
    this is used as a file name.
    """
    named = str(source.get("slug") or "")
    return {
        "project": str(source.get("project") or "unknown"),
        "slug": named if SLUG.fullmatch(named) else "unknown",
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
        # A source that could not answer proposes nothing. Only what a source
        # measured can say what to do about it.
        "offers": [],
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
    config: str | None = None,
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
    sources, diagnostics = declared_sources(profile, config)
    # Resolved before the manifest is written, so the run records what it was
    # given rather than what it would have been given.
    workers = min(
        len(sources), environment_int("SCUFRIS_BRIEFING_PARALLEL", len(sources))
    )
    # Read before this run writes its own manifest over the last one: a date
    # and a profile name one directory. Every source is told the same moment,
    # so a run is one window and not one for each source.
    since = previous_started(date, profile)
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
        # What this run was actually given. The profile's numbers live in the
        # timer unit's environment, and a run started any other way - the
        # `scufris_briefing_run` tool, or a shell - gets the code defaults
        # instead, silently. A source cut off at 15 minutes when the profile
        # says 8 hours now says which number it was cut by.
        "bounds": {
            "source_deadline": source_deadline,
            "run_deadline": run_deadline,
            "parallel": workers,
        },
    }
    write_manifest(manifest)
    if not sources:
        return finish(manifest, [])
    clock = time.monotonic()

    def bounded(source: dict[str, Any]) -> dict[str, Any]:
        left = run_deadline - (time.monotonic() - clock)
        try:
            return ask(source, profile, date, min(source_deadline, left), since)
        except Exception as trouble:  # noqa: BLE001
            # The last line between one source and the whole morning. `ask`
            # answers rather than raises, so reaching here means a way to fail
            # nobody has thought of yet; the run still publishes with the rest
            # and this source is named.
            return failed_contribution(
                source, f"the runner could not ask this source: {trouble!r}"
            )

    # Every source at once unless a profile caps it. A morning of six cheap
    # reports wants no cap; a night of two agents that each fan out into review
    # lanes is already wide at the moment a group is under review, and a third
    # project declaring the profile should not quietly make it wider.
    #
    # The cap is on sources and not on what a source starts. What a source
    # spawns is its harness's business and this cannot see it.
    with ThreadPoolExecutor(max_workers=workers) as pool:
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
        # The numbered list is the manifest's own, not any source's. It is
        # written once, here, and every reader after this - the page, the wake,
        # a pick made hours later - reads these numbers rather than counting
        # again.
        "offers": numbered_offers(contributions),
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


#: Where a deployment writes each profile's bounds, below the config home.
#:
#: Home Manager owns a profile: its schedule, its deadlines, how wide it runs.
#: Those numbers reached the collection through the timer unit's environment,
#: which is to say they reached a run the timer started and nothing else. A
#: briefing asked for by hand - the `scufris_briefing_run` tool, or a shell -
#: got the code defaults instead, silently, so a night profile that allows eight
#: hours a source was cut at fifteen minutes and the work was lost.
#:
#: The file is the same numbers where every run can read them. It is generated,
#: never hand-written, and a deployment without one is the ordinary case for a
#: checkout.
PROFILE_BOUNDS_FILE = "briefing-profiles.json"

#: What a profile may set, and the variable each one is read through.
PROFILE_BOUNDS = {
    "deadline": "SCUFRIS_BRIEFING_DEADLINE",
    "source_deadline": "SCUFRIS_BRIEFING_SOURCE_DEADLINE",
    "parallel": "SCUFRIS_BRIEFING_PARALLEL",
    "max_offers": "SCUFRIS_BRIEFING_MAX_OFFERS",
    "max_body": "SCUFRIS_BRIEFING_MAX_BODY",
    "keep_days": "SCUFRIS_BRIEFING_KEEP_DAYS",
}


def config_home() -> Path:
    base = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
    return base / "scufris"


def apply_profile_bounds(profile: str) -> None:
    """Puts the deployment's numbers for one profile into this process.

    Into the environment rather than through every caller, because that is where
    the bounds are already read from and where a source's own subprocess already
    inherits them. The environment still wins where it is set, so the timer unit
    and a run asking for a number by hand both keep the number they asked for.

    Everything that can go wrong here reads as if the file said nothing. A
    briefing that refuses to run over a generated file is worse than one held to
    its defaults, and the manifest records the numbers it actually used.
    """
    try:
        raw = (config_home() / PROFILE_BOUNDS_FILE).read_bytes()
    except OSError:
        return
    try:
        declared = json.loads(raw)
    except (json.JSONDecodeError, UnicodeDecodeError):
        return
    if not isinstance(declared, dict):
        return
    bounds = declared.get(profile)
    if not isinstance(bounds, dict):
        return
    for field, variable in PROFILE_BOUNDS.items():
        value = bounds.get(field)
        # `parallel` is null for a profile that runs every source at once, which
        # is what saying nothing already means.
        if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
            continue
        os.environ.setdefault(variable, str(value))


def environment_seconds(name: str, fallback: float) -> float:
    raw = os.environ.get(name)
    if raw is None:
        return fallback
    try:
        seconds = float(raw)
    except ValueError:
        return fallback
    return seconds if seconds > 0 else fallback


def environment_int(name: str, fallback: int) -> int:
    """A whole number the environment may raise, or the built-in bound.

    Unset, unreadable and zero all mean the fallback. A profile that wants a
    longer night says so; anything a deployment gets wrong reads as if it had
    said nothing, because a briefing that refuses to run over a typo in a unit
    file is worse than one held to its defaults.
    """
    raw = os.environ.get(name)
    if raw is None:
        return fallback
    try:
        value = int(raw)
    except ValueError:
        return fallback
    return value if value > 0 else fallback


def max_offers() -> int:
    """How many offers one source may make.

    Read where it is used rather than captured at import, because the number is
    stated in the prompt the source is given and checked against the answer it
    sends back. Both have to be the same number, and a profile sets it.
    """
    return environment_int("SCUFRIS_BRIEFING_MAX_OFFERS", MAX_OFFERS)


def max_body() -> int:
    """How long a contribution body may be, for the same reason."""
    return environment_int("SCUFRIS_BRIEFING_MAX_BODY", MAX_BODY)


def prune(keep: int | None = None) -> None:
    """Keep the last days and drop what is older.

    A briefing is read on the morning it is for, and once in a while a few days
    back. Nothing here is a record worth keeping a year of.

    Days, not runs: a date directory holds every profile that ran that day, so
    a machine with a morning and a nightly keeps the same span of history as
    one with a morning alone. What the number has to outlast is
    `previous_started`, which reads back through the kept days to say when this
    profile last ran.
    """
    if keep is None:
        keep = environment_int("SCUFRIS_BRIEFING_KEEP_DAYS", KEEP_DAYS)
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
        "in the same words." + offers_instruction(manifest)
    )


def offers_instruction(manifest: dict[str, Any]) -> str:
    """What the wake says about the numbered list, when there is one.

    The numbers are already assigned and already on disk. This says to use
    them rather than to make them, because a briefing said in one set of
    numbers and stored under another would make a pick mean two things.
    """
    offers = manifest.get("offers", [])
    if not offers:
        return ""
    counted = len(offers)
    return (
        f" This run has {counted} thing{'' if counted == 1 else 's'} that could "
        "be done next, already numbered in the manifest. End the briefing with "
        "them as a numbered list, using those numbers exactly and adding none "
        "of your own. Alex picks by number."
    )


def failure_message(manifest: dict[str, Any]) -> str:
    """What the foreground is told when a run gathered nothing.

    A run where every source failed used to reach nobody: `wake` refused any
    state but `collected`, `pending` left it out, and the unit exited 0. The
    briefing simply did not arrive, and noticing that was Alex's job. One
    source failing out of five was reported; five out of five was silence.
    """
    named = ", ".join(item["project"] for item in manifest["sources"])
    return (
        f"The {manifest['profile']} briefing for {manifest['date']} did not "
        f"collect: every source failed ({named}). Tell the user plainly that "
        "there is no briefing this time and name the sources that could not "
        "answer. Claim nothing about what they would have said. Call "
        "scufris_briefing_publish with that same sentence and the same date "
        f"and profile - a run nobody publishes is a run this asks about again "
        "every session - then tell the user, and do not collect it again "
        "unless he asks."
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
    # A failed run is delivered too. It is the one state where the absence of
    # a briefing is itself the news, and refusing to carry it made a total
    # failure quieter than a partial one.
    if manifest["state"] not in ("collected", "failed"):
        return {**answer, "reason": f"the run is {manifest['state']}"}
    if not manifest["sources"]:
        return {**answer, "reason": "no project declared this briefing"}
    command = [
        ctl or os.environ.get("SCUFRIS_CTL") or CTL,
        "wake",
        failure_message(manifest)
        if manifest["state"] == "failed"
        else wake_message(manifest),
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
        return {
            **answer,
            "reason": done.stderr.strip() or f"{command[0]} exited {done.returncode}",
        }
    return {**answer, "woken": True}
