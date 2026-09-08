#!/usr/bin/env python3
"""The briefing on the command line.

One run of this asks every project that declares a briefing, keeps what they
said, and leaves a page beside it. The agent drives it through tools; a systemd
timer drives it on a schedule; a person drives it here, which is also how it is
tested.

    scufris-briefing sources
    scufris-briefing collect --profile morning
    scufris-briefing wake --profile morning
    scufris-briefing pending --json
    scufris-briefing show --json
    scufris-briefing publish < prose.md
    scufris-briefing open

Every subcommand works on one run, which a local date and a profile name
together. `--date` names another day; without it, today where the machine is.
`--profile` names another briefing; without it, the one run for the day that is
waiting to be written up. Two of those are two briefings and are never guessed
between, so a wake can neither publish into another profile's run nor into one
that was already delivered.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import briefing


class Stop(Exception):
    """Something the caller should read on standard error and act on."""


def shared() -> argparse.ArgumentParser:
    """The flags every level accepts, wherever the caller puts them."""
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument(
        "--date",
        default=argparse.SUPPRESS,
        help="the local date of the run, as YYYY-MM-DD",
    )
    common.add_argument(
        "--profile",
        default=argparse.SUPPRESS,
        help="which briefing; the default is the one waiting to be written up",
    )
    common.add_argument(
        "--ctl",
        default=argparse.SUPPRESS,
        help="the control client a wake is carried by; the default is scufris-ctl",
    )
    common.add_argument(
        "--json", action="store_true", default=argparse.SUPPRESS, help="answer as JSON"
    )
    return common


def parser() -> argparse.ArgumentParser:
    common = shared()
    top = argparse.ArgumentParser(
        prog="scufris-briefing",
        description=__doc__,
        parents=[common],
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    commands = top.add_subparsers(dest="command", required=True)
    commands.add_parser(
        "sources", parents=[common], help="the projects that declare this briefing"
    )
    commands.add_parser(
        "collect", parents=[common], help="ask every source and keep what it said"
    )
    commands.add_parser(
        "wake",
        parents=[common],
        help="carry a gathered run to the foreground conversation",
    )
    commands.add_parser(
        "pending",
        parents=[common],
        help="the runs for a date that are gathered and not written up",
    )
    commands.add_parser(
        "show", parents=[common], help="the run, with every contribution"
    )
    commands.add_parser("state", parents=[common], help="what the run for a date says")
    commands.add_parser(
        "publish",
        parents=[common],
        help="keep the prose on standard input and render the page",
    )
    commands.add_parser("render", parents=[common], help="write the page from the run")
    commands.add_parser("open", parents=[common], help="open the page")
    commands.add_parser("path", parents=[common], help="where the run is")
    return top


def wanted_date(options: argparse.Namespace) -> str:
    given = getattr(options, "date", None)
    return briefing.local_date() if given is None else briefing.validated_date(given)


def named_profile(options: argparse.Namespace) -> str | None:
    """The profile the caller named, or nothing when it named none."""
    given = getattr(options, "profile", None)
    return None if given is None else briefing.validated_profile(given)


def wanted_profile(options: argparse.Namespace) -> str:
    """The profile a collection asks the projects for."""
    return named_profile(options) or briefing.DEFAULT_PROFILE


def wanted_run(
    options: argparse.Namespace, *, undelivered: bool = False
) -> tuple[str, str]:
    """The one run a subcommand works on.

    A caller that named no profile is resolved against what is on disk rather
    than against a default name, so the ordinary case - one briefing gathered
    and waiting for its prose - needs nothing said about it.
    """
    date = wanted_date(options)
    return date, briefing.resolve(date, named_profile(options), undelivered=undelivered)


def say(options: argparse.Namespace, value: object, lines: list[str]) -> None:
    if getattr(options, "json", False):
        print(json.dumps(value, indent=2, sort_keys=True))
        return
    for line in lines:
        print(line)


def source_lines(sources: list[dict], diagnostics: list[dict]) -> list[str]:
    lines = [
        f"{item['project']}  {item['harness']}  {item['description']}"
        for item in sources
    ]
    lines.extend(f"{item['project']}: {item['diagnostic']}" for item in diagnostics)
    return lines or ["no project declares this briefing"]


def wake_line(result: dict) -> str:
    if result["woken"]:
        return f"woke the conversation for the {result['profile']} briefing"
    return f"nothing was woken: {result['reason']}"


def run_lines(run: dict) -> list[str]:
    manifest = run["manifest"]
    lines = [f"{manifest['date']} {manifest['profile']} {manifest['state']}"]
    for item in manifest["sources"]:
        lines.append(f"  [{item['status']}] {item['project']}: {item['headline']}")
    for item in manifest.get("diagnostics", []):
        lines.append(f"  [skipped] {item['project']}: {item['diagnostic']}")
    return lines


def main(argv: list[str] | None = None) -> int:
    options = parser().parse_args(argv)
    try:
        if options.command == "sources":
            sources, diagnostics = briefing.declared_sources(wanted_profile(options))
            say(
                options,
                {"sources": sources, "diagnostics": diagnostics},
                source_lines(sources, diagnostics),
            )
        elif options.command == "collect":
            manifest = briefing.collect(wanted_date(options), wanted_profile(options))
            say(options, manifest, run_lines({"manifest": manifest}))
        elif options.command == "wake":
            date, profile = wanted_run(options)
            result = briefing.wake(
                date, profile, ctl=getattr(options, "ctl", None) or None
            )
            say(options, result, [wake_line(result)])
        elif options.command == "pending":
            date = wanted_date(options)
            runs = [
                {
                    "date": manifest["date"],
                    "profile": manifest["profile"],
                    "sources": len(manifest["sources"]),
                    "message": briefing.wake_message(manifest),
                }
                for manifest in briefing.pending(date)
            ]
            say(
                options,
                {"date": date, "runs": runs},
                [f"{run['profile']} {run['sources']}" for run in runs],
            )
        elif options.command == "show":
            date, profile = wanted_run(options)
            run = briefing.read_run(date, profile)
            say(options, run, run_lines(run))
        elif options.command == "state":
            date, profile = wanted_run(options)
            state = briefing.run_state(date, profile)
            say(options, {"date": date, "profile": profile, "state": state}, [state])
        elif options.command == "publish":
            date, profile = wanted_run(options, undelivered=True)
            prose = sys.stdin.read(briefing.MAX_PROSE + 1)
            result = briefing.publish(date, profile, prose)
            say(options, result, [result["page"]])
        elif options.command == "render":
            path = briefing.render(*wanted_run(options))
            say(options, {"page": path}, [path])
        elif options.command == "open":
            path = briefing.render(*wanted_run(options))
            # The page is a local file and the desktop owns what opens it.
            subprocess.run(["xdg-open", path], check=False)
            say(options, {"page": path}, [path])
        elif options.command == "path":
            directory = briefing.run_dir(*wanted_run(options))
            say(options, {"run": str(directory)}, [str(directory)])
    except briefing.Refused as trouble:
        raise Stop(str(trouble)) from None
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Stop as stop:
        print(f"scufris-briefing: {stop}", file=sys.stderr)
        raise SystemExit(1) from None
