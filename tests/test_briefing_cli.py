"""The briefing command line, run as the agent runs it.

Every test runs the real program against a temporary state directory and a
temporary project root, with a harness that is a script.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[1]
COMMAND = REPOSITORY / "tools" / "briefing" / "cli.py"

ANSWERING = """#!/usr/bin/env python3
import os
import pathlib
print(pathlib.Path(os.environ["BRIEFING_ANSWER"]).read_text())
"""

OPENER = """#!/usr/bin/env python3
import os
import pathlib
import sys
pathlib.Path(os.environ["BRIEFING_OPENED"]).write_text(sys.argv[1])
"""

ENVELOPE = {
    "title": "The Den",
    "status": "ok",
    "headline": "Nothing is left over from yesterday.",
    "facts": [{"label": "Restant", "value": "0 tasks"}],
    "body": "Yesterday closed clean.",
}


class Command(unittest.TestCase):
    def setUp(self) -> None:
        self.room = tempfile.TemporaryDirectory(prefix="scufris-briefing-cli-")
        self.addCleanup(self.room.cleanup)
        self.root = Path(self.room.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name, program in (
            ("pi", ANSWERING),
            ("claude", ANSWERING),
            ("xdg-open", OPENER),
        ):
            executable = self.bin / name
            executable.write_text(program, encoding="utf-8")
            executable.chmod(0o755)
        self.answer = self.root / "answer.txt"
        self.answer.write_text(
            f"```json\n{json.dumps(ENVELOPE)}\n```\n", encoding="utf-8"
        )
        self.opened = self.root / "opened.txt"
        self.projects = self.root / "projects"
        self.project = self.projects / "the-den"
        self.project.mkdir(parents=True)
        subprocess.run(
            ["git", "init", "-b", "master"],
            cwd=self.project,
            check=True,
            capture_output=True,
        )
        (self.project / ".scufris.toml").write_text(
            "[briefings.morning]\n"
            'description = "Report the journal."\n'
            'keywords = { harness = "pi" }\n'
            'guidance = "Read the journal and report yesterday."\n',
            encoding="utf-8",
        )
        self.env = {
            **os.environ,
            "PATH": f"{self.bin}:{os.environ['PATH']}",
            "XDG_STATE_HOME": str(self.root / "state"),
            "SCUFRIS_PROJECT_ROOTS": json.dumps([str(self.projects)]),
            "BRIEFING_ANSWER": str(self.answer),
            "BRIEFING_OPENED": str(self.opened),
        }

    def run_briefing(self, *arguments: str, stdin: str = "") -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(COMMAND), "--date", "2026-08-31", *arguments],
            input=stdin,
            capture_output=True,
            text=True,
            check=False,
            env=self.env,
            timeout=60,
        )

    def ok(self, *arguments: str, stdin: str = "") -> str:
        done = self.run_briefing(*arguments, stdin=stdin)
        self.assertEqual(done.returncode, 0, done.stderr)
        return done.stdout.strip()

    def answered(self, *arguments: str, stdin: str = "") -> object:
        return json.loads(self.ok(*arguments, "--json", stdin=stdin))

    def test_sources_names_only_what_declares_the_profile(self) -> None:
        found = self.answered("sources")
        assert isinstance(found, dict)
        self.assertEqual(
            [item["project"] for item in found["sources"]], ["projects/the-den"]
        )
        self.assertEqual(self.answered("sources", "--profile", "evening")["sources"], [])

    def test_a_run_is_collected_shown_published_and_opened(self) -> None:
        collected = self.answered("collect")
        assert isinstance(collected, dict)
        self.assertEqual(collected["state"], "collected")
        self.assertEqual(collected["sources"][0]["headline"], ENVELOPE["headline"])

        shown = self.answered("show")
        assert isinstance(shown, dict)
        self.assertEqual(shown["contributions"][0]["body"], ENVELOPE["body"])
        self.assertIsNone(shown["prose"])

        published = self.answered("publish", stdin="Good morning. Nothing is waiting.")
        assert isinstance(published, dict)
        self.assertEqual(published["state"], "delivered")
        rendered = Path(published["page"]).read_text(encoding="utf-8")
        self.assertIn("Good morning. Nothing is waiting.", rendered)
        self.assertIn("Yesterday closed clean.", rendered)

        self.ok("open")
        self.assertEqual(self.opened.read_text(), published["page"])

    def test_plain_output_reads_as_lines(self) -> None:
        self.ok("collect")
        self.assertIn("[ok] projects/the-den:", self.ok("show"))
        self.assertTrue(self.ok("path").endswith("2026-08-31"))

    def test_showing_a_run_that_was_never_collected_is_refused(self) -> None:
        done = self.run_briefing("show")
        self.assertEqual(done.returncode, 1)
        self.assertIn("no briefing run for 2026-08-31", done.stderr)

    def test_a_date_that_is_not_a_date_is_refused(self) -> None:
        done = subprocess.run(
            [sys.executable, str(COMMAND), "--date", "yesterday", "show"],
            capture_output=True,
            text=True,
            check=False,
            env=self.env,
            timeout=60,
        )
        self.assertEqual(done.returncode, 1)
        self.assertIn("YYYY-MM-DD", done.stderr)


if __name__ == "__main__":
    unittest.main()
