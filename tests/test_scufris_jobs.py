from __future__ import annotations

import hashlib
import importlib.machinery
import importlib.util
import json
import os
import secrets
import shutil
import subprocess
import tempfile
import time
import unicodedata
import unittest
from pathlib import Path
from typing import Any
from unittest import mock

REPOSITORY = Path(__file__).resolve().parents[1]
HELPER = REPOSITORY / "tools" / "jobs" / "scufris-jobs"
REPORTER = REPOSITORY / "tools" / "jobs" / "scufris-report"
MENU_FIXTURE = "scufris-menu.toml"

#: Sprout is a separate tool on the person's own path and not something this
#: repository builds. The tests that make a Sprout workspace run it for real,
#: so on a machine without it they say they were skipped rather than failing
#: on a program they were never given.
SPROUT = shutil.which("sprout")

FAKE_PI = """#!/usr/bin/env python3
import os
import pathlib
import sys
import time
state = pathlib.Path(os.environ['XDG_STATE_HOME'])
directory = state / 'scufris' / 'jobs' / os.environ['SCUFRIS_JOB_ID']
(directory / 'worker-prompt.txt').write_text(sys.argv[-1])
(directory / 'worker-argv.json').write_text(__import__('json').dumps(sys.argv[1:]))
if '--tools' in sys.argv:
    tools = set(sys.argv[sys.argv.index('--tools') + 1].split(','))
    if {'bash', 'edit', 'write'} & tools:
        (pathlib.Path.cwd() / 'PI_MUTATION').write_text('unsafe tools exposed\\n')
(directory / 'worker-capability').write_text(__import__('os').environ['SCUFRIS_REPORT_CAPABILITY'])
generation = int(__import__('os').environ['SCUFRIS_JOB_GENERATION'])
with (directory / 'status').open('a') as stream:
    for event, summary in [('working', 'fake worker started'), ('done', 'report complete')]:
        stream.write(__import__('json').dumps({'generation': generation, 'event': event, 'summary': summary}, separators=(',', ':')) + '\\n')
for line in sys.stdin:
    with (directory / 'received').open('a') as stream:
        stream.write(line)
    if line.strip() == '/exit':
        break
    time.sleep(0.01)
"""


def load_jobs_module() -> Any:
    loader = importlib.machinery.SourceFileLoader("scufris_jobs_test", str(HELPER))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    if spec is None:
        raise RuntimeError("could not load jobs helper")
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


FAKE_PI_EXIT_AFTER_DONE = """#!/usr/bin/env python3
import os
import pathlib
state = pathlib.Path(os.environ['XDG_STATE_HOME'])
directory = state / 'scufris' / 'jobs' / os.environ['SCUFRIS_JOB_ID']
generation = int(__import__('os').environ['SCUFRIS_JOB_GENERATION'])
with (directory / 'status').open('a') as stream:
    stream.write(__import__('json').dumps({'generation': generation, 'event': 'done', 'summary': 'assignment complete'}, separators=(',', ':')) + '\\n')
"""


class WorkerResourcePathTest(unittest.TestCase):
    def test_worker_report_extension_supports_packaged_and_source_layouts(
        self,
    ) -> None:
        jobs_module = load_jobs_module()
        with tempfile.TemporaryDirectory(
            prefix="scufris-worker-resource-"
        ) as temporary:
            root = Path(temporary)
            helper = root / "tools" / "jobs" / "scufris-jobs"
            helper.parent.mkdir(parents=True)
            helper.touch()

            packaged = root / "extensions" / "scufris" / "workflow" / "worker-report.ts"
            packaged.parent.mkdir(parents=True)
            packaged.touch()
            self.assertEqual(jobs_module.worker_report_extension(helper), packaged)

            packaged.unlink()
            source = (
                root
                / "agent"
                / "extensions"
                / "scufris"
                / "workflow"
                / "worker-report.ts"
            )
            source.parent.mkdir(parents=True)
            source.touch()
            self.assertEqual(jobs_module.worker_report_extension(helper), source)

            source.unlink()
            with self.assertRaisesRegex(
                jobs_module.JobError, "worker report extension is unavailable"
            ):
                jobs_module.worker_report_extension(helper)


class HelperBoundsTest(unittest.TestCase):
    """The bounds one side of the helper writes and another side reads."""

    def setUp(self) -> None:
        self.jobs = load_jobs_module()

    def test_a_summary_is_bounded_by_what_the_record_will_hold(self) -> None:
        # The record refuses a summary over 500 bytes. `parse_event` used to
        # accept 4096, so `read_events` copied one into the record, `store_job`
        # raised, and the cursor never advanced past the line: `events` and
        # `recover` failed for every job in the poll, forever.
        line = json.dumps(
            {"generation": 1, "event": "done", "summary": "x" * self.jobs.MAX_SUMMARY}
        )
        self.assertIsNotNone(self.jobs.parse_event(line))
        over = json.dumps(
            {
                "generation": 1,
                "event": "done",
                "summary": "x" * (self.jobs.MAX_SUMMARY + 1),
            }
        )
        self.assertIsNone(self.jobs.parse_event(over))
        # Measured in bytes, not code points. A 500-character summary of an em
        # dash is 1500 bytes and the record refuses it.
        wide = json.dumps({"generation": 1, "event": "done", "summary": "—" * 250})
        self.assertIsNone(self.jobs.parse_event(wide))
        # The record holds exactly what the parser admits.
        self.assertTrue(
            self.jobs.valid_record_text(
                "x" * self.jobs.MAX_SUMMARY, self.jobs.MAX_SUMMARY
            )
        )

    def test_every_door_refuses_the_same_worker_text(self) -> None:
        """The parse, record and report doors must agree on one predicate.

        They did not. `report` and `parse_event` tested
        `ord(character) < 32`; `valid_record_text`, which the record copy runs
        through, tested `str.isprintable()`. Every code point between them was
        admitted into the event log and then refused at the record, and because
        the cursor never advanced past the offending line, one worker summary
        wedged the event drain for every job, permanently.

        These are code points that pass an `ord < 32` test and fail
        `isprintable`: a C1 control, a no-break space, a soft hyphen, a
        zero-width joiner, a bidi override, a line separator and an unassigned
        code point.
        """
        wedging = [
            "\u009b",  # C1 control introducer
            "\u00a0",  # no-break space
            "\u00ad",  # soft hyphen
            "\u200d",  # zero-width joiner
            "\u202e",  # right-to-left override
            "\u2028",  # line separator
            "\u0378",  # unassigned
        ]
        for character in wedging:
            name = f"U+{ord(character):04X}"
            self.assertGreaterEqual(ord(character), 32, name)
            self.assertFalse(character.isprintable(), name)

            summary = f"work{character}ing"
            # The parse door refuses it.
            line = json.dumps({"generation": 1, "event": "done", "summary": summary})
            self.assertIsNone(self.jobs.parse_event(line), name)
            # The record door refuses it, as it always did.
            self.assertFalse(
                self.jobs.valid_record_text(summary, self.jobs.MAX_SUMMARY), name
            )
            # And they agree, which is the whole point.
            self.assertFalse(self.jobs.displayable(summary), name)

        # An ordinary summary still passes all of them, including one with a
        # space and non-ASCII prose.
        for good in ("work ing", "ran the checks", "wrote the caf\u00e9 page"):
            line = json.dumps({"generation": 1, "event": "done", "summary": good})
            self.assertIsNotNone(self.jobs.parse_event(line), good)
            self.assertTrue(
                self.jobs.valid_record_text(good, self.jobs.MAX_SUMMARY), good
            )

    def test_a_trimmed_report_keeps_the_history_that_fits(self) -> None:
        # The worker prompt tells a restarted execution to read `report.md` for
        # what the last one left it. Replacing the whole file with the newest
        # entry discarded every earlier finding and said nothing about it.
        entries = [
            b"# working: step " + str(index).encode() + b"\n\nWhat I found.\n"
            for index in range(4)
        ]
        current = b"\n".join(entries)
        self.assertEqual(self.jobs.report_entries(current), entries)
        fresh = b"# done: finished\n\nThe last word.\n"
        trimmed = self.jobs.trimmed_report(current, fresh)
        self.assertLessEqual(len(trimmed), self.jobs.MAX_REPORT_FILE)
        self.assertTrue(trimmed.startswith(b"# report trimmed"))
        self.assertIn(b"0 older entries dropped", trimmed)
        self.assertTrue(trimmed.endswith(fresh))
        for entry in entries:
            self.assertIn(entry, trimmed)
        # Entries too large to all fit drop the oldest first, and the marker
        # counts what went rather than leaving it to be guessed.
        big = b"# working: bulk\n\n" + b"y" * (self.jobs.MAX_REPORT_FILE // 2)
        cut = self.jobs.trimmed_report(b"\n".join([*entries, big, big]), fresh)
        self.assertLessEqual(len(cut), self.jobs.MAX_REPORT_FILE)
        self.assertTrue(cut.endswith(fresh))
        self.assertIn(big, cut)
        self.assertNotIn(entries[0], cut)
        self.assertRegex(cut, rb"[1-9]\d* older entr(y|ies) dropped")

    def test_report_entries_round_trips_the_file_it_reads(self) -> None:
        self.assertEqual(self.jobs.report_entries(b""), [])
        for current in (
            b"# only: one\n\nBody.\n",
            b"# a: one\n\nBody with a # hash.\n\n# b: two\n\nMore.\n",
        ):
            self.assertEqual(b"\n".join(self.jobs.report_entries(current)), current)


class ReplacementJobsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="scufris-jobs-")
        self.root = Path(self.temporary.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("pi", "claude"):
            executable = self.bin / name
            executable.write_text(FAKE_PI)
            executable.chmod(0o755)
        self.projects = self.root / "projects"
        self.project = self.projects / "nova-protocol"
        self.project.mkdir(parents=True)
        subprocess.run(
            ["git", "init", "-b", "master"],
            cwd=self.project,
            check=True,
            capture_output=True,
        )
        subprocess.run(
            ["git", "config", "user.email", "test@example.invalid"],
            cwd=self.project,
            check=True,
        )
        subprocess.run(
            ["git", "config", "user.name", "Scufris Test"],
            cwd=self.project,
            check=True,
        )
        (self.project / "README.md").write_text("# Fixture\n")
        (self.project / ".scufris.toml").write_text(
            """[conventions]
keywords = { tracking = "tatr", workspace = "sprout", base = "master" }
guidance = "Use project tasks."

[agents.work]
description = "Implement a change in the project."
keywords = { harness = "pi", model = "openai-codex/gpt-5.6-sol", thinking = "medium" }

[agents.review]
description = "Read the finished change and report findings."
keywords = { harness = "pi", model = "openai-codex/gpt-5.6-sol", thinking = "medium" }
"""
        )
        subprocess.run(
            ["git", "add", "README.md", ".scufris.toml"],
            cwd=self.project,
            check=True,
        )
        subprocess.run(
            ["git", "commit", "-m", "fixture"],
            cwd=self.project,
            check=True,
            capture_output=True,
        )
        self.env = os.environ.copy()
        for selector in ("TMUX", "TMUX_PANE"):
            self.env.pop(selector, None)
        # The helper always uses the default tmux server. TMUX_TMPDIR moves that
        # default under the fixture so tests never touch the developer server.
        self.env.update(
            {
                "PATH": f"{self.bin}:{self.env['PATH']}",
                "XDG_STATE_HOME": str(self.root / "state"),
                "TMUX_TMPDIR": str(self.root / "tmux"),
                "XDG_CONFIG_HOME": str(self.root / "config"),
                "SCUFRIS_PROJECT_ROOTS": json.dumps([str(self.projects)]),
            }
        )
        # The machine's own briefing sources are read from here, so a real
        # ~/.config/scufris/config.toml never reaches a test.
        self.env.pop("SCUFRIS_CONFIG", None)
        (self.root / "tmux").mkdir()
        self.jobs: list[str] = []
        self.trusted_capabilities: dict[str, str] = {}
        # Sprout features are keyed by project name in a shared user cache, so a
        # stale worktree from an interrupted run must never block a later one.
        self.run_token = secrets.token_hex(4)

    def tearDown(self) -> None:
        # Sprout worktrees live outside the fixture, so cleanup must ask for
        # their removal explicitly or every run leaks one.
        for job_id in self.jobs:
            self.call("stop", {"job_id": job_id, "remove_workspace": True}, check=False)
        self.temporary.cleanup()

    def call(
        self, command: str, request: dict[str, Any], *, check: bool = True
    ) -> dict[str, Any]:
        request = dict(request)
        job_id = request.get("job_id")
        if command == "spawn" and isinstance(job_id, str):
            capability = hashlib.sha256(f"trusted:{job_id}".encode()).hexdigest()
            self.trusted_capabilities[job_id] = capability
            request["trusted_capability"] = capability
        elif command == "failure" and isinstance(job_id, str):
            request.setdefault("capability", self.trusted_capabilities[job_id])
        result = subprocess.run(
            [str(HELPER), command],
            input=json.dumps(request),
            text=True,
            capture_output=True,
            env=self.env,
            timeout=30,
            check=False,
        )
        value = json.loads(result.stdout)
        if check and (result.returncode != 0 or not value["ok"]):
            self.fail(f"helper failed: {value} stderr={result.stderr}")
        return value

    def tmux(
        self, *arguments: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            ["tmux", *arguments],
            text=True,
            capture_output=True,
            env=self.env,
            timeout=10,
            check=False,
        )
        if check and result.returncode != 0:
            self.fail(f"tmux failed: {result.stderr}")
        return result

    def cli(
        self, *arguments: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            [str(REPOSITORY / "scripts" / "scufris-jobs"), *arguments],
            text=True,
            capture_output=True,
            env=self.env,
            timeout=30,
            check=False,
        )
        if check and result.returncode != 0:
            self.fail(
                f"CLI failed: {result.returncode} stdout={result.stdout} "
                f"stderr={result.stderr}"
            )
        return result

    def fixture_job(self, job_id: str, overrides: dict[str, Any] | None = None) -> Path:
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        directory.mkdir(parents=True)
        status = directory / "status"
        status.write_text(
            json.dumps(
                {"generation": 1, "event": "working", "summary": "fixture started"},
                separators=(",", ":"),
            )
            + "\n"
        )
        record: dict[str, Any] = {
            "version": 2,
            "job_id": job_id,
            "owner_session": "fixture-owner",
            "workflow_id": hashlib.sha256(f"workflow:{job_id}".encode()).hexdigest(),
            "root_job": job_id,
            "parent_job": None,
            "project": None,
            "project_root": None,
            "project_root_device": None,
            "project_root_inode": None,
            "context_fingerprint": None,
            "workspace": "temporary",
            "feature": None,
            "review_of": None,
            "working_directory": str(directory / "workspace"),
            "workspace_device": self.root.stat().st_dev,
            "workspace_inode": self.root.stat().st_ino,
            "landing_branch": None,
            "harness": "pi",
            "harness_session": "00000000-0000-4000-8000-000000000000",
            "model": "fixture-model",
            "thinking": "medium",
            "state": "working",
            "summary": "fixture starting",
            "created_at": "2026-08-23T12:00:00Z",
            "archived_at": None,
            "generation": 1,
            "event_offset": 0,
            "status_device": status.stat().st_dev,
            "status_inode": status.stat().st_ino,
            "execution_state": None,
            "tmux_session_name": None,
            "tmux_session_id": None,
            "tmux_window_id": None,
            "tmux_pane_id": None,
            "execution_token": None,
            "cleanup": None,
        }
        record.update(overrides or {})
        (directory / "job.json").write_text(json.dumps(record))
        (directory / "report.md").write_text("")
        (directory / "prompt.md").write_text("Fixture prompt.\n")
        (directory / "conversation.md").write_text("")
        return directory

    def assert_archived(self, job_id: str) -> dict[str, Any]:
        jobs = self.root / "state" / "scufris" / "jobs"
        self.assertFalse((jobs / job_id).exists())
        directory = jobs / "_archive" / job_id
        self.assertTrue(directory.is_dir())
        record = json.loads((directory / "job.json").read_text())
        self.assertIsNotNone(record["archived_at"])
        self.assertIsNone(record["execution_state"])
        self.assertTrue((directory / "report.md").is_file())
        self.assertTrue((directory / "status").is_file())
        return record

    def worker_capability(self, job_id: str) -> str:
        path = self.root / "state" / "scufris" / "jobs" / job_id / "worker-capability"
        self.wait_for(path, "")
        return path.read_text()

    def wait_for(self, path: Path, text: str) -> None:
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            if path.exists():
                content = path.read_text()
                if text in content:
                    return
                if ": " in text:
                    event, summary = text.split(": ", 1)
                    for line in content.splitlines():
                        try:
                            value = json.loads(line)
                        except json.JSONDecodeError:
                            continue
                        if (
                            value.get("event") == event
                            and value.get("summary") == summary
                        ):
                            return
            time.sleep(0.05)
        self.fail(f"timed out waiting for {text!r} in {path}")

    def test_jobs_cli_empty_aliases_and_lookup_errors(self) -> None:
        for arguments in ((), ("all",), ("--all",)):
            result = self.cli(*arguments)
            self.assertEqual(result.stdout, "No Scufris jobs.\n")
        self.assertEqual(json.loads(self.cli("all", "--json").stdout), {"jobs": []})

        jobs = self.root / "state" / "scufris" / "jobs"
        (jobs / "abc111111111").mkdir(parents=True)
        (jobs / "abc222222222").mkdir()
        ambiguous = self.cli("abc", check=False)
        self.assertEqual(ambiguous.returncode, 1)
        self.assertIn("ambiguous job ID abc", ambiguous.stderr)
        missing = self.cli("def", "--json", check=False)
        self.assertEqual(missing.returncode, 1)
        self.assertEqual(json.loads(missing.stdout), {"error": "job ID not found: def"})
        invalid = self.cli("not-an-id", check=False)
        self.assertEqual(invalid.returncode, 1)
        self.assertIn("lowercase hexadecimal", invalid.stderr)
        usage = self.cli("abc111111111", "--all", check=False)
        self.assertEqual(usage.returncode, 2)
        self.assertIn("--all cannot be used with a job ID", usage.stderr)

    def test_jobs_cli_rejects_invalid_records_and_unsafe_artifacts(self) -> None:
        invalid_records = {
            "100000000001": {"unexpected": True},
            "100000000002": {"owner_session": 7},
            "100000000003": {"harness": "shell"},
            "100000000004": {"workspace": "sprout"},
        }
        for job_id, override in invalid_records.items():
            directory = self.fixture_job(job_id, override)
            if "unexpected" in override:
                record = json.loads((directory / "job.json").read_text())
                record["unexpected"] = record.pop("unexpected")
                (directory / "job.json").write_text(json.dumps(record))

        malformed = self.fixture_job("200000000001")
        (malformed / "job.json").write_text("{")
        oversized = self.fixture_job("200000000002")
        (oversized / "job.json").write_bytes(b" " * (64 * 1024 + 1))
        symlinked = self.fixture_job("200000000003")
        (symlinked / "job.json").unlink()
        (symlinked / "job.json").symlink_to(malformed / "job.json")
        special = self.fixture_job("200000000004")
        (special / "job.json").unlink()
        os.mkfifo(special / "job.json")

        listed = json.loads(self.cli("all", "--json").stdout)["jobs"]
        self.assertEqual(len(listed), 8)
        self.assertTrue(all(job["state"] == "invalid" for job in listed))
        for job_id in invalid_records:
            detail = self.cli(job_id, "--json", check=False)
            self.assertEqual(detail.returncode, 1)
            self.assertIn("job record is invalid", json.loads(detail.stdout)["error"])
        self.assertIn(
            "job record is invalid",
            json.loads(self.cli("200000000001", "--json", check=False).stdout)["error"],
        )
        self.assertIn(
            "too large",
            json.loads(self.cli("200000000002", "--json", check=False).stdout)["error"],
        )
        for job_id in ("200000000003", "200000000004"):
            result = self.cli(job_id, "--json", check=False)
            self.assertEqual(result.returncode, 1)
            self.assertIn("job.json", json.loads(result.stdout)["error"])

        safe = self.fixture_job("300000000001")
        outside = self.root / "outside"
        outside.write_text("working: followed unsafe path\n")
        for name in ("status", "report.md", "project-context.md"):
            path = safe / name
            path.unlink(missing_ok=True)
            path.symlink_to(outside)
            result = self.cli("300000000001", "--json", check=False)
            self.assertEqual(result.returncode, 1, name)
            self.assertIn(name, json.loads(result.stdout)["error"])
            path.unlink()
            path.write_text("" if name != "status" else "working: restored\n")
        prompt = safe / "prompt.md"
        prompt.unlink()
        os.mkfifo(prompt)
        result = self.cli("300000000001", "--json", check=False)
        self.assertEqual(result.returncode, 1)
        self.assertIn("prompt.md", json.loads(result.stdout)["error"])

    def test_jobs_cli_bounds_artifacts_and_escapes_only_human_output(self) -> None:
        directory = self.fixture_job("400000000001")
        escape_text = "line\x1b[31m red\x07\rnext"
        (directory / "report.md").write_text(escape_text)
        (directory / "project-context.md").write_text(escape_text)
        (directory / "prompt.md").write_text(escape_text)
        status_summary = "status\u009b31m text"
        status_lines = "".join(
            json.dumps(
                {"generation": 1, "event": "working", "summary": summary},
                separators=(",", ":"),
            )
            + "\n"
            for summary in [
                *(f"update {index}" for index in range(20000)),
                status_summary,
            ]
        )
        (directory / "status").write_text(status_lines)

        detail = json.loads(self.cli("400000000001", "--json").stdout)
        self.assertEqual(detail["report"], escape_text)
        self.assertEqual(detail["project_context"], escape_text)
        self.assertEqual(detail["prompt"], escape_text)
        # The poisoned line is refused at the parse door rather than shown.
        # `\u009b` passed `ord(character) < 32` at `report` and `parse_event`
        # and failed `isprintable()` at the record, so copying it into the
        # record raised and every job's event drain wedged behind it. One
        # predicate at all four doors means a status file that already holds
        # such a line drains past it to the last good reading.
        self.assertEqual(detail["summary"], "update 19999")
        self.assertLessEqual(len(detail["events"]), 100)
        human = self.cli("400000000001").stdout
        self.assertNotIn("\x1b", human)
        self.assertNotIn("\x07", human)
        self.assertNotIn("\r", human)
        self.assertIn(r"\x1b[31m red\x07\x0dnext", human)
        self.assertNotIn("\u009b", human)

        detail_maximum = 512 * 1024
        report_maximum = 2 * 1024 * 1024
        (directory / "report.md").write_bytes(b"r" * (report_maximum + 100))
        (directory / "project-context.md").write_bytes(b"c" * (detail_maximum + 100))
        (directory / "prompt.md").write_bytes(b"p" * (detail_maximum + 100))
        bounded = json.loads(self.cli("400000000001", "--json").stdout)
        self.assertEqual(len(bounded["report"]), report_maximum)
        self.assertEqual(len(bounded["project_context"]), detail_maximum)
        self.assertEqual(len(bounded["prompt"]), detail_maximum)

    def test_jobs_cli_uses_unicode_display_cell_widths(self) -> None:
        model = "界" * 20
        summary = ("界e\u0301" * 40) + " end"
        directory = self.fixture_job("500000000001", {"model": model})
        (directory / "status").write_text(
            json.dumps(
                {"generation": 1, "event": "working", "summary": summary},
                separators=(",", ":"),
            )
            + "\n"
        )
        parsed = json.loads(self.cli("all", "--json").stdout)["jobs"][0]
        self.assertEqual(parsed["model"], model)
        self.assertEqual(parsed["summary"], summary)

        lines = self.cli("all").stdout.splitlines()
        separator, row = lines[2], lines[3]

        def cells(text: str) -> int:
            return sum(
                0
                if unicodedata.combining(character)
                else 2
                if unicodedata.east_asian_width(character) in {"W", "F"}
                else 1
                for character in text
            )

        self.assertEqual(cells(row), cells(separator))
        self.assertIn("...", row)

    def test_done_is_terminal_without_spurious_harness_failure(self) -> None:
        executable = self.bin / "pi"
        executable.write_text(FAKE_PI_EXIT_AFTER_DONE)
        executable.chmod(0o755)
        job_id = "aaa111bbb222"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Finish and exit unexpectedly.",
                "owner_session": "foreground-session",
            },
        )
        self.jobs.append(job_id)
        status = self.root / "state" / "scufris" / "jobs" / job_id / "status"
        self.wait_for(status, '"event":"done","summary":"assignment complete"')
        events = self.call("events", {"jobs": [{"job_id": job_id}]})["result"]["jobs"][
            0
        ]
        self.assertEqual(
            [event["line"] for event in events["events"]],
            ["done: assignment complete"],
        )
        inspected = self.call("inspect", {"job_id": job_id})["result"]
        self.assertEqual(inspected["state"], "done")
        self.assertFalse(inspected["window_alive"])
        self.assertNotIn("worker harness exited unexpectedly", status.read_text())

    def test_project_context_is_an_agent_menu_and_bad_config_is_ignored(self) -> None:
        projects = self.call("projects", {})["result"]["projects"]
        self.assertEqual(projects, ["projects/nova-protocol"])
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        self.assertTrue(context["configured"])
        markdown = context["markdown"]
        self.assertIn("## Conventions", markdown)
        self.assertIn("tracking: tatr", markdown)
        self.assertIn("workspace: sprout", markdown)
        self.assertIn("An explicit instruction in the request wins", markdown)
        self.assertIn("## Agents", markdown)
        self.assertIn("This is a menu, not a workflow.", markdown)
        self.assertIn("### work", markdown)
        self.assertIn("Implement a change in the project.", markdown)
        self.assertIn("### review", markdown)
        # Exact tokens the orchestrator must reproduce stay on their own line.
        self.assertIn("model: openai-codex/gpt-5.6-sol", markdown)
        self.assertIn("thinking: medium", markdown)
        self.assertIn("Never start an agent because the project declares it.", markdown)
        # A menu declares no sequence and no gate of its own.
        self.assertNotIn("Follow these project preferences", markdown)

        # A project agent Scufris has never seen renders like any other entry.
        (self.project / ".scufris.toml").write_text(
            "[agents.fuzz]\n"
            'description = "Run the differential fuzzer against the change."\n'
            'keywords = { harness = "claude", model = "opus", thinking = "xhigh" }\n'
        )
        unfamiliar = self.call("context", {"project": "projects/nova-protocol"})[
            "result"
        ]
        self.assertTrue(unfamiliar["configured"])
        self.assertIsNone(unfamiliar["diagnostic"])
        self.assertIn("### fuzz", unfamiliar["markdown"])
        self.assertIn("harness: claude", unfamiliar["markdown"])

        (self.project / ".scufris.toml").write_text("not = [valid")
        ignored = self.call("context", {"project": "projects/nova-protocol"})["result"]
        self.assertFalse(ignored["configured"])
        self.assertIn("ignored .scufris.toml", ignored["diagnostic"])

        # The retired workflow shape is refused rather than half-read.
        (self.project / ".scufris.toml").write_text(
            '[preferences.implementation]\nkeywords = { harness = "pi" }\n'
        )
        retired = self.call("context", {"project": "projects/nova-protocol"})["result"]
        self.assertFalse(retired["configured"])
        self.assertIn("preferences workflow shape was retired", retired["diagnostic"])

        # Every agent says what it is for, so the menu can be read by name.
        (self.project / ".scufris.toml").write_text(
            '[agents.work]\nkeywords = { harness = "pi" }\n'
        )
        undescribed = self.call("context", {"project": "projects/nova-protocol"})[
            "result"
        ]
        self.assertFalse(undescribed["configured"])
        self.assertIn("short printable description", undescribed["diagnostic"])

        # Keyword values must stay flat so they render as copyable scalars.
        (self.project / ".scufris.toml").write_text(
            "[agents.work]\n"
            'description = "Implement a change."\n'
            "keywords = { model = { nested = 1 } }\n"
        )
        nested = self.call("context", {"project": "projects/nova-protocol"})["result"]
        self.assertFalse(nested["configured"])
        self.assertIn("scalars", nested["diagnostic"])

        # A list of scalars is a legitimate keyword value.
        (self.project / ".scufris.toml").write_text(
            '[conventions]\nkeywords = { checks = ["npm run check", "nix flake check"] }\n'
        )
        listed = self.call("context", {"project": "projects/nova-protocol"})["result"]
        self.assertTrue(listed["configured"])
        self.assertIn("checks: npm run check, nix flake check", listed["markdown"])

        for adapter in (
            '{ harness = "claude", model = "opus", thinking = "minimal" }',
            '{ harness = "unknown", model = "reviewer", thinking = "medium" }',
            '{ harness = "pi", model = "reviewer", thinking = "max" }',
        ):
            (self.project / ".scufris.toml").write_text(
                f'[agents.review]\ndescription = "Review the change."\nkeywords = {adapter}\n'
            )
            unsupported = self.call("context", {"project": "projects/nova-protocol"})[
                "result"
            ]
            self.assertFalse(unsupported["configured"])
            self.assertIn("unsupported adapter", unsupported["diagnostic"])

    def test_the_machines_own_sources_are_read_beside_the_projects(self) -> None:
        # A source with no project is declared once for the machine. It is an
        # ordinary entry: the same reader validates it, and its slug is
        # namespaced so a section named for a project cannot take that
        # project's contribution file.
        menu = (self.project / ".scufris.toml").read_text()
        (self.project / ".scufris.toml").write_text(
            menu + "\n[briefings.morning]\n"
            'description = "Report the project."\n'
            'guidance = "Read the project."\n'
        )
        config = self.root / "config" / "scufris" / "config.toml"
        config.parent.mkdir(parents=True)
        config.write_text(
            "[briefings.morning.jobs]\n"
            'description = "What Scufris did overnight."\n'
            'keywords = { harness = "pi", thinking = "medium" }\n'
            'guidance = "Read the job history."\n'
            "[briefings.morning.projects-nova-protocol]\n"
            'description = "Named for a project."\n'
            f'guidance = "Run in a named root."\nroot = "{self.root}"\n'
        )
        listed = self.call("briefings", {"profile": "morning"})["result"]
        self.assertEqual(listed["diagnostics"], [])
        self.assertEqual(
            [item["slug"] for item in listed["sources"]],
            ["@jobs", "@projects-nova-protocol", "projects-nova-protocol"],
        )
        machine = listed["sources"][0]
        self.assertEqual(machine["project"], "@jobs")
        self.assertEqual(machine["guidance"], "Read the job history.")
        # No root of its own means the machine's own home.
        self.assertEqual(machine["root"], str(Path.home()))
        self.assertEqual(listed["sources"][1]["root"], str(self.root.resolve()))

        # A named file that is not there is a refusal; the default path being
        # absent is a machine that declared none.
        refused = self.call(
            "briefings",
            {"profile": "morning", "config": "/nonexistent/typo.toml"},
            check=False,
        )
        self.assertFalse(refused["ok"])
        self.assertIn("typo.toml", refused["error"])

        # A malformed user file costs the user file and nothing else.
        config.write_text("this is not = toml [\n")
        broken = self.call("briefings", {"profile": "morning"})["result"]
        self.assertEqual(
            [item["project"] for item in broken["sources"]], ["projects/nova-protocol"]
        )
        self.assertEqual(len(broken["diagnostics"]), 1)
        self.assertEqual(broken["diagnostics"][0]["project"], str(config))

    def test_non_store_user_config_symlinks_are_refused_before_toml_is_read(
        self,
    ) -> None:
        config = self.root / "config" / "scufris" / "config.toml"
        config.parent.mkdir(parents=True)
        generated = self.root / "generated-config.toml"
        generated.write_text(
            "[briefings.morning.jobs]\n"
            'description = "Job history."\n'
            'guidance = "Read the history."\n'
        )
        config.symlink_to(generated)
        listed = self.call("briefings", {"profile": "morning"})["result"]
        self.assertEqual(listed["sources"], [])
        self.assertIn("unsafe symlink", listed["diagnostics"][0]["diagnostic"])

        config.unlink()
        config.symlink_to("/dev/zero")
        listed = self.call("briefings", {"profile": "morning"})["result"]
        self.assertEqual(listed["sources"], [])
        self.assertIn("unsafe symlink", listed["diagnostics"][0]["diagnostic"])

    def test_the_history_listing_answers_for_jobs_no_other_listing_can(self) -> None:
        # Archiving is what happens to a workflow that finished, and every
        # other listing skips the archive. Nothing could answer what landed.
        live = "aa0000000001"
        self.fixture_job(live, {"created_at": "2026-09-08T05:00:00Z"})
        old = "cc0000000003"
        self.fixture_job(old, {"created_at": "2026-08-01T09:00:00Z"})
        landed = "bb0000000002"
        directory = self.fixture_job(
            landed,
            {
                "created_at": "2026-09-07T10:00:00Z",
                "archived_at": "2026-09-08T06:30:00Z",
                "state": "done",
                "summary": "the change landed",
            },
        )
        (directory / "receipts.jsonl").write_text(
            json.dumps(
                {
                    "job_id": landed,
                    "measured_at": "2026-09-08T06:29:00Z",
                    "trigger": "cleanup",
                    "facts": {"landed": True, "pushed": False},
                    "claims": [
                        {
                            "claim": "pushed",
                            "said": "I pushed the branch.",
                            "field": "pushed",
                            "measured": False,
                            "verdict": "claimed, not verified",
                        }
                    ],
                    "sentences": ["pushed: claimed, not verified"],
                },
                sort_keys=True,
            )
            + "\n"
        )
        archive = self.root / "state" / "scufris" / "jobs" / "_archive"
        archive.mkdir(parents=True)
        directory.rename(archive / landed)

        # The live listing cannot see it, by design.
        self.assertEqual(
            {
                job["job_id"]
                for job in json.loads(self.cli("all", "--json").stdout)["jobs"]
            },
            {live, old},
        )
        listed = self.call("history", {"since": "2026-09-08T00:00:00Z"})["result"]
        self.assertEqual([job["job_id"] for job in listed["jobs"]], [landed, live])
        recorded = listed["jobs"][0]
        self.assertTrue(recorded["archived"])
        self.assertEqual(recorded["receipt_count"], 1)
        self.assertEqual(
            recorded["receipt"]["sentences"], ["pushed: claimed, not verified"]
        )
        # Without a window it answers for everything on disk.
        self.assertEqual(
            {job["job_id"] for job in self.call("history", {})["result"]["jobs"]},
            {live, old, landed},
        )
        # A window that is not a moment is refused rather than ignored.
        refused = self.call("history", {"since": "last night"}, check=False)
        self.assertFalse(refused["ok"])
        self.assertIn("ISO 8601", refused["error"])

        # A source reads it through the same command a person does.
        rows = self.cli("history", "--since", "2026-09-08T00:00:00Z").stdout
        self.assertIn(landed, rows)
        self.assertIn("pushed: claimed, not verified", rows)
        self.assertIn("archived", rows)
        self.assertNotIn(old, rows)

    def test_a_briefing_source_is_listed_but_never_offered_as_an_agent(self) -> None:
        menu = (self.project / ".scufris.toml").read_text()
        (self.project / ".scufris.toml").write_text(
            menu + "\n[briefings.morning]\n"
            'description = "Report the cadence gap and pending QA."\n'
            'keywords = { harness = "claude", model = "opus", thinking = "high" }\n'
            'guidance = "Read web/data and report what changed overnight."\n'
            "\n[briefings.weekly]\n"
            'description = "Report the week."\n'
            'guidance = "Read the week."\n'
        )
        listed = self.call("briefings", {"profile": "morning"})["result"]
        self.assertEqual(listed["diagnostics"], [])
        self.assertEqual(len(listed["sources"]), 1)
        source = listed["sources"][0]
        self.assertEqual(source["project"], "projects/nova-protocol")
        self.assertEqual(source["root"], str(self.project))
        self.assertEqual(source["slug"], "projects-nova-protocol")
        self.assertEqual(source["harness"], "claude")
        self.assertEqual(source["model"], "opus")
        self.assertEqual(source["thinking"], "high")
        self.assertIn("Read web/data", source["guidance"])
        # A profile nobody declared is an empty morning, not an error.
        self.assertEqual(
            self.call("briefings", {"profile": "evening"})["result"]["sources"], []
        )
        self.assertEqual(
            len(self.call("briefings", {"profile": "weekly"})["result"]["sources"]), 1
        )

        # The delegation menu must not carry it. A briefing entry rendered
        # beside the agents reads as one more agent the request may name.
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        self.assertTrue(context["configured"])
        self.assertIsNone(context["diagnostic"])
        self.assertIn("### work", context["markdown"])
        self.assertNotIn("morning", context["markdown"])
        self.assertNotIn("Read web/data", context["markdown"])

    def test_a_broken_briefing_table_costs_the_briefing_and_not_the_menu(self) -> None:
        menu = (self.project / ".scufris.toml").read_text()
        (self.project / ".scufris.toml").write_text(
            menu + '\n[briefings.morning]\nguidance = "Read the project."\n'
        )
        listed = self.call("briefings", {"profile": "morning"})["result"]
        self.assertEqual(listed["sources"], [])
        self.assertEqual(len(listed["diagnostics"]), 1)
        self.assertIn(
            "short printable description", listed["diagnostics"][0]["diagnostic"]
        )
        # The agents survive it.
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        self.assertTrue(context["configured"])
        self.assertIn("### work", context["markdown"])

    def test_a_project_that_only_declares_a_briefing_keeps_a_usable_context(
        self,
    ) -> None:
        (self.project / ".scufris.toml").write_text(
            "[briefings.morning]\n"
            'description = "Report yesterday from the journal."\n'
            'guidance = "Run scufris-den and report the day."\n'
        )
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        self.assertTrue(context["configured"])
        self.assertIsNone(context["diagnostic"])
        self.assertNotIn("scufris-den", context["markdown"])
        self.assertIn(
            "Never start an agent because the project declares it.", context["markdown"]
        )
        self.assertEqual(
            len(self.call("briefings", {"profile": "morning"})["result"]["sources"]), 1
        )

    def test_recommended_menu_fixture_renders_conventions_and_every_agent(
        self,
    ) -> None:
        # The fixture is the shape a real .scufris.toml must take. Rendering it
        # here keeps the documented snippet and the parser in step.
        jobs_module = load_jobs_module()
        rendered = jobs_module.render_context(
            "projects/nova-protocol", REPOSITORY / "tests" / "fixtures" / MENU_FIXTURE
        )
        self.assertTrue(rendered["configured"])
        self.assertIsNone(rendered["diagnostic"])
        markdown = rendered["markdown"]
        self.assertIn("## Conventions", markdown)
        self.assertIn("workspace: sprout", markdown)
        self.assertIn("This is a menu, not a workflow.", markdown)
        for heading in ("### work", "### review", "### quick-review"):
            self.assertIn(heading, markdown)
        # A fresh reviewer re-derives fault every round, so the menu tells the
        # foreground to steer the one review job instead of spawning another.
        #
        # Matched against the guidance with its line breaks collapsed. The
        # sentence is prose in a wrapped block, so where it wraps is the
        # author's business and not something an assertion should pin.
        flowed = " ".join(markdown.split())
        self.assertIn("steer that same job with scufris_job_send", flowed)
        self.assertIn("no record of what it already accepted", flowed)
        # The review agent is run through the project's own review command.
        self.assertIn("command: /scufris-review", markdown)
        self.assertIn("Run `/scufris-review` over the range", flowed)

    def test_general_job_uses_temporary_workspace_and_generic_events(self) -> None:
        job_id = "abc123def456"
        result = self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Create a research report.",
                "owner_session": "foreground-session",
            },
        )["result"]
        self.jobs.append(job_id)
        self.assertEqual(result["project"], None)
        self.assertEqual(result["workspace"], "temporary")
        self.assertEqual(result["harness"], "pi")
        self.assertEqual(result["model"], "openai-codex/gpt-5.6-sol")
        self.assertEqual(result["thinking"], "medium")
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(
            directory / "status", '"event":"done","summary":"report complete"'
        )
        self.assertTrue((directory / "workspace").is_dir())
        self.assertFalse((directory / "project-context.md").exists())
        prompt = (directory / "prompt.md").read_text()
        self.assertIn("done: <summary>", prompt)
        self.assertNotIn("ready:", prompt)
        self.assertNotIn("needs-decision:", prompt)
        self.assertIn("You cannot report `failed`", prompt)
        self.assertIn("Call the `scufris_report` tool.", prompt)

        events = self.call("events", {"jobs": [{"job_id": job_id}]})["result"]["jobs"][
            0
        ]
        self.assertEqual(
            [event["line"] for event in events["events"]],
            ["working: fake worker started", "done: report complete"],
        )
        for event in events["events"]:
            self.call("ack-event", {"job_id": job_id, "event_id": event["id"]})
        worker_capability = self.worker_capability(job_id)
        reporter_env = {
            **self.env,
            "SCUFRIS_JOB_ID": job_id,
            "SCUFRIS_REPORT_CAPABILITY": worker_capability,
        }
        adapted = subprocess.run(
            [str(REPORTER), "working", "report adapter verified"],
            input="# Adapter\n\nThe report adapter works.\n",
            text=True,
            capture_output=True,
            env=reporter_env,
            check=True,
            timeout=30,
        )
        self.assertEqual(
            adapted.stdout,
            "appended working: report adapter verified\n",
        )
        rejected_failure = self.call(
            "report",
            {
                "job_id": job_id,
                "capability": worker_capability,
                "event": "failed",
                "summary": "worker selected failure",
                "report": "# Invalid\n",
            },
            check=False,
        )
        self.assertFalse(rejected_failure["ok"])
        self.assertIn("only working, blocked, or done", rejected_failure["error"])
        rejected_adapter = subprocess.run(
            [str(REPORTER), "failed", "worker selected failure"],
            input="# Invalid\n",
            text=True,
            capture_output=True,
            env=reporter_env,
            check=False,
            timeout=30,
        )
        self.assertEqual(rejected_adapter.returncode, 2)
        self.assertIn("working, blocked, or done", rejected_adapter.stderr)

        reported = self.call(
            "report",
            {
                "job_id": job_id,
                "capability": worker_capability,
                "event": "done",
                "summary": "research report complete",
                "report": "# Result\n\nThe report is complete.\n",
            },
        )["result"]
        self.assertEqual(reported["event"], "done")
        self.assertEqual(
            (directory / "report.md").read_text(),
            "# working: report adapter verified\n\n"
            "# Adapter\n\nThe report adapter works.\n\n"
            "# done: research report complete\n\n"
            "# Result\n\nThe report is complete.\n",
        )
        inspected = self.call("inspect", {"job_id": job_id, "include_report": True})[
            "result"
        ]
        self.assertEqual(inspected["report"], (directory / "report.md").read_text())
        next_events = self.call("events", {"jobs": [{"job_id": job_id}]})["result"][
            "jobs"
        ][0]
        self.assertEqual(
            [event["line"] for event in next_events["events"]],
            [
                "working: report adapter verified",
                "done: research report complete",
            ],
        )
        for event in next_events["events"]:
            self.call("ack-event", {"job_id": job_id, "event_id": event["id"]})
        large_detail = "x" * (500 * 1024)
        for index in range(5):
            self.call(
                "report",
                {
                    "job_id": job_id,
                    "capability": worker_capability,
                    "event": "working",
                    "summary": f"bounded update {index}",
                    "report": large_detail,
                },
            )
        bounded_report = (directory / "report.md").read_text()
        self.assertLessEqual(len(bounded_report.encode()), 2 * 1024 * 1024)
        # The ceiling keeps the newest whole entries rather than replacing the
        # file with one of them. A restarted execution is told to read this
        # file for what the last one left it, so what it loses has to be the
        # oldest, and it has to say so.
        self.assertTrue(bounded_report.startswith("# report trimmed\n"))
        self.assertRegex(bounded_report, r"[1-9]\d* older entr(y|ies) dropped")
        self.assertTrue(
            bounded_report.endswith(
                "# working: bounded update 4\n\n" + large_detail + "\n"
            )
        )
        self.assertIn("# working: bounded update 3\n", bounded_report)
        self.assertNotIn("# working: bounded update 0\n", bounded_report)

        restarted = self.call(
            "send", {"job_id": job_id, "message": "Continue carefully."}
        )["result"]
        self.assertTrue(restarted["restarted"])
        self.assertEqual(restarted["generation"], 2)
        self.assertIn(
            "Continue carefully.", (directory / "conversation.md").read_text()
        )

        listed_jobs = json.loads(self.cli("all", "--json").stdout)["jobs"]
        self.assertEqual([item["job_id"] for item in listed_jobs], [job_id])
        self.assertEqual(
            self.cli("--all", "--json").stdout,
            self.cli("--json").stdout,
        )
        table = self.cli("all").stdout
        self.assertIn("Columns: ID=job ID; STATE=latest event; LIVE=worker pane", table)
        self.assertIn("ID            STATE", table)
        self.assertIn(job_id, table)

        detail = self.cli(job_id[:6]).stdout
        self.assertIn(f"Job ID: {job_id}", detail)
        self.assertIn("Created: ", detail)
        self.assertIn(f"Working directory: {directory / 'workspace'}", detail)
        self.assertIn("Tmux pane ID: %", detail)
        self.assertIn("  g1 working: report adapter verified\n", detail)
        self.assertIn("Report:\n# report trimmed", detail)
        self.assertIn("# working: bounded update 4", detail)
        self.assertIn("Prompt:\n# Scufris delegated job", detail)
        json_detail = json.loads(self.cli(job_id, "--json").stdout)
        self.assertEqual(json_detail["job_id"], job_id)
        self.assertEqual(json_detail["report"], bounded_report)

        stopped = self.call("stop", {"job_id": job_id})["result"]
        self.assertTrue(stopped["clean"])
        self.assert_archived(job_id)
        self.assertEqual(json.loads(self.cli("all", "--json").stdout), {"jobs": []})
        archived_detail = json.loads(self.cli(job_id, "--json").stdout)
        self.assertEqual(archived_detail["report"], bounded_report)
        self.assertIsNotNone(archived_detail["archived_at"])
        refused = self.call("send", {"job_id": job_id, "message": "again"}, check=False)
        self.assertFalse(refused["ok"])
        self.assertIn("archived", refused["error"])

    def test_trusted_failure_is_linked_and_report_symlinks_are_refused(self) -> None:
        job_id = "123abc456def"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Wait for a trusted failure fixture.",
                "owner_session": "foreground-session",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")
        self.call(
            "failure",
            {
                "job_id": job_id,
                "summary": "invalid worker status",
                "report": "Trusted orchestration rejected an invalid status.",
            },
        )
        self.assertEqual(
            (directory / "report.md").read_text(),
            "# failed: invalid worker status\n\n"
            "Trusted orchestration rejected an invalid status.\n",
        )
        self.assertEqual((directory / "report.md").stat().st_mode & 0o777, 0o600)
        self.assertEqual((directory / "status").stat().st_mode & 0o777, 0o600)

        report_path = directory / "report.md"
        report_path.unlink()
        report_path.symlink_to(directory / "status")
        rejected_inspection = self.call(
            "inspect", {"job_id": job_id, "include_report": True}, check=False
        )
        self.assertFalse(rejected_inspection["ok"])
        self.assertIn("report.md", rejected_inspection["error"])
        rejected = self.call(
            "report",
            {
                "job_id": job_id,
                "capability": self.worker_capability(job_id),
                "event": "working",
                "summary": "must not follow symlink",
                "report": "Invalid write target.",
            },
            check=False,
        )
        self.assertFalse(rejected["ok"])
        self.assertIn("report.md", rejected["error"])
        self.assertNotIn("must not follow symlink", (directory / "status").read_text())

    def test_worker_capabilities_are_job_bound_and_details_are_byte_bounded(
        self,
    ) -> None:
        job_ids = ["a1b2c3d4e5f6", "f6e5d4c3b2a1"]
        for job_id in job_ids:
            self.call(
                "spawn",
                {
                    "job_id": job_id,
                    "instructions": "Wait for capability checks.",
                    "owner_session": "foreground-session",
                },
            )
            self.jobs.append(job_id)
            directory = self.root / "state" / "scufris" / "jobs" / job_id
            self.wait_for(directory / "status", "done: report complete")
        first_capability = self.worker_capability(job_ids[0])
        first_directory = self.root / "state" / "scufris" / "jobs" / job_ids[0]
        auth_text = (first_directory / ".report-auth.json").read_text()
        self.assertNotIn(first_capability, auth_text)
        self.assertNotIn(self.trusted_capabilities[job_ids[0]], auth_text)
        self.assertNotIn("capability", (first_directory / "job.json").read_text())
        self.assertEqual(
            (first_directory / ".report-auth.json").stat().st_mode & 0o777, 0o400
        )
        self.assertEqual(
            (first_directory / "report.lock").stat().st_mode & 0o777, 0o600
        )
        forged_launch = subprocess.run(
            [str(HELPER), "launch", job_ids[0], first_capability],
            text=True,
            capture_output=True,
            env=self.env,
            check=False,
            timeout=30,
        )
        self.assertNotEqual(forged_launch.returncode, 0)
        self.assertIn("launch capability does not authorize", forged_launch.stdout)
        second_directory = self.root / "state" / "scufris" / "jobs" / job_ids[1]
        prior_status = (second_directory / "status").read_text()
        forged = self.call(
            "report",
            {
                "job_id": job_ids[1],
                "capability": first_capability,
                "event": "working",
                "summary": "forged cross-job update",
                "report": "This must be rejected.",
            },
            check=False,
        )
        self.assertFalse(forged["ok"])
        self.assertIn("does not authorize", forged["error"])
        self.assertEqual((second_directory / "status").read_text(), prior_status)

        forged_failure = self.call(
            "failure",
            {
                "job_id": job_ids[0],
                "capability": first_capability,
                "summary": "forged trusted failure",
                "report": "This must also be rejected.",
            },
            check=False,
        )
        self.assertFalse(forged_failure["ok"])
        self.assertIn("does not authorize", forged_failure["error"])

        second_capability = self.worker_capability(job_ids[1])
        oversized_utf8 = self.call(
            "report",
            {
                "job_id": job_ids[1],
                "capability": second_capability,
                "event": "working",
                "summary": "oversized UTF-8 detail",
                "report": "é" * (256 * 1024 + 1),
            },
            check=False,
        )
        self.assertFalse(oversized_utf8["ok"])
        self.assertIn("detail is too large", oversized_utf8["error"])

        reporter_env = {
            **self.env,
            "SCUFRIS_JOB_ID": job_ids[1],
            "SCUFRIS_REPORT_CAPABILITY": second_capability,
        }
        adapter = subprocess.run(
            [str(REPORTER), "working", "oversized adapter input"],
            input=b"x" * (512 * 1024 + 1),
            capture_output=True,
            env=reporter_env,
            check=False,
            timeout=30,
        )
        self.assertEqual(adapter.returncode, 2)
        self.assertIn(b"exceeds 512 KiB", adapter.stderr)
        invalid_utf8 = subprocess.run(
            [str(REPORTER), "working", "invalid adapter UTF-8"],
            input=b"\xff",
            capture_output=True,
            env=reporter_env,
            check=False,
            timeout=30,
        )
        self.assertEqual(invalid_utf8.returncode, 2)
        self.assertIn(b"valid UTF-8", invalid_utf8.stderr)

    def test_atomic_report_faults_never_publish_status_without_evidence(self) -> None:
        job_id = "0a1b2c3d4e5f"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Wait for report fault injection.",
                "owner_session": "foreground-session",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")
        jobs_module = load_jobs_module()
        (directory / "report.md").write_bytes(b"x" * jobs_module.MAX_REPORT_FILE)
        environment = {"XDG_STATE_HOME": str(self.root / "state")}
        status_before = (directory / "status").read_bytes()
        report_before = (directory / "report.md").read_bytes()
        with (
            mock.patch.dict(os.environ, environment, clear=False),
            mock.patch.object(
                jobs_module.os, "replace", side_effect=OSError("injected replace fault")
            ),
            self.assertRaises(OSError),
        ):
            jobs_module.write_report(
                job_id, "working", "replace must fail", "new evidence"
            )
        self.assertEqual((directory / "report.md").read_bytes(), report_before)
        self.assertEqual((directory / "status").read_bytes(), status_before)
        self.assertEqual(list(directory.glob(".report-*.tmp")), [])

        original_open = jobs_module.open_job_artifact

        def fail_status(path: Path, flags: int) -> int:
            if path.name == "status" and flags & os.O_WRONLY:
                raise jobs_module.JobError("injected status fault")
            return original_open(path, flags)

        with (
            mock.patch.dict(os.environ, environment, clear=False),
            mock.patch.object(
                jobs_module, "open_job_artifact", side_effect=fail_status
            ),
            self.assertRaises(jobs_module.JobError),
        ):
            jobs_module.write_report(
                job_id,
                "working",
                "durable evidence first",
                "evidence survives status failure",
            )
        self.assertEqual((directory / "status").read_bytes(), status_before)
        self.assertTrue(
            (directory / "report.md")
            .read_text()
            .endswith(
                "# working: durable evidence first\n\n"
                "evidence survives status failure\n"
            )
        )
        self.assertEqual(list(directory.glob(".report-*.tmp")), [])

    def test_project_job_persists_exact_context_snapshot(self) -> None:
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        job_id = "fedcba987654"
        result = self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Inspect the next task.",
                "owner_session": "foreground-session",
                "project": context["project"],
                "project_root": context["project_root"],
                "context_markdown": context["markdown"],
                "context_fingerprint": context["fingerprint"],
                "workspace": "project",
            },
        )["result"]
        self.jobs.append(job_id)
        self.assertEqual(result["workspace"], "project")
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")
        self.assertEqual(
            (directory / "project-context.md").read_text(), context["markdown"]
        )
        record = json.loads((directory / "job.json").read_text())
        self.assertEqual(record["context_fingerprint"], context["fingerprint"])
        self.assertEqual(record["working_directory"], str(self.project))

        review_context = self.call("context", {"project": "projects/nova-protocol"})[
            "result"
        ]
        review_id = "111aaa222bbb"
        review = self.call(
            "spawn",
            {
                "job_id": review_id,
                "instructions": "Review the implementation.",
                "owner_session": "foreground-session",
                "project": review_context["project"],
                "project_root": review_context["project_root"],
                "context_markdown": review_context["markdown"],
                "context_fingerprint": review_context["fingerprint"],
                "review_of": job_id,
            },
        )["result"]
        self.jobs.append(review_id)
        self.assertEqual(review["workspace"], "review")
        self.assertEqual(
            review["review_isolation"],
            {
                "enforcement": "model-tool-allowlist",
                "filesystem": "not-os-sandboxed",
                "tools": ["read", "grep", "find", "ls", "scufris_report"],
                "trusted_boundary": ["harness-executable"],
            },
        )
        review_directory = self.root / "state" / "scufris" / "jobs" / review_id
        self.wait_for(review_directory / "status", "done: report complete")
        self.assertIn(
            "independent read-only reviewer",
            (review_directory / "prompt.md").read_text(),
        )
        argv = json.loads((review_directory / "worker-argv.json").read_text())
        self.assertIn("read,grep,find,ls,scufris_report", argv)
        self.assertIn(
            str(REPOSITORY / "agent/extensions/scufris/workflow/worker-report.ts"),
            argv,
        )
        self.assertFalse((self.project / "PI_MUTATION").exists())
        review_record = json.loads((review_directory / "job.json").read_text())
        self.assertEqual(review_record["review_of"], job_id)
        self.assertEqual(review_record["working_directory"], str(self.project))
        source_record = json.loads((directory / "job.json").read_text())
        self.assertEqual(
            (
                review_record["working_directory"],
                review_record["workspace_device"],
                review_record["workspace_inode"],
            ),
            (
                source_record["working_directory"],
                source_record["workspace_device"],
                source_record["workspace_inode"],
            ),
        )

    def test_claude_review_uses_enforced_read_tools_and_captured_report(self) -> None:
        claude_review = """#!/usr/bin/env python3
import json
import os
import pathlib
import sys
state = pathlib.Path(os.environ['XDG_STATE_HOME'])
directory = state / 'scufris' / 'jobs' / os.environ['SCUFRIS_JOB_ID']
generation = int(os.environ['SCUFRIS_JOB_GENERATION'])
argv = sys.argv[1:]
(directory / 'worker-argv.json').write_text(json.dumps(argv))
(directory / f'worker-argv-g{generation}.json').write_text(json.dumps(argv))
(directory / 'worker-env.json').write_text(json.dumps({
    'report_capability': 'SCUFRIS_REPORT_CAPABILITY' in os.environ,
}))
tools = argv[argv.index('--tools') + 1].split(',')
if {'Bash', 'Edit', 'Write', 'NotebookEdit'} & set(tools):
    (pathlib.Path.cwd() / 'CLAUDE_MUTATION').write_text('unsafe tools exposed\\n')
print(f'# Claude independent review generation {generation}\\n\\nNo findings.')
"""
        (self.bin / "claude").write_text(claude_review)
        (self.bin / "claude").chmod(0o755)
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        source_id = "c1a0de000001"
        common = {
            "owner_session": "claude-review-owner",
            "project": context["project"],
            "project_root": context["project_root"],
            "context_markdown": context["markdown"],
            "context_fingerprint": context["fingerprint"],
        }
        self.call(
            "spawn",
            {
                "job_id": source_id,
                "instructions": "Implement the review fixture.",
                **common,
            },
        )
        self.jobs.append(source_id)
        review_context = self.call("context", {"project": "projects/nova-protocol"})[
            "result"
        ]
        review_id = "c1a0de000002"
        result = self.call(
            "spawn",
            {
                "job_id": review_id,
                "instructions": "Review the implementation for concrete defects.",
                "owner_session": "claude-review-owner",
                "project": review_context["project"],
                "project_root": review_context["project_root"],
                "context_markdown": review_context["markdown"],
                "context_fingerprint": review_context["fingerprint"],
                "review_of": source_id,
                "harness": "claude",
                "model": "opus",
                "thinking": "xhigh",
            },
        )["result"]
        self.jobs.append(review_id)
        self.assertEqual(result["harness"], "claude")
        self.assertEqual(result["model"], "opus")
        self.assertEqual(result["thinking"], "xhigh")
        self.assertEqual(
            result["review_isolation"],
            {
                "enforcement": "model-tool-allowlist",
                "filesystem": "not-os-sandboxed",
                "tools": ["Read", "Glob", "Grep"],
                "trusted_boundary": [
                    "harness-executable",
                    "managed-claude-policy",
                ],
            },
        )
        review_directory = self.root / "state" / "scufris" / "jobs" / review_id
        self.wait_for(
            review_directory / "status",
            "done: independent review complete",
        )
        argv = json.loads((review_directory / "worker-argv.json").read_text())
        self.assertIn("--print", argv)
        self.assertNotIn("--dangerously-skip-permissions", argv)
        self.assertEqual(argv[argv.index("--permission-mode") + 1], "dontAsk")
        self.assertEqual(argv[argv.index("--tools") + 1], "Read,Glob,Grep")
        self.assertEqual(argv[argv.index("--setting-sources") + 1], "")
        self.assertIn("Bash", argv[argv.index("--disallowed-tools") + 1])
        self.assertIn("--session-id", argv)
        worker_env = json.loads((review_directory / "worker-env.json").read_text())
        self.assertFalse(worker_env["report_capability"])
        self.assertFalse((self.project / "CLAUDE_MUTATION").exists())
        prompt = (review_directory / "prompt.md").read_text()
        self.assertIn("Return one concrete Markdown review", prompt)
        self.assertNotIn("Send reports through", prompt)
        inspected = self.call("inspect", {"job_id": review_id, "include_report": True})[
            "result"
        ]
        self.assertEqual(inspected["working_directory"], str(self.project))
        self.assertEqual(inspected["review_isolation"], result["review_isolation"])
        self.assertIn("# Claude independent review generation 1", inspected["report"])

        first_events = self.call("events", {"jobs": [{"job_id": review_id}]})["result"][
            "jobs"
        ][0]["events"]
        for event in first_events:
            self.call("ack-event", {"job_id": review_id, "event_id": event["id"]})
        resumed = self.call(
            "send", {"job_id": review_id, "message": "Recheck the correction."}
        )["result"]
        self.assertEqual(resumed["generation"], 2)
        self.wait_for(
            review_directory / "status",
            '"generation":2,"event":"done","summary":"independent review complete"',
        )
        second_argv = json.loads((review_directory / "worker-argv-g2.json").read_text())
        self.assertIn("--resume", second_argv)
        self.assertNotIn("--session-id", second_argv)
        report = (review_directory / "report.md").read_text()
        self.assertIn("# Claude independent review generation 1", report)
        self.assertIn("# Claude independent review generation 2", report)
        self.assertEqual(report.count("# done: independent review complete"), 2)

        jobs_module = load_jobs_module()
        stale = subprocess.CompletedProcess(
            ["claude"], 0, b"# Stale generation one output\n", b""
        )
        with mock.patch.dict(os.environ, self.env, clear=True):
            self.assertFalse(
                jobs_module.publish_harness_completion(
                    review_id, 1, stale, capture_review=True
                )
            )
        self.assertNotIn(
            "Stale generation one output",
            (review_directory / "report.md").read_text(),
        )
        with (
            mock.patch.dict(os.environ, self.env, clear=True),
            self.assertRaisesRegex(
                jobs_module.JobError,
                "job generation changed before report publication",
            ),
        ):
            jobs_module.write_report(
                review_id,
                "done",
                "stale publication",
                "Must not publish.",
                expected_generation=1,
            )
        self.assertNotIn(
            "Must not publish.", (review_directory / "report.md").read_text()
        )

    def test_adapter_rejects_unsupported_harness_thinking_combinations(self) -> None:
        jobs_module = load_jobs_module()
        cases = (
            ("pi", "reviewer", "max", "Pi does not support max thinking"),
            ("claude", "opus", "minimal", "Claude does not support"),
            ("other", "reviewer", "medium", "harness must be pi or claude"),
        )
        for harness, model, thinking, message in cases:
            with self.assertRaisesRegex(jobs_module.JobError, message):
                jobs_module.selected_adapter(harness, model, thinking)

    def test_review_launches_refuse_replaced_workspace_identity(self) -> None:
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        source_id = "c0ffee000001"
        review_id = "c0ffee000002"
        common = {
            "owner_session": "identity-review-owner",
            "project": context["project"],
            "project_root": context["project_root"],
            "context_markdown": context["markdown"],
            "context_fingerprint": context["fingerprint"],
        }
        self.call(
            "spawn",
            {"job_id": source_id, "instructions": "Own the workspace.", **common},
        )
        self.jobs.append(source_id)
        self.call(
            "spawn",
            {
                "job_id": review_id,
                "instructions": "Review the exact workspace.",
                "review_of": source_id,
                **common,
            },
        )
        self.jobs.append(review_id)
        review_directory = self.root / "state" / "scufris" / "jobs" / review_id
        self.wait_for(review_directory / "status", "done: report complete")
        events = self.call("events", {"jobs": [{"job_id": review_id}]})["result"][
            "jobs"
        ][0]["events"]
        for event in events:
            self.call("ack-event", {"job_id": review_id, "event_id": event["id"]})

        jobs_module = load_jobs_module()
        token = "d" * 64
        with mock.patch.dict(os.environ, self.env, clear=True):
            record = jobs_module.load_job(review_id)
            creating = {
                **record,
                "generation": 2,
                "state": "working",
                "summary": "recovery launch pending",
                "execution_state": "creating",
                "tmux_session_name": jobs_module.execution_session_name(
                    review_id, 2, token
                ),
                "tmux_session_id": None,
                "tmux_window_id": None,
                "tmux_pane_id": None,
                "execution_token": token,
            }
            jobs_module.store_job(creating)
            jobs_module.atomic_write(
                review_directory / ".launch-capability", b"0" * 64, 0o400
            )

        original = self.root / "original-project"
        self.project.rename(original)
        shutil.copytree(original, self.project)
        previous_cwd = Path.cwd()
        try:
            with mock.patch.dict(os.environ, self.env, clear=True):
                with (
                    mock.patch.object(jobs_module, "tmux") as tmux_call,
                    self.assertRaisesRegex(
                        jobs_module.JobError, "workspace identity changed"
                    ),
                ):
                    jobs_module.start_execution(creating, "0" * 64)
                tmux_call.assert_not_called()

                with (
                    mock.patch.object(jobs_module, "tmux") as tmux_call,
                    self.assertRaisesRegex(
                        jobs_module.JobError, "workspace identity changed"
                    ),
                ):
                    jobs_module.finish_precreated_execution(creating, {})
                tmux_call.assert_not_called()

                with (
                    mock.patch.object(
                        jobs_module, "execution_snapshot", return_value=None
                    ),
                    self.assertRaisesRegex(
                        jobs_module.JobError, "workspace identity changed"
                    ),
                ):
                    jobs_module.recover_job(creating)

                os.chdir(self.project)
                with self.assertRaisesRegex(
                    jobs_module.JobError,
                    "execution working directory identity changed",
                ):
                    jobs_module.validate_execution_cwd(creating)
        finally:
            os.chdir(previous_cwd)
            shutil.rmtree(self.project)
            original.rename(self.project)

    def test_a_launch_that_never_reaches_the_harness_says_so(self) -> None:
        job_id = "1a2b3c4d5e60"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Refuse before the harness runs.",
                "owner_session": "launch-failure-owner",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")

        jobs_module = load_jobs_module()
        capability = "e" * 64
        token = "d" * 64
        with mock.patch.dict(os.environ, self.env, clear=True):
            record = jobs_module.load_job(job_id)
            generation = record["generation"] + 1
            jobs_module.store_job(
                {
                    **record,
                    "generation": generation,
                    "state": "working",
                    "summary": "worker starting",
                    "execution_state": "creating",
                    "tmux_session_name": jobs_module.execution_session_name(
                        job_id, generation, token
                    ),
                    "tmux_session_id": None,
                    "tmux_window_id": None,
                    "tmux_pane_id": None,
                    "execution_token": token,
                }
            )
            auth = jobs_module.load_report_auth(job_id)
            auth["generation"] = generation
            auth["launch_capability_hash"] = jobs_module.capability_hash(capability)
            jobs_module.atomic_write(
                directory / ".report-auth.json",
                json.dumps(auth, sort_keys=True).encode(),
                0o400,
            )
            # The pane runs somewhere the workspace no longer is, so the launch
            # refuses before it starts anything.
            with self.assertRaisesRegex(
                jobs_module.JobError, "execution working directory identity changed"
            ):
                jobs_module.launch(job_id, capability)
        self.assertIn(
            '{"generation":2,"event":"failed",'
            '"summary":"worker harness did not start"}',
            (directory / "status").read_text(),
        )
        self.assertIn(
            "execution working directory identity changed",
            (directory / "report.md").read_text(),
        )

    def test_claude_review_creation_recovery_captures_terminal_report(self) -> None:
        claude_review = """#!/usr/bin/env python3
import json
import os
import pathlib
import sys
state = pathlib.Path(os.environ['XDG_STATE_HOME'])
directory = state / 'scufris' / 'jobs' / os.environ['SCUFRIS_JOB_ID']
(directory / 'recovered-argv.json').write_text(json.dumps(sys.argv[1:]))
print('# Recovered Claude review\\n\\nNo findings.')
"""
        (self.bin / "claude").write_text(claude_review)
        (self.bin / "claude").chmod(0o755)
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        source_id = "decade000001"
        review_id = "decade000002"
        common = {
            "owner_session": "claude-recovery-owner",
            "project": context["project"],
            "project_root": context["project_root"],
            "context_markdown": context["markdown"],
            "context_fingerprint": context["fingerprint"],
        }
        self.call(
            "spawn",
            {"job_id": source_id, "instructions": "Own the workspace.", **common},
        )
        self.jobs.append(source_id)

        jobs_module = load_jobs_module()
        request = {
            "job_id": review_id,
            "instructions": "Recover this Claude review creation.",
            "trusted_capability": hashlib.sha256(b"trusted:recovery").hexdigest(),
            "review_of": source_id,
            "harness": "claude",
            "model": "opus",
            "thinking": "xhigh",
            **common,
        }
        original_store = jobs_module.store_job
        injected = False

        def crash_after_server_creation(record: dict[str, Any]) -> None:
            nonlocal injected
            if (
                not injected
                and record.get("execution_state") == "creating"
                and record.get("tmux_session_id") is not None
            ):
                injected = True
                raise OSError("injected Claude review creation crash")
            original_store(record)

        with (
            mock.patch.dict(os.environ, self.env, clear=True),
            mock.patch.object(
                jobs_module, "store_job", side_effect=crash_after_server_creation
            ),
            self.assertRaisesRegex(OSError, "injected Claude review creation crash"),
        ):
            jobs_module.spawn(request)
        self.jobs.append(review_id)

        with mock.patch.dict(os.environ, self.env, clear=True):
            durable = jobs_module.load_job(review_id)
            self.assertEqual(durable["execution_state"], "creating")
            recovered = jobs_module.recover({"owner_session": "claude-recovery-owner"})
            self.assertTrue(
                next(job for job in recovered["jobs"] if job["job_id"] == review_id)[
                    "window_alive"
                ]
            )
        review_directory = self.root / "state" / "scufris" / "jobs" / review_id
        self.wait_for(review_directory / "status", "done: independent review complete")
        argv = json.loads((review_directory / "recovered-argv.json").read_text())
        self.assertIn("--session-id", argv)
        self.assertIn(
            "# Recovered Claude review", (review_directory / "report.md").read_text()
        )

    def test_generation_cursor_is_lossless_and_restart_does_not_replay(self) -> None:
        job_id = "aabbccddeeff"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Publish a terminal fixture.",
                "owner_session": "cursor-owner",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")

        first = self.call("events", {"jobs": [{"job_id": job_id}]})["result"]["jobs"][0]
        replay = self.call("events", {"jobs": [{"job_id": job_id}]})["result"]["jobs"][
            0
        ]
        self.assertEqual(
            [event["id"] for event in first["events"]],
            [event["id"] for event in replay["events"]],
        )
        self.assertEqual([event["generation"] for event in first["events"]], [1, 1])
        self.call("ack-event", {"job_id": job_id, "event_id": first["events"][0]["id"]})
        remaining = self.call("events", {"jobs": [{"job_id": job_id}]})["result"][
            "jobs"
        ][0]
        self.assertEqual(
            [event["line"] for event in remaining["events"]],
            ["done: report complete"],
        )
        self.call(
            "ack-event",
            {"job_id": job_id, "event_id": remaining["events"][0]["id"]},
        )
        self.assertEqual(
            self.call("events", {"jobs": [{"job_id": job_id}]})["result"]["jobs"][0][
                "events"
            ],
            [],
        )

        restarted = self.call(
            "send", {"job_id": job_id, "message": "Run a correction generation."}
        )["result"]
        self.assertTrue(restarted["restarted"])
        self.assertEqual(restarted["generation"], 2)
        self.wait_for(directory / "status", '"generation":2')
        generation_two = self.call("events", {"jobs": [{"job_id": job_id}]})["result"][
            "jobs"
        ][0]["events"]
        self.assertEqual([event["generation"] for event in generation_two], [2, 2])
        self.assertNotEqual(generation_two[0]["id"], first["events"][0]["id"])

    def test_status_replacement_refuses_cursor_drift(self) -> None:
        job_id = "ffeeddccbbaa"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Publish status for replacement test.",
                "owner_session": "status-owner",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")
        status = directory / "status"
        content = status.read_bytes()
        replacement = directory / "replacement-status"
        replacement.write_bytes(content)
        os.replace(replacement, status)
        rejected = self.call("events", {"jobs": [{"job_id": job_id}]}, check=False)
        self.assertFalse(rejected["ok"])
        self.assertIn("status identity changed", rejected["error"])

    def test_terminal_and_recursive_cleanup_preserve_unrelated_resources(self) -> None:
        self.tmux("new-session", "-d", "-s", "unrelated-explicit-session")
        first = "010203040506"
        second = "102030405060"
        for job_id, owner in ((first, "workflow-a"), (second, "workflow-b")):
            self.call(
                "spawn",
                {
                    "job_id": job_id,
                    "instructions": "Wait for isolated cleanup.",
                    "owner_session": owner,
                },
            )
            self.jobs.append(job_id)
        first_directory = self.root / "state" / "scufris" / "jobs" / first
        self.wait_for(first_directory / "status", "done: report complete")
        events = self.call("events", {"jobs": [{"job_id": first}]})["result"]["jobs"][0]
        self.assertTrue(events["events"])
        first_session = json.loads((first_directory / "job.json").read_text())[
            "tmux_session_name"
        ]
        second_directory = self.root / "state" / "scufris" / "jobs" / second
        second_session = json.loads((second_directory / "job.json").read_text())[
            "tmux_session_name"
        ]
        self.assertEqual(
            self.tmux("has-session", "-t", "unrelated-explicit-session").returncode,
            0,
        )
        self.assertTrue(second_directory.is_dir())
        self.call("stop", {"job_id": first})
        self.assert_archived(first)
        self.assertTrue(second_directory.is_dir())
        # Cleanup shares the default server, so it must kill only its own
        # session and leave every unrelated session running.
        self.assertNotEqual(
            self.tmux("has-session", "-t", f"={first_session}", check=False).returncode,
            0,
        )
        for survivor in ("unrelated-explicit-session", f"={second_session}"):
            self.assertEqual(self.tmux("has-session", "-t", survivor).returncode, 0)
        self.tmux("kill-session", "-t", "unrelated-explicit-session")

    def test_a_worker_finds_its_job_under_another_shells_tmux_server(self) -> None:
        # A tmux session takes the server's environment, and the server
        # belongs to whoever started it first. On a developer's machine that
        # is a login shell, so a staging run beside one used to launch its
        # workers into the developer's own answer to where jobs are kept.
        stray = dict(self.env)
        stray["XDG_STATE_HOME"] = str(self.root / "elsewhere")
        subprocess.run(
            ["tmux", "new-session", "-d", "-s", "another-shell"],
            env=stray,
            check=True,
            capture_output=True,
            timeout=10,
        )
        self.addCleanup(self.tmux, "kill-session", "-t", "another-shell", check=False)
        job_id = "5c0f1a2b3d4e"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Find your own record.",
                "owner_session": "stray-server-owner",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        # Without the session's own environment the worker dies on the first
        # artifact it looks for, and this file is never written.
        self.wait_for(directory / "status", "done: report complete")
        self.assertFalse((self.root / "elsewhere").exists())

    def test_atomic_tmux_ownership_mismatch_refuses_termination(self) -> None:
        job_id = "abcdefabcdef"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Wait for ownership validation.",
                "owner_session": "ownership-owner",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")
        record = json.loads((directory / "job.json").read_text())
        self.tmux(
            "set-option",
            "-t",
            record["tmux_session_name"],
            "@scufris_execution_token",
            "0" * 64,
        )
        rejected = self.call("events", {"jobs": [{"job_id": job_id}]}, check=False)
        self.assertFalse(rejected["ok"])
        self.assertIn("ownership mismatch", rejected["error"])
        self.assertEqual(
            self.tmux("has-session", "-t", record["tmux_session_name"]).returncode,
            0,
        )
        self.tmux(
            "set-option",
            "-t",
            record["tmux_session_name"],
            "@scufris_execution_token",
            record["execution_token"],
        )
        accepted = self.call("events", {"jobs": [{"job_id": job_id}]})["result"]
        self.assertTrue(accepted["jobs"][0]["events"])
        self.assertNotEqual(
            self.tmux(
                "has-session", "-t", record["tmux_session_name"], check=False
            ).returncode,
            0,
        )

    def test_atomic_server_revalidation_closes_the_check_kill_race(self) -> None:
        job_id = "0011aabbccdd"
        token = "a" * 64
        directory = self.fixture_job(
            job_id,
            {
                "execution_state": "running",
                "tmux_session_name": f"scufris-{job_id}-g1-{token[:16]}",
                "tmux_session_id": "$91",
                "tmux_window_id": "@92",
                "tmux_pane_id": "%93",
                "execution_token": token,
            },
        )
        jobs_module = load_jobs_module()
        with mock.patch.dict(os.environ, self.env, clear=True):
            job = jobs_module.load_job(job_id)
            snapshot = {
                "name": job["tmux_session_name"],
                "session_id": job["tmux_session_id"],
                "window_id": job["tmux_window_id"],
                "pane_id": job["tmux_pane_id"],
                "job_id": job_id,
                "token": token,
                "generation": "1",
                "phase": "running",
                "pane_dead": "0",
            }
            replacement = subprocess.CompletedProcess(
                ["tmux"], 0, b"scufris-ownership-mismatch\n", b""
            )
            with (
                mock.patch.object(
                    jobs_module, "tmux", return_value=replacement
                ) as call,
                self.assertRaises(jobs_module.JobError),
            ):
                jobs_module.stop_execution(job, snapshot)
        arguments = call.call_args.args[0]
        self.assertEqual(arguments[0], "if-shell")
        self.assertIn("kill-session", arguments[-2])
        self.assertIn(token, arguments[-3])
        self.assertTrue(directory.is_dir())
        unchanged = json.loads((directory / "job.json").read_text())
        self.assertEqual(unchanged["execution_token"], token)

    def test_creation_crash_is_recovered_from_durable_intent(self) -> None:
        jobs_module = load_jobs_module()
        job_id = "112233445566"
        request = {
            "job_id": job_id,
            "instructions": "Recover creation.",
            "owner_session": "crash-owner",
            "trusted_capability": hashlib.sha256(b"trusted:crash").hexdigest(),
        }
        original_store = jobs_module.store_job
        injected = False

        def crash_after_server_creation(record: dict[str, Any]) -> None:
            nonlocal injected
            if (
                not injected
                and record.get("execution_state") == "creating"
                and record.get("tmux_session_id") is not None
            ):
                injected = True
                raise OSError("injected crash after tmux creation")
            original_store(record)

        with (
            mock.patch.dict(os.environ, self.env, clear=True),
            mock.patch.object(
                jobs_module, "store_job", side_effect=crash_after_server_creation
            ),
            self.assertRaises(OSError),
        ):
            jobs_module.spawn(request)
        self.jobs.append(job_id)
        with mock.patch.dict(os.environ, self.env, clear=True):
            durable = jobs_module.load_job(job_id)
            self.assertEqual(durable["execution_state"], "creating")
            self.assertIsNone(durable["tmux_session_id"])
            recovered = jobs_module.recover({"owner_session": "crash-owner"})
            self.assertEqual(recovered["jobs"][0]["job_id"], job_id)
            self.assertTrue(recovered["jobs"][0]["window_alive"])
            self.assertEqual(jobs_module.load_job(job_id)["execution_state"], "running")

    def test_a_pane_from_another_session_is_named_with_its_tmux_session(self) -> None:
        job_id = "5a5b5c5d5e5f"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Outlive the session that started this.",
                "owner_session": "the-previous-session",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")

        jobs_module = load_jobs_module()
        with mock.patch.dict(os.environ, self.env, clear=True):
            # The session that started it never came back, so its pane is
            # nobody's. Nothing here can stop it, so what the answer is worth is
            # the name a person can reach it by.
            stray = jobs_module.orphans({"owner_session": "the-session-now"})
            self.assertEqual(stray["job_ids"], [job_id])
            named = stray["jobs"][0]
            self.assertEqual(named["job_id"], job_id)
            self.assertEqual(
                named["tmux_session"],
                jobs_module.load_job(job_id)["tmux_session_name"],
            )
            self.assertTrue(named["tmux_session"])

            # And its own session sees nothing stray, because `recover` is what
            # answers for the jobs it owns.
            self.assertEqual(
                jobs_module.orphans({"owner_session": "the-previous-session"}),
                {"job_ids": [], "jobs": []},
            )

    def test_partial_descendant_failure_retains_root_for_retry(self) -> None:
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        root_id = "223344556677"
        review_id = "334455667788"
        self.call(
            "spawn",
            {
                "job_id": root_id,
                "instructions": "Own the workflow root.",
                "owner_session": "partial-owner",
                "project": context["project"],
                "project_root": context["project_root"],
                "context_markdown": context["markdown"],
                "context_fingerprint": context["fingerprint"],
                "workspace": "project",
            },
        )
        self.jobs.append(root_id)
        review_context = self.call("context", {"project": "projects/nova-protocol"})[
            "result"
        ]
        self.call(
            "spawn",
            {
                "job_id": review_id,
                "instructions": "Review the workflow root.",
                "owner_session": "partial-owner",
                "project": review_context["project"],
                "project_root": review_context["project_root"],
                "context_markdown": review_context["markdown"],
                "context_fingerprint": review_context["fingerprint"],
                "review_of": root_id,
            },
        )
        jobs_module = load_jobs_module()
        original_archive = jobs_module.archive_job

        def fail_descendant(record: dict[str, Any], archived_at: str) -> None:
            if record["job_id"] == review_id:
                raise OSError("injected descendant archive failure")
            original_archive(record, archived_at)

        with (
            mock.patch.dict(os.environ, self.env, clear=True),
            mock.patch.object(jobs_module, "archive_job", side_effect=fail_descendant),
            self.assertRaises(jobs_module.JobError),
        ):
            jobs_module.stop({"job_id": root_id})
        jobs = self.root / "state" / "scufris" / "jobs"
        self.assertTrue((jobs / root_id).is_dir())
        self.assertTrue((jobs / review_id).is_dir())
        self.assertIsNone(
            json.loads((jobs / root_id / "job.json").read_text())["archived_at"]
        )
        retried = self.call("stop", {"job_id": root_id})["result"]
        self.assertTrue(retried["clean"])
        self.assert_archived(root_id)
        self.assert_archived(review_id)
        self.assertTrue(self.call("stop", {"job_id": root_id})["result"]["clean"])

    def test_reviewer_descendants_share_one_recursive_graph(self) -> None:
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        ids = ["445566778899", "556677889900", "667788990011"]
        self.call(
            "spawn",
            {
                "job_id": ids[0],
                "instructions": "Implement the graph root.",
                "owner_session": "graph-owner",
                "project": context["project"],
                "project_root": context["project_root"],
                "context_markdown": context["markdown"],
                "context_fingerprint": context["fingerprint"],
                "workspace": "project",
            },
        )
        self.jobs.append(ids[0])
        for job_id, parent in zip(ids[1:], ids, strict=False):
            child_context = self.call("context", {"project": "projects/nova-protocol"})[
                "result"
            ]
            self.call(
                "spawn",
                {
                    "job_id": job_id,
                    "instructions": "Review the parent job read-only.",
                    "owner_session": "graph-owner",
                    "project": child_context["project"],
                    "project_root": child_context["project_root"],
                    "context_markdown": child_context["markdown"],
                    "context_fingerprint": child_context["fingerprint"],
                    "review_of": parent,
                },
            )
        records = [
            json.loads(
                (
                    self.root / "state" / "scufris" / "jobs" / job_id / "job.json"
                ).read_text()
            )
            for job_id in ids
        ]
        self.assertTrue(all(record["root_job"] == ids[0] for record in records))
        self.assertEqual(
            [record["parent_job"] for record in records], [None, ids[0], ids[1]]
        )
        self.assertEqual(len({record["workflow_id"] for record in records}), 1)
        # A descendant ID must not escalate into stopping its parents.
        refused = self.call("stop", {"job_id": ids[2]}, check=False)
        self.assertFalse(refused["ok"])
        self.assertIn(ids[0], refused["error"])
        self.assertTrue((self.root / "state" / "scufris" / "jobs" / ids[0]).is_dir())
        cleaned = self.call("stop", {"job_id": ids[0]})["result"]
        self.assertEqual(set(cleaned["removed_jobs"]), set(ids))
        self.assertEqual(json.loads(self.cli("all", "--json").stdout), {"jobs": []})
        archived = json.loads(self.cli("all", "--archived", "--json").stdout)["jobs"]
        self.assertEqual({job["job_id"] for job in archived}, set(ids))

    def test_restart_crash_before_tmux_creation_rotates_generation_safely(self) -> None:
        job_id = "789900112233"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Complete before restart.",
                "owner_session": "restart-owner",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")
        events = self.call("events", {"jobs": [{"job_id": job_id}]})["result"]["jobs"][
            0
        ]["events"]
        for event in events:
            self.call("ack-event", {"job_id": job_id, "event_id": event["id"]})

        jobs_module = load_jobs_module()
        with mock.patch.dict(os.environ, self.env, clear=True):
            previous = jobs_module.load_job(job_id)
            restarted = {
                **previous,
                "generation": previous["generation"] + 1,
                "state": "working",
                "summary": "restart intent persisted",
            }
            jobs_module.store_job(restarted)
            prepared, _capability = jobs_module.prepare_execution(restarted)
            self.assertEqual(prepared["execution_state"], "creating")
            self.assertIsNone(jobs_module.execution_snapshot(prepared))
            recovered = jobs_module.recover({"owner_session": "restart-owner"})
            self.assertEqual(recovered["jobs"][0]["generation"], 2)
            self.assertTrue(recovered["jobs"][0]["window_alive"])
        self.wait_for(directory / "status", '"generation":2')
        new_events = self.call("events", {"jobs": [{"job_id": job_id}]})["result"][
            "jobs"
        ][0]["events"]
        self.assertTrue(new_events)
        self.assertTrue(all(event["generation"] == 2 for event in new_events))

    def test_blocked_ends_the_execution_and_steering_restores_the_session(self) -> None:
        blocked_pi = """#!/usr/bin/env python3
import json
import os
import pathlib
import sys
state = pathlib.Path(os.environ['XDG_STATE_HOME'])
directory = state / 'scufris' / 'jobs' / os.environ['SCUFRIS_JOB_ID']
generation = int(os.environ['SCUFRIS_JOB_GENERATION'])
(directory / f'argv-g{generation}.json').write_text(json.dumps(sys.argv[1:]))
event = 'blocked' if generation == 1 else 'done'
with (directory / 'status').open('a') as stream:
    for name, summary in [('working', 'starting blocked fixture'), (event, 'needs one decision')]:
        stream.write(json.dumps({'generation': generation, 'event': name, 'summary': summary}, separators=(',', ':')) + '\\n')
"""
        (self.bin / "pi").write_text(blocked_pi)
        (self.bin / "pi").chmod(0o755)
        job_id = "890011223344"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Block for mediation.",
                "owner_session": "blocked-owner",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "blocked: needs one decision")
        session = json.loads((directory / "job.json").read_text())["harness_session"]
        first_argv = json.loads((directory / "argv-g1.json").read_text())
        self.assertIn("--session-id", first_argv)
        self.assertIn(session, first_argv)
        events = self.call("events", {"jobs": [{"job_id": job_id}]})["result"]["jobs"][
            0
        ]["events"]
        self.assertEqual(events[-1]["line"], "blocked: needs one decision")
        for event in events:
            self.call("ack-event", {"job_id": job_id, "event_id": event["id"]})
        # blocked is terminal: the execution is released and the pane is gone.
        record = json.loads((directory / "job.json").read_text())
        self.assertIsNone(record["execution_state"])
        self.assertIsNone(record["tmux_session_name"])
        recovered = self.call("recover", {"owner_session": "blocked-owner"})["result"][
            "jobs"
        ][0]
        self.trusted_capabilities[job_id] = recovered["trusted_capability"]
        self.assertFalse(recovered["window_alive"])
        sent = self.call(
            "send", {"job_id": job_id, "message": "Use the safe default."}
        )["result"]
        self.assertTrue(sent["restarted"])
        self.assertEqual(sent["generation"], 2)
        self.wait_for(directory / "status", '"generation":2')
        # The restored generation reuses the same pinned harness session.
        second_argv = json.loads((directory / "argv-g2.json").read_text())
        self.assertIn(session, second_argv)
        self.assertIn(
            "Use the safe default.", (directory / "conversation.md").read_text()
        )

    @unittest.skipUnless(SPROUT, "sprout is not installed")
    def test_project_configuration_drift_refuses_sprout_cleanup(self) -> None:
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        job_id = "901122334455"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Create a drift-sensitive Sprout.",
                "owner_session": "drift-owner",
                "project": context["project"],
                "project_root": context["project_root"],
                "context_markdown": context["markdown"],
                "context_fingerprint": context["fingerprint"],
                "workspace": "sprout",
                "feature": f"drift-sensitive-{self.run_token}",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        alternate_projects = self.root / "alternate" / "projects"
        alternate_projects.mkdir(parents=True)
        shutil.copytree(self.project, alternate_projects / "nova-protocol")
        original_roots = self.env["SCUFRIS_PROJECT_ROOTS"]
        self.env["SCUFRIS_PROJECT_ROOTS"] = json.dumps([str(alternate_projects)])
        rejected = self.call(
            "stop", {"job_id": job_id, "remove_workspace": True}, check=False
        )
        self.assertFalse(rejected["ok"])
        self.assertIn("drifted", rejected["error"])
        self.assertTrue(directory.is_dir())
        self.env["SCUFRIS_PROJECT_ROOTS"] = original_roots
        cleaned = self.call("stop", {"job_id": job_id, "remove_workspace": True})[
            "result"
        ]
        self.assertTrue(cleaned["clean"])
        self.assert_archived(job_id)

    @unittest.skipIf(SPROUT is None, "sprout is not installed")
    def test_a_refused_landing_leaves_the_workflow_landable_and_stoppable(self) -> None:
        # The land intent is durable so an interrupted merge can only be
        # finished, never restarted with different words. Writing it before the
        # Sprout guard ran meant an ordinary refusal - Alex having edits on
        # master - left a workflow that could not be landed with a different
        # subject, could not be stopped at all, and had no exit but editing the
        # record by hand.
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        job_id = "aa1122334455"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Make a change worth landing.",
                "owner_session": "refused-land-owner",
                "project": context["project"],
                "project_root": context["project_root"],
                "context_markdown": context["markdown"],
                "context_fingerprint": context["fingerprint"],
                "workspace": "sprout",
                "feature": f"refused-landing-{self.run_token}",
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")
        worktree = Path(
            json.loads((directory / "job.json").read_text())["working_directory"]
        )
        (worktree / "RESULT.md").write_text("worth landing\n")
        subprocess.run(["git", "add", "RESULT.md"], cwd=worktree, check=True)
        subprocess.run(
            ["git", "commit", "-m", "Add result"],
            cwd=worktree,
            check=True,
            capture_output=True,
        )
        # What `sprout land --dry-run` refuses: the main checkout is dirty.
        readme = self.project / "README.md"
        kept = readme.read_text()
        readme.write_text(kept + "Alex was in the middle of something.\n")
        refused = self.call(
            "land", {"job_id": job_id, "subject": "Land the change"}, check=False
        )
        self.assertFalse(refused["ok"])
        record = json.loads((directory / "job.json").read_text())
        self.assertIsNone(record["cleanup"], "a refused landing recorded an intent")
        # Every exit is still open: different words, and stopping instead.
        again = self.call(
            "land",
            {"job_id": job_id, "subject": "Different words entirely"},
            check=False,
        )
        self.assertFalse(again["ok"])
        self.assertNotIn("already durable", again["error"])
        readme.write_text(kept)
        # The unmerged branch is refused, which leaves a durable stop intent.
        # Re-deciding after seeing why is the intended next step: a stop that
        # could only be retried with the words that already failed would be a
        # workflow with no exit at all.
        kept_branch = self.call(
            "stop", {"job_id": job_id, "remove_workspace": True}, check=False
        )
        self.assertFalse(kept_branch["ok"])
        self.assertIn("not merged", kept_branch["error"])
        stopped = self.call(
            "stop", {"job_id": job_id, "remove_workspace": True, "abandon": True}
        )["result"]
        self.assertEqual(stopped["state"], "stopped")

    def test_workers_share_the_default_server_and_never_kill_it(self) -> None:
        jobs_module = load_jobs_module()
        with mock.patch.dict(os.environ, self.env, clear=True):
            with mock.patch.object(jobs_module, "run") as call:
                call.return_value = subprocess.CompletedProcess(["tmux"], 0, b"", b"")
                jobs_module.tmux(["list-sessions"])
            self.assertEqual(call.call_args.args[0], ["tmux", "list-sessions"])
            for argv in (
                ["kill-server"],
                ["if-shell", "-F", "-t", "=other:", "1", "kill-server"],
                ["-S", "/tmp/other.sock", "list-sessions"],
                ["-L", "other", "list-sessions"],
            ):
                with self.assertRaises(jobs_module.JobError):
                    jobs_module.tmux(argv)

    def test_stop_refuses_a_session_outside_the_worker_namespace(self) -> None:
        job_id = "aa11bb22cc33"
        token = "b" * 64
        self.fixture_job(
            job_id,
            {
                "execution_state": "running",
                "tmux_session_name": f"scufris-{job_id}-g1-{token[:16]}",
                "tmux_session_id": "$81",
                "tmux_window_id": "@82",
                "tmux_pane_id": "%83",
                "execution_token": token,
            },
        )
        jobs_module = load_jobs_module()
        with mock.patch.dict(os.environ, self.env, clear=True):
            job = jobs_module.load_job(job_id)
            hijacked = {**job, "tmux_session_name": "scufris2"}
            snapshot = {
                "name": hijacked["tmux_session_name"],
                "session_id": job["tmux_session_id"],
                "window_id": job["tmux_window_id"],
                "pane_id": job["tmux_pane_id"],
                "job_id": job_id,
                "token": token,
                "generation": "1",
                "phase": "running",
                "pane_dead": "0",
            }
            with (
                mock.patch.object(jobs_module, "tmux") as call,
                self.assertRaises(jobs_module.JobError),
            ):
                jobs_module.stop_execution(hijacked, snapshot)
            call.assert_not_called()

    @unittest.skipUnless(SPROUT, "sprout is not installed")
    def test_sprout_job_has_explicit_review_target_and_guarded_landing(self) -> None:
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        job_id = "999aaa888bbb"
        result = self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Implement the fixture change.",
                "owner_session": "foreground-session",
                "project": context["project"],
                "project_root": context["project_root"],
                "context_markdown": context["markdown"],
                "context_fingerprint": context["fingerprint"],
                "workspace": "sprout",
                "feature": f"fixture-{job_id}-{self.run_token}",
            },
        )["result"]
        self.jobs.append(job_id)
        self.assertEqual(result["workspace"], "sprout")
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")
        record = json.loads((directory / "job.json").read_text())
        worktree = Path(record["working_directory"])
        target = self.call("review-target", {"job_id": job_id})["result"]
        self.assertEqual(target["cwd"], str(worktree))
        self.assertEqual(target["default_branch"], "master")

        (worktree / "RESULT.md").write_text("replacement works\n")
        subprocess.run(["git", "add", "RESULT.md"], cwd=worktree, check=True)
        subprocess.run(
            ["git", "commit", "-m", "Add result"],
            cwd=worktree,
            check=True,
            capture_output=True,
        )

        quick_target = self.call("quick-review-target", {"job_id": job_id})["result"]
        self.assertEqual(quick_target["cwd"], str(worktree))
        self.assertEqual(
            quick_target["base_revision"],
            subprocess.run(
                ["git", "rev-parse", "refs/heads/master"],
                cwd=worktree,
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip(),
        )
        self.assertEqual(
            quick_target["revision"],
            subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=worktree,
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip(),
        )
        self.assertEqual(
            quick_target["state_dir"], str(directory / "quick-review-agent")
        )

        review_context = self.call("context", {"project": "projects/nova-protocol"})[
            "result"
        ]
        reviewer_id = "aaabbbcccddd"
        self.call(
            "spawn",
            {
                "job_id": reviewer_id,
                "instructions": "Review the exact committed Sprout.",
                "owner_session": "foreground-session",
                "project": review_context["project"],
                "project_root": review_context["project_root"],
                "context_markdown": review_context["markdown"],
                "context_fingerprint": review_context["fingerprint"],
                "review_of": job_id,
            },
        )
        reviewer_directory = self.root / "state" / "scufris" / "jobs" / reviewer_id
        self.assertTrue(reviewer_directory.is_dir())

        landed = self.call(
            "land",
            {
                "job_id": job_id,
                "subject": "Land fixture result",
                "remove_workspace": True,
            },
        )["result"]
        self.assertTrue(landed["landed"])
        self.assertTrue(landed["workspace_removed"])
        self.assertIs(landed["receipt"]["facts"]["landed"], True)
        self.assertEqual(
            landed["receipt"]["commit"], self.git("rev-parse", "refs/heads/master")
        )
        self.assertNotIn("not landed", landed["receipt"]["sentences"])
        self.assertEqual(set(landed["removed_jobs"]), {job_id, reviewer_id})
        self.assert_archived(reviewer_id)
        self.assert_archived(job_id)
        self.assertEqual(
            (self.project / "RESULT.md").read_text(), "replacement works\n"
        )
        self.assertFalse(worktree.exists())

    @unittest.skipUnless(SPROUT, "sprout is not installed")
    def test_manual_squash_and_official_noop_both_report_landed(self) -> None:
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        job_id = "aa0000000013"
        feature = f"manual-squash-{self.run_token}"
        self.call(
            "spawn",
            {
                "job_id": job_id,
                "instructions": "Prepare the manually landed fixture.",
                "owner_session": "manual-squash-owner",
                "project": context["project"],
                "project_root": context["project_root"],
                "context_markdown": context["markdown"],
                "context_fingerprint": context["fingerprint"],
                "workspace": "sprout",
                "feature": feature,
            },
        )
        self.jobs.append(job_id)
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")
        record = json.loads((directory / "job.json").read_text())
        worktree = Path(record["working_directory"])
        (worktree / "RESULT.md").write_text("manual squash\n")
        self.git("add", "RESULT.md", cwd=worktree)
        self.git("commit", "-q", "-m", "feature identity", cwd=worktree)
        source_revision = self.git("rev-parse", "HEAD", cwd=worktree)

        subprocess.run(
            ["sprout", "land", feature, "-m", "Manual squash landing"],
            cwd=self.project,
            env=self.env,
            check=True,
            capture_output=True,
        )
        landed_revision = self.git("rev-parse", "master")
        self.assertNotEqual(source_revision, landed_revision)
        self.assertEqual(
            self.git("rev-parse", f"{source_revision}^{{tree}}"),
            self.git("rev-parse", f"{landed_revision}^{{tree}}"),
        )
        (directory / "report.md").write_text(
            "# done: Synced, re-verified, and landed the fix on master.\n"
        )

        done_receipt = self.call("receipt", {"job_id": job_id, "trigger": "done"})[
            "result"
        ]
        self.assertIs(done_receipt["facts"]["landed"], True)
        self.assertEqual(done_receipt["commit"], landed_revision)
        self.assertEqual(done_receipt["claims"][0]["verdict"], "verified")
        self.assertEqual(done_receipt["sentences"], [])

        result = self.call(
            "land",
            {
                "job_id": job_id,
                "subject": "Manual squash landing",
                "remove_workspace": True,
            },
        )["result"]
        self.assertTrue(result["landed"])
        self.assertIs(result["receipt"]["facts"]["landed"], True)
        self.assertEqual(result["receipt"]["commit"], landed_revision)
        self.assertEqual(result["receipt"]["claims"][0]["verdict"], "verified")
        self.assertEqual(result["receipt"]["sentences"], [])
        archived = self.assert_archived(job_id)
        self.assertEqual(archived["cleanup"]["revision"], landed_revision)
        self.assertFalse(worktree.exists())

    # ------------------------------------------------------------------
    # Receipts. A completion claim is checked, not believed, so every test
    # here asserts the difference between a measured fact and an absent one.
    # ------------------------------------------------------------------

    def git(self, *arguments: str, cwd: Path | None = None) -> str:
        result = subprocess.run(
            ["git", *arguments],
            cwd=cwd or self.project,
            text=True,
            capture_output=True,
            check=True,
        )
        return result.stdout.strip()

    def with_origin(self) -> Path:
        """Gives the fixture project a real remote it can fetch from."""
        upstream = self.root / "upstream.git"
        subprocess.run(
            ["git", "init", "-q", "-b", "master", "--bare", str(upstream)],
            check=True,
            capture_output=True,
        )
        self.git("remote", "add", "origin", str(upstream))
        self.git("push", "-q", "origin", "master")
        return upstream

    def fake_program(self, name: str, script: str) -> Path:
        """Installs one program ahead of the real one for this test only."""
        directory = self.root / "shims"
        directory.mkdir(exist_ok=True)
        executable = directory / name
        executable.write_text(script)
        executable.chmod(0o755)
        self.env["PATH"] = f"{directory}:{self.env['PATH']}"
        return executable

    def project_job(self, job_id: str, report: str = "") -> Path:
        directory = self.fixture_job(
            job_id,
            {
                "project": "projects/nova-protocol",
                "project_root": str(self.project),
                "project_root_device": self.project.stat().st_dev,
                "project_root_inode": self.project.stat().st_ino,
                "context_fingerprint": hashlib.sha256(b"context").hexdigest(),
                "workspace": "project",
                "working_directory": str(self.project),
                "landing_branch": "master",
            },
        )
        if report:
            (directory / "report.md").write_text(report)
        return directory

    def sprout_job(
        self, job_id: str, feature: str, working_directory: Path | None = None
    ) -> Path:
        working = working_directory or self.project
        return self.fixture_job(
            job_id,
            {
                "project": "projects/nova-protocol",
                "project_root": str(self.project),
                "project_root_device": self.project.stat().st_dev,
                "project_root_inode": self.project.stat().st_ino,
                "context_fingerprint": hashlib.sha256(b"context").hexdigest(),
                "workspace": "sprout",
                "feature": feature,
                "working_directory": str(working),
                "workspace_device": working.stat().st_dev,
                "workspace_inode": working.stat().st_ino,
                "landing_branch": "master",
            },
        )

    def feature_worktree(self, feature: str) -> Path:
        working = self.root / f"worktree-{feature}"
        self.git("worktree", "add", "-q", "-b", feature, str(working), "master")
        return working

    def merged_feature_commit(self) -> str:
        self.git("checkout", "-q", "-b", "feature-work")
        (self.project / "RESULT.md").write_text("feature work\n")
        self.git("add", "RESULT.md")
        self.git("commit", "-q", "-m", "feature work")
        self.git("checkout", "-q", "master")
        self.git("merge", "-q", "--ff", "feature-work")
        return self.git("rev-parse", "HEAD")

    def test_receipt_contradicts_a_worker_claim_of_push_and_release(self) -> None:
        self.with_origin()
        commit = self.merged_feature_commit()
        self.fake_program("gh", "#!/bin/sh\nprintf '[]\\n'\n")
        job_id = "aa0000000001"
        self.project_job(
            job_id,
            "# done: shipped\n\nI landed the change, pushed and released it.\n",
        )
        value = self.call("receipt", {"job_id": job_id})["result"]

        facts = value["facts"]
        self.assertEqual(value["commit"], commit)
        # Landed is true and measured; pushed is false and measured. The
        # difference between the two is the whole point of the receipt.
        self.assertIs(facts["landed"], True)
        self.assertIs(facts["pushed"], False)
        self.assertIsNone(facts["release_run"])
        self.assertIs(facts["release"], False)
        self.assertEqual(facts["ahead"], 1)
        self.assertEqual(facts["behind"], 0)
        self.assertEqual(facts["remote"], "origin")

        verdicts = {claim["claim"]: claim["verdict"] for claim in value["claims"]}
        self.assertEqual(verdicts["pushed"], "claimed, not verified")
        self.assertEqual(verdicts["released"], "claimed, not verified")
        self.assertEqual(verdicts["landed"], "verified")
        self.assertIn("pushed: claimed, not verified", value["sentences"])
        self.assertNotIn("landed: claimed, not verified", value["sentences"])

        printed = self.cli(job_id).stdout
        self.assertIn("claimed, not verified", printed)
        self.assertIn("pushed: false", printed)

    def test_receipt_says_not_landed_without_treating_negation_as_a_claim(
        self,
    ) -> None:
        self.with_origin()
        self.git("checkout", "-q", "-b", "feature-unlanded")
        (self.project / "RESULT.md").write_text("unlanded\n")
        self.git("add", "RESULT.md")
        self.git("commit", "-q", "-m", "unlanded work")
        self.fake_program("gh", "#!/bin/sh\nprintf '[]\\n'\n")
        job_id = "aa0000000002"
        self.project_job(
            job_id,
            "# done: Fixed and verified the change; not landed or released.\n",
        )
        value = self.call("receipt", {"job_id": job_id})["result"]
        self.assertIs(value["facts"]["landed"], False)
        self.assertIsNone(value["facts"]["landed_revision"])
        self.assertEqual(value["claims"], [])
        self.assertEqual(value["sentences"], ["not landed"])
        self.assertIn("not landed", self.cli(job_id).stdout)

    def test_direct_master_release_uses_the_first_equivalent_base_revision(
        self,
    ) -> None:
        self.with_origin()
        source = self.feature_worktree("release-work")
        (source / "VERSION").write_text("9.9.9\n")
        self.git("add", "VERSION", cwd=source)
        self.git("commit", "-q", "-m", "prepare release", cwd=source)
        source_revision = self.git("rev-parse", "HEAD", cwd=source)

        # The release was committed directly on master with a different commit
        # identity, then master gained an unrelated task-close commit.
        (self.project / "VERSION").write_text("9.9.9\n")
        self.git("add", "VERSION")
        self.git("commit", "-q", "-m", "Release 9.9.9")
        release_revision = self.git("rev-parse", "HEAD")
        self.git("tag", "-a", "v9.9.9", "-m", "v9.9.9", release_revision)
        (self.project / "CLOSED.md").write_text("closed\n")
        self.git("add", "CLOSED.md")
        self.git("commit", "-q", "-m", "close release task")
        self.git("push", "-q", "origin", "master", "refs/tags/v9.9.9")

        self.fake_program(
            "gh",
            "#!/bin/sh\n"
            'if [ "$1 $2" = "release list" ]; then\n'
            "  printf '%s\\n' "
            '\'[{"isDraft":false,"isPrerelease":false,'
            '"publishedAt":"2026-09-10T18:47:53Z",'
            '"tagName":"v9.9.9","url":"https://release.invalid"}]\'\n'
            "else\n"
            '  printf \'[{"status":"completed",'
            '"conclusion":"success","url":"https://run.invalid"}]\\n\'\n'
            "fi\n",
        )
        job_id = "aa0000000012"
        directory = self.sprout_job(job_id, "release-work", source)
        (directory / "report.md").write_text(
            "# done: Released Scufris v9.9.9; all release checks passed.\n\n"
            "The task-close commit was pushed. Fixed an issue in the landed "
            "safeguards.\n"
        )
        value = self.call("receipt", {"job_id": job_id})["result"]

        self.assertNotEqual(source_revision, release_revision)
        self.assertEqual(
            self.git("rev-parse", f"{source_revision}^{{tree}}"),
            self.git("rev-parse", f"{release_revision}^{{tree}}"),
        )
        self.assertIs(value["facts"]["landed"], True)
        self.assertEqual(value["facts"]["landed_revision"], release_revision)
        self.assertEqual(value["commit"], release_revision)
        self.assertIs(value["facts"]["pushed"], True)
        self.assertEqual(value["facts"]["tags_remote"], ["v9.9.9"])
        self.assertEqual(value["facts"]["release"]["tagName"], "v9.9.9")
        verdicts = {claim["claim"]: claim["verdict"] for claim in value["claims"]}
        self.assertEqual(verdicts, {"pushed": "verified", "released": "verified"})
        self.assertEqual(value["sentences"], [])

    def test_unmeasurable_facts_are_null_with_a_reason_never_false(self) -> None:
        # An unreachable remote must never read as "not pushed". That mistake
        # would be the exact lie receipts exist to prevent.
        self.git("remote", "add", "origin", str(self.root / "missing.git"))
        job_id = "aa0000000003"
        self.project_job(job_id, "# done: shipped\n\nI pushed the branch.\n")
        value = self.call("receipt", {"job_id": job_id})["result"]
        for name in ("pushed", "ahead", "behind", "release_run"):
            self.assertIsNone(value["facts"][name])
            self.assertIn(name, value["unavailable"])
        self.assertIsNot(value["facts"]["pushed"], False)
        # A git fact that needs no remote is still measured.
        self.assertIsNotNone(value["facts"]["head"])
        self.assertIs(value["facts"]["dirty"], False)
        claim = next(item for item in value["claims"] if item["claim"] == "pushed")
        self.assertEqual(claim["verdict"], "claimed, not verified")
        self.assertIsNotNone(claim["reason"])

    def test_receipt_survives_an_unusable_gh_without_losing_git_facts(self) -> None:
        self.with_origin()
        commit = self.merged_feature_commit()
        self.git("tag", "release-candidate", commit)
        self.git("push", "-q", "origin", "refs/tags/release-candidate")
        self.fake_program("gh", "#!/bin/sh\necho 'gh: not authenticated' >&2\nexit 1\n")
        job_id = "aa0000000004"
        self.project_job(job_id, "# done: shipped\n\nI released it.\n")
        value = self.call("receipt", {"job_id": job_id})["result"]
        self.assertIsNone(value["facts"]["release_run"])
        self.assertIn("not authenticated", value["unavailable"]["release_run"])
        self.assertIsNone(value["facts"]["release"])
        self.assertIn("not authenticated", value["unavailable"]["release"])
        release_claim = next(
            claim for claim in value["claims"] if claim["claim"] == "released"
        )
        self.assertIsNone(release_claim["measured"])
        self.assertIn("not authenticated", release_claim["reason"])
        self.assertIs(value["facts"]["landed"], True)
        self.assertIs(value["facts"]["pushed"], False)

    def test_receipt_reports_a_temporary_workspace_as_having_no_repository(
        self,
    ) -> None:
        job_id = "aa0000000005"
        self.fixture_job(job_id)
        value = self.call("receipt", {"job_id": job_id})["result"]
        self.assertEqual(value["workspace"], "temporary")
        self.assertEqual(value["facts"], {})
        self.assertIn("repository", value["unavailable"]["workspace"])
        self.assertEqual(value["sentences"], [])

    def test_a_reviewer_receipt_measures_the_source_workspace(self) -> None:
        self.with_origin()
        source_id, reviewer_id = "aa0000000006", "aa0000000007"
        self.project_job(source_id)
        self.fixture_job(
            reviewer_id,
            {
                "root_job": source_id,
                "parent_job": source_id,
                "review_of": source_id,
                "project": "projects/nova-protocol",
                "project_root": str(self.project),
                "project_root_device": self.project.stat().st_dev,
                "project_root_inode": self.project.stat().st_ino,
                "context_fingerprint": hashlib.sha256(b"context").hexdigest(),
                "workspace": "review",
                "working_directory": str(self.project),
                "landing_branch": "master",
            },
        )
        value = self.call("receipt", {"job_id": reviewer_id})["result"]
        # A reviewer shares the source workspace, so it owns no facts of its own.
        self.assertEqual(value["measured_job"], source_id)
        self.assertEqual(value["job_id"], reviewer_id)
        self.assertIsNotNone(value["facts"]["head"])

    def test_receipt_history_keeps_the_earlier_measurement(self) -> None:
        self.with_origin()
        job_id = "aa0000000008"
        directory = self.project_job(job_id)
        first = self.call("receipt", {"job_id": job_id, "trigger": "done"})["result"]
        second = self.call("receipt", {"job_id": job_id, "trigger": "land"})["result"]
        records = [
            json.loads(line)
            for line in (directory / "receipts.jsonl").read_text().splitlines()
            if line.strip()
        ]
        self.assertEqual([record["trigger"] for record in records], ["done", "land"])
        self.assertEqual(records[0]["measured_at"], first["measured_at"])

        # Inspect reads the newest record and never measures for itself.
        inspected = self.call("inspect", {"job_id": job_id})["result"]
        self.assertEqual(inspected["receipt_count"], 2)
        self.assertEqual(inspected["receipt"]["trigger"], "land")
        self.assertEqual(inspected["receipt"]["measured_at"], second["measured_at"])
        self.call("receipt", {"job_id": job_id, "trigger": "nonsense"}, check=False)
        self.assertEqual(
            self.call(
                "receipt", {"job_id": job_id, "trigger": "nonsense"}, check=False
            )["error"],
            "invalid receipt trigger",
        )

    def test_stop_keeps_an_unmerged_branch_unless_the_request_abandons_it(self) -> None:
        # Sprout owns the refusal and ships separately, so this stands in for
        # the version that knows `--force` and proves this side passes it only
        # for an explicit abandon.
        recorded = self.root / "sprout-argv.log"
        self.fake_program(
            "sprout",
            "#!/bin/sh\n"
            f'printf "%s\\n" "$*" >> {recorded}\n'
            'case "$1 $2" in\n'
            '  "rm --help") echo "usage: sprout rm <feature> [--force]"; exit 0;;\n'
            "esac\n"
            'case "$1" in\n'
            f'  show) printf "%s\\n" "{self.project}"; exit 0;;\n'
            "  rm)\n"
            '    for argument in "$@"; do\n'
            '      if [ "$argument" = "--force" ]; then exit 0; fi\n'
            "    done\n"
            '    echo "sprout: refusing to delete unmerged branch" >&2\n'
            "    exit 1;;\n"
            "esac\n"
            "exit 0\n",
        )
        job_id = "aa0000000009"
        self.sprout_job(job_id, "unmerged-work")
        refused = self.call(
            "stop", {"job_id": job_id, "remove_workspace": True}, check=False
        )
        self.assertFalse(refused["ok"])
        self.assertIn("unmerged", refused["error"])
        self.assertTrue(
            (self.root / "state" / "scufris" / "jobs" / job_id).is_dir(),
            "a refused removal must keep the job and its branch",
        )
        self.assertNotIn("--force", recorded.read_text())

        abandoned = self.call(
            "stop", {"job_id": job_id, "remove_workspace": True, "abandon": True}
        )["result"]
        self.assertEqual(abandoned["state"], "stopped")
        self.assertIn("rm unmerged-work --force", recorded.read_text())
        self.assert_archived(job_id)

    def test_stop_withholds_force_from_a_sprout_that_does_not_know_it(self) -> None:
        # Sprout ships separately, so a machine can still hold the version that
        # deletes without asking. Handing that one `--force` would fail every
        # abandon on an unknown argument, and the probe exists to prevent it.
        recorded = self.root / "sprout-argv.log"
        self.fake_program(
            "sprout",
            "#!/bin/sh\n"
            f'printf "%s\\n" "$*" >> {recorded}\n'
            'case "$1 $2" in\n'
            '  "rm --help") echo "usage: sprout rm <feature>"; exit 0;;\n'
            "esac\n"
            'case "$1" in\n'
            f'  show) printf "%s\\n" "{self.project}"; exit 0;;\n'
            "  rm)\n"
            '    for argument in "$@"; do\n'
            '      if [ "$argument" = "--force" ]; then\n'
            '        echo "sprout: unexpected argument --force" >&2\n'
            "        exit 1\n"
            "      fi\n"
            "    done\n"
            "    exit 0;;\n"
            "esac\n"
            "exit 0\n",
        )
        job_id = "aa0000000011"
        self.sprout_job(job_id, "old-sprout-work")
        stopped = self.call(
            "stop", {"job_id": job_id, "remove_workspace": True, "abandon": True}
        )["result"]
        self.assertEqual(stopped["state"], "stopped")
        self.assertIn("rm old-sprout-work", recorded.read_text())
        self.assertNotIn("--force", recorded.read_text())
        self.assert_archived(job_id)

    def test_stop_refuses_abandon_without_removal(self) -> None:
        job_id = "aa0000000010"
        self.fixture_job(job_id)
        refused = self.call("stop", {"job_id": job_id, "abandon": True}, check=False)
        self.assertFalse(refused["ok"])
        self.assertIn("remove_workspace", refused["error"])


if __name__ == "__main__":
    unittest.main()
