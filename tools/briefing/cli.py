#!/usr/bin/env python3
"""The morning briefing on the command line.

One run of this asks every project that declares a briefing, keeps what they
said, and leaves a page beside it. The agent drives it through tools; a person
drives it here, which is also how it is tested.

    scufris-briefing sources
    scufris-briefing collect
    scufris-briefing show --json
    scufris-briefing publish < prose.md
    scufris-briefing open

Every subcommand works on one local date. `--date` names another one; without
it, today where the machine is.
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
        help="which briefing the projects declared; the default is morning",
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


def wanted_profile(options: argparse.Namespace) -> str:
    return briefing.validated_profile(getattr(options, "profile", None) or "morning")


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
            manifest = briefing.collect(wanted_profile(options), wanted_date(options))
            say(options, manifest, run_lines({"manifest": manifest}))
        elif options.command == "show":
            run = briefing.read_run(wanted_date(options))
            say(options, run, run_lines(run))
        elif options.command == "state":
            date = wanted_date(options)
            state = briefing.run_state(date)
            say(options, {"date": date, "state": state}, [state])
        elif options.command == "publish":
            prose = sys.stdin.read(briefing.MAX_PROSE + 1)
            result = briefing.publish(wanted_date(options), prose)
            say(options, result, [result["page"]])
        elif options.command == "render":
            path = briefing.render(wanted_date(options))
            say(options, {"page": path}, [path])
        elif options.command == "open":
            path = briefing.render(wanted_date(options))
            # The page is a local file and the desktop owns what opens it.
            subprocess.run(["xdg-open", path], check=False)
            say(options, {"page": path}, [path])
        elif options.command == "path":
            directory = briefing.run_dir(wanted_date(options))
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
