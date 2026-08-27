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
                "SCUFRIS_PROJECT_ROOTS": json.dumps([str(self.projects)]),
            }
        )
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
        self.assertEqual(detail["summary"], status_summary)
        self.assertLessEqual(len(detail["events"]), 100)
        human = self.cli("400000000001").stdout
        self.assertNotIn("\x1b", human)
        self.assertNotIn("\x07", human)
        self.assertNotIn("\r", human)
        self.assertIn(r"\x1b[31m red\x07\x0dnext", human)
        self.assertIn(r"status\x9b31m text", human)

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
        self.assertIn("steer that same job with scufris_job_send", markdown)
        self.assertIn("no record of what it already accepted", markdown)

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
        self.assertTrue(bounded_report.startswith("# working: bounded update 4\n"))
        self.assertNotIn("# working: bounded update 3\n", bounded_report)

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
        self.assertIn("Report:\n# working: bounded update 4", detail)
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
            str(REPOSITORY / "extensions/scufris/workflow/worker-report.ts"),
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
        self.assertEqual(set(landed["removed_jobs"]), {job_id, reviewer_id})
        self.assert_archived(reviewer_id)
        self.assert_archived(job_id)
        self.assertEqual(
            (self.project / "RESULT.md").read_text(), "replacement works\n"
        )
        self.assertFalse(worktree.exists())


if __name__ == "__main__":
    unittest.main()
