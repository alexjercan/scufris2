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
REPORTER = REPOSITORY / "tools" / "jobs" / "scufris-report"

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
    stream.write('done: report complete\\n')
for line in sys.stdin:
    with (directory / 'received').open('a') as stream:
        stream.write(line)
    if line.strip() == '/exit':
        break
    time.sleep(0.01)
"""

FAKE_PI_EXIT_AFTER_DONE = """#!/usr/bin/env python3
import pathlib
import sys
prompt = pathlib.Path(sys.argv[-1].removeprefix('Read and follow '))
directory = prompt.parent
with (directory / 'status').open('a') as stream:
    stream.write('done: assignment complete\\n')
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
            """[preferences.tracking]
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

    def test_done_remains_nonterminal_and_harness_exit_generates_failure(self) -> None:
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
        self.wait_for(status, "failed: worker harness exited unexpectedly")
        self.assertEqual(
            status.read_text().splitlines(),
            [
                "done: assignment complete",
                "failed: worker harness exited unexpectedly",
            ],
        )

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
        directory = self.root / "state" / "scufris" / "jobs" / job_id
        self.wait_for(directory / "status", "done: report complete")
        self.assertTrue((directory / "workspace").is_dir())
        self.assertFalse((directory / "project-context.md").exists())
        prompt = (directory / "prompt.md").read_text()
        self.assertIn("done: <summary>", prompt)
        self.assertNotIn("ready:", prompt)
        self.assertNotIn("needs-decision:", prompt)
        self.assertIn("You cannot report `failed`", prompt)
        self.assertIn("Call the `scufris_report` tool.", prompt)

        events = self.call(
            "events",
            {"jobs": [{"job_id": job_id, "offset": 0, "tail": "", "inode": None}]},
        )["result"]["jobs"][0]
        self.assertEqual(
            events["events"],
            ["working: fake worker started", "done: report complete"],
        )
        reporter_env = {**self.env, "SCUFRIS_JOB_ID": job_id}
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
            "reported working: report adapter verified\n",
        )
        rejected_failure = self.call(
            "report",
            {
                "job_id": job_id,
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
                "event": "done",
                "summary": "research report complete",
                "report": "# Result\n\nThe report is complete.\n",
            },
        )["result"]
        self.assertEqual(reported["event"], "done")
        self.assertEqual(
            (directory / "report.md").read_text(),
            "# Result\n\nThe report is complete.\n",
        )
        next_events = self.call(
            "events",
            {
                "jobs": [
                    {
                        "job_id": job_id,
                        "offset": events["offset"],
                        "tail": events["tail"],
                        "inode": events["inode"],
                    }
                ]
            },
        )["result"]["jobs"][0]
        self.assertEqual(
            next_events["events"],
            [
                "working: report adapter verified",
                "done: research report complete",
            ],
        )
        self.call("send", {"job_id": job_id, "message": "Continue carefully."})
        self.wait_for(directory / "received", "Continue carefully.")

        listed = subprocess.run(
            [str(REPOSITORY / "scripts" / "scufris-jobs"), "--json"],
            text=True,
            capture_output=True,
            env=self.env,
            check=True,
            timeout=30,
        )
        listed_jobs = json.loads(listed.stdout)["jobs"]
        self.assertEqual([item["job_id"] for item in listed_jobs], [job_id])

        self.call("stop", {"job_id": job_id})
        time.sleep(0.05)
        self.assertNotIn(
            "worker harness exited unexpectedly",
            (directory / "status").read_text(),
        )

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
                "feature": f"fixture-{job_id}",
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

        snapshot = self.call("quick-review-snapshot", {"job_id": job_id})["result"]
        walkthrough = f"""# Fixture Quick Review

Review the exact fixture change.

:::walkthrough
status: ready
revision: {snapshot["revision"]}
baseRevision: {snapshot["base_revision"]}
files: 1
added: 1
removed: 0
:::

:::change
id: result-file
importance: important
file: RESULT.md
lines: 1
:::

The implementation adds the requested result.

```diff
+replacement works
```

:::review
Confirm that the result content is correct.
:::
"""
        self.env["SCUFRIS_FAKE_QUICK_REVIEW_OUTPUT"] = json.dumps(
            {
                "revision": snapshot["revision"],
                "markdown": walkthrough,
                "sectionCount": 1,
            }
        )
        try:
            quick_review = self.call(
                "quick-review-build",
                {
                    "job_id": job_id,
                    "model": "openai-codex/gpt-5.6-sol",
                    "thinking": "medium",
                },
            )["result"]
        finally:
            self.env.pop("SCUFRIS_FAKE_QUICK_REVIEW_OUTPUT", None)
        self.assertEqual(quick_review["revision"], snapshot["revision"])
        self.assertEqual(
            Path(quick_review["artifact"]).read_text(),
            walkthrough,
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
