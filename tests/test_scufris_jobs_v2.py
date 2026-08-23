from __future__ import annotations

import json
import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from typing import Any

REPOSITORY = Path(__file__).resolve().parents[1]
HELPER = REPOSITORY / "tools" / "jobs" / "scufris-jobs"

FAKE_PI = """#!/usr/bin/env python3
import pathlib
import sys
import time
prompt = pathlib.Path(sys.argv[-1].removeprefix('Read and follow '))
directory = prompt.parent
(directory / 'worker-prompt.txt').write_text(prompt.read_text())
(directory / 'worker-argv.json').write_text(__import__('json').dumps(sys.argv[1:]))
with (directory / 'status').open('a') as stream:
    stream.write('working: fake worker started\\n')
    stream.write('ready: report-complete\\n')
for line in sys.stdin:
    with (directory / 'received').open('a') as stream:
        stream.write(line)
    if line.strip() == '/exit':
        break
    time.sleep(0.01)
"""


class ReplacementJobsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="scufris-v2-")
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
            """version = 1

[preferences.tracking]
name = "tatr"
guidance = "Use project tasks."

[preferences.implementation]
name = "pi"
options = { model = "openai-codex/gpt-5.6-sol", thinking = "medium" }
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
        self.env.pop("TMUX", None)
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

    def tearDown(self) -> None:
        for job_id in self.jobs:
            self.call("stop", {"job_id": job_id}, check=False)
        self.temporary.cleanup()

    def call(
        self, command: str, request: dict[str, Any], *, check: bool = True
    ) -> dict[str, Any]:
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

    def wait_for(self, path: Path, text: str) -> None:
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            if path.exists() and text in path.read_text():
                return
            time.sleep(0.05)
        self.fail(f"timed out waiting for {text!r} in {path}")

    def test_project_context_is_canonical_and_malformed_config_is_ignored(self) -> None:
        projects = self.call("projects", {})["result"]["projects"]
        self.assertEqual(projects, ["projects/nova-protocol"])
        context = self.call("context", {"project": "projects/nova-protocol"})["result"]
        self.assertTrue(context["configured"])
        self.assertIn("## tracking", context["markdown"])
        self.assertIn("Preferred selection: tatr", context["markdown"])
        self.assertIn('"thinking": "medium"', context["markdown"])

        (self.project / ".scufris.toml").write_text("not = [valid")
        ignored = self.call("context", {"project": "projects/nova-protocol"})["result"]
        self.assertFalse(ignored["configured"])
        self.assertIn("ignored .scufris.toml", ignored["diagnostic"])

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
        directory = self.root / "state" / "scufris" / "jobs-v2" / job_id
        self.wait_for(directory / "status", "ready: report-complete")
        self.assertTrue((directory / "workspace").is_dir())
        self.assertFalse((directory / "project-context.md").exists())
        prompt = (directory / "prompt.md").read_text()
        self.assertIn("ready: <milestone-slug>", prompt)

        polled = self.call(
            "poll",
            {"jobs": [{"job_id": job_id, "offset": 0, "tail": "", "inode": None}]},
        )["result"]["jobs"][0]
        self.assertEqual(
            polled["events"],
            ["working: fake worker started", "ready: report-complete"],
        )
        self.call("send", {"job_id": job_id, "message": "Continue carefully."})
        self.wait_for(directory / "received", "Continue carefully.")

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
        directory = self.root / "state" / "scufris" / "jobs-v2" / job_id
        self.wait_for(directory / "status", "ready: report-complete")
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
        review_directory = self.root / "state" / "scufris" / "jobs-v2" / review_id
        self.wait_for(review_directory / "status", "ready: report-complete")
        self.assertIn(
            "independent read-only reviewer",
            (review_directory / "prompt.md").read_text(),
        )
        argv = json.loads((review_directory / "worker-argv.json").read_text())
        self.assertIn("read,grep,find,ls", argv)
        review_record = json.loads((review_directory / "job.json").read_text())
        self.assertEqual(review_record["review_of"], job_id)
        self.assertEqual(review_record["working_directory"], str(self.project))

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
                "feature": "fixture-change",
            },
        )["result"]
        self.jobs.append(job_id)
        self.assertEqual(result["workspace"], "sprout")
        directory = self.root / "state" / "scufris" / "jobs-v2" / job_id
        self.wait_for(directory / "status", "ready: report-complete")
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
        self.assertEqual(
            (self.project / "RESULT.md").read_text(), "replacement works\n"
        )
        self.assertFalse(worktree.exists())


if __name__ == "__main__":
    unittest.main()
