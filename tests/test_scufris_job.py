from __future__ import annotations

import json
import os
import shutil
import stat
import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from typing import Any

REPOSITORY = Path(__file__).resolve().parents[1]
CLI = REPOSITORY / "scripts" / "scufris-job"

FAKE_HARNESS = r"""#!/usr/bin/env python3
import os
import pathlib
import sys

prompt_arg = sys.argv[-1]
prefix = "Read and follow "
if not prompt_arg.startswith(prefix):
    raise SystemExit("missing prompt pointer")
prompt = pathlib.Path(prompt_arg[len(prefix):])
directory = prompt.parent
(directory / "role-marker").write_text(os.environ.get("SCUFRIS_ROLE", ""), encoding="utf-8")
with (directory / "scufris-environment.json").open("w", encoding="utf-8") as stream:
    import json
    json.dump({key: os.environ.get(key) for key in (
        "SCUFRIS_ROLE",
        "SCUFRIS_SPEECH",
        "SCUFRIS_CALM",
        "SCUFRIS_PIPER_MODEL",
        "SCUFRIS_PIPER_CONFIG",
    )}, stream)
with (directory / "argv.json").open("w", encoding="utf-8") as stream:
    import json
    json.dump(sys.argv[1:], stream)
with (directory / "status").open("a", encoding="utf-8", newline="") as stream:
    stream.write("working: fake harness ready\n")
for line in sys.stdin:
    message = line.rstrip("\r\n")
    with (directory / "received").open("a", encoding="utf-8", newline="") as stream:
        stream.write(message + "\n")
    if message in {"/quit", "/exit"}:
        break
    with (directory / "status").open("a", encoding="utf-8", newline="") as stream:
        stream.write("working: steering received\n")
"""


class ScufrisJobIntegrationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="scufris-test-")
        self.root = Path(self.temporary.name)
        self.state = self.root / "state"
        self.cache = self.root / "cache"
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("pi", "claude"):
            path = self.bin / name
            path.write_text(FAKE_HARNESS, encoding="utf-8")
            path.chmod(0o755)
        real_sprout = shutil.which("sprout")
        if real_sprout is None:
            self.fail("sprout is required")
        sprout_wrapper = self.bin / "sprout"
        sprout_wrapper.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, pathlib, sys\n"
            "log = os.environ.get('SCUFRIS_SPROUT_LOG')\n"
            "if log:\n"
            "    with pathlib.Path(log).open('a', encoding='utf-8') as stream:\n"
            "        stream.write(json.dumps(sys.argv[1:]) + '\\n')\n"
            "if os.environ.get('SCUFRIS_FAIL_RM') == '1' and sys.argv[1:2] == ['rm']:\n"
            "    print('injected cleanup failure', file=sys.stderr)\n"
            "    raise SystemExit(1)\n"
            f"os.execv({real_sprout!r}, [{real_sprout!r}, *sys.argv[1:]])\n",
            encoding="utf-8",
        )
        sprout_wrapper.chmod(0o755)
        self.projects_root = self.root / "projects"
        self.project = self.projects_root / "target"
        self.project.mkdir(parents=True)
        self.run_external(["git", "init", "-b", "master"], cwd=self.project)
        self.run_external(
            ["git", "config", "user.email", "test@example.invalid"], cwd=self.project
        )
        self.run_external(
            ["git", "config", "user.name", "Scufris Test"], cwd=self.project
        )
        (self.project / "README.md").write_text("# Fixture\n", encoding="utf-8")
        self.run_external(["git", "add", "README.md"], cwd=self.project)
        self.run_external(["git", "commit", "-m", "fixture"], cwd=self.project)
        self.tmux_root = self.root / "tmux"
        self.tmux_root.mkdir()
        self.env = os.environ.copy()
        self.env.pop("TMUX", None)
        self.env.update(
            {
                "PATH": f"{self.bin}:{self.env['PATH']}",
                "XDG_STATE_HOME": str(self.state),
                "XDG_CACHE_HOME": str(self.cache),
                "TMUX_TMPDIR": str(self.tmux_root),
                "SCUFRIS_PROJECT_ROOTS": json.dumps([str(self.projects_root)]),
                "SCUFRIS_ROLE": "orchestrator",
                "SCUFRIS_SPEECH": "1",
                "SCUFRIS_CALM": "1",
                "SCUFRIS_PIPER_MODEL": "/trusted/model.onnx",
                "SCUFRIS_PIPER_CONFIG": "/trusted/model.json",
            }
        )
        self.jobs: dict[str, str] = {}
        Path("/tmp/scufris-must-not-exist").unlink(missing_ok=True)
        self.default_server = self.default_tmux_server()

    def tearDown(self) -> None:
        for job_id, feature in self.jobs.items():
            self.cli("stop", {"job_id": job_id}, check=False)
            self.run_external(
                ["sprout", "rm", feature],
                cwd=self.project,
                env=self.env,
                check=False,
            )
        self.assertEqual(self.default_tmux_server(), self.default_server)
        self.temporary.cleanup()

    def default_tmux_server(self) -> str | None:
        if "TMUX" not in os.environ:
            return None
        result = subprocess.run(
            ["tmux", "display-message", "-p", "#{pid}"],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        return result.stdout.strip() if result.returncode == 0 else None

    def run_external(
        self,
        argv: list[str],
        *,
        cwd: Path | None = None,
        env: dict[str, str] | None = None,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            argv,
            cwd=cwd,
            env=env,
            text=True,
            capture_output=True,
            check=check,
            timeout=30,
        )

    def cli(
        self, command: str, request: dict[str, Any], *, check: bool = True
    ) -> dict[str, Any]:
        result = subprocess.run(
            [str(CLI), command],
            input=json.dumps(request),
            env=self.env,
            text=True,
            capture_output=True,
            check=False,
            timeout=90,
        )
        envelope = json.loads(result.stdout)
        if check and (result.returncode != 0 or not envelope["ok"]):
            self.fail(f"CLI failed: {envelope} stderr={result.stderr}")
        return envelope

    def spawn(
        self,
        job_id: str = "abc123def456",
        harness: str = "pi",
        *,
        project: str | None = None,
        current_root: Path | None = None,
        feature: str | None = None,
        cleanup: str | None = None,
    ) -> dict[str, Any]:
        request = {
            "job_id": job_id,
            "harness": harness,
            "instructions": "Make a bounded fixture change.",
            "current_root": str(current_root or self.project),
        }
        if project is not None:
            request["project"] = project
        if feature is not None:
            request["feature"] = feature
        if cleanup is not None:
            request["cleanup"] = cleanup
        envelope = self.cli(
            "spawn",
            request,
        )
        result = envelope["result"]
        self.jobs[job_id] = result["feature"]
        return result

    def wait_for(self, path: Path, text: str, timeout: float = 8) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if path.exists() and text in path.read_text(encoding="utf-8"):
                return
            time.sleep(0.05)
        self.fail(f"timed out waiting for {text!r} in {path}")

    def poll_request(
        self,
        job_id: str,
        *,
        offset: int = 0,
        tail: str = "",
        inode: int | None = None,
    ) -> dict[str, Any]:
        return self.cli(
            "poll",
            {
                "jobs": [
                    {"job_id": job_id, "offset": offset, "tail": tail, "inode": inode}
                ]
            },
        )["result"]["jobs"][0]

    def test_spawn_send_inspect_stop_and_orphans(self) -> None:
        result = self.spawn()
        self.assertEqual(result["model"], "openai-codex/gpt-5.6-sol")
        self.assertEqual(result["thinking"], "medium")
        self.assertEqual(result["project"], "current")
        self.assertEqual(result["feature"], f"scufris-{result['job_id']}")
        self.assertEqual(result["cleanup"], "remove")
        directory = self.state / "scufris" / "jobs" / result["job_id"]
        self.wait_for(directory / "status", "working: fake harness ready")
        pi_argv = json.loads((directory / "argv.json").read_text(encoding="utf-8"))
        self.assertNotIn("--extension", pi_argv)
        self.assertNotIn("--skill", pi_argv)
        self.assertEqual((directory / "role-marker").read_text(encoding="utf-8"), "")
        self.assertEqual(
            json.loads(
                (directory / "scufris-environment.json").read_text(encoding="utf-8")
            ),
            {
                "SCUFRIS_ROLE": None,
                "SCUFRIS_SPEECH": None,
                "SCUFRIS_CALM": None,
                "SCUFRIS_PIPER_MODEL": None,
                "SCUFRIS_PIPER_CONFIG": None,
            },
        )

        self.assertEqual(stat.S_IMODE((directory / "prompt.md").stat().st_mode), 0o400)
        prompt = (directory / "prompt.md").read_text(encoding="utf-8")
        for required in (
            "bounded delegated worker, not a normal user session",
            "Do not land, push",
            "or spawn workers",
            "decision, recommendation, consequences",
            "blocker, attempts, evidence, effect, and exact unblock condition",
            "Scufris verifies Git state and opens structured Plannotator review",
            "Review feedback returns to this same session",
            "Do not use `done` before review",
            "done: review approved with no changes requested",
            f"sprout sync {result['feature']}",
        ):
            self.assertIn(required, prompt)
        self.assertEqual(stat.S_IMODE((directory / "job.json").stat().st_mode), 0o400)
        record = json.loads((directory / "job.json").read_text(encoding="utf-8"))
        self.assertEqual(record["cleanup"], "remove")
        self.assertTrue(
            (self.cache / "sprouts" / self.project.name / result["feature"]).is_dir()
        )
        self.assertEqual(result["tmux_session"], f"target_scufris-{result['job_id']}")
        windows = self.run_external(
            [
                "tmux",
                "list-windows",
                "-t",
                f"={result['tmux_session']}",
                "-F",
                "#{window_name}:#{pane_current_path}",
            ],
            env=self.env,
        ).stdout.splitlines()
        worktree = self.cache / "sprouts" / self.project.name / result["feature"]
        self.assertEqual(windows, [f"job-{result['job_id']}:{worktree}"])
        self.assertEqual(
            self.cli("orphans", {})["result"]["job_ids"], [result["job_id"]]
        )

        message = "literal ; $(touch /tmp/scufris-must-not-exist) ' \\"
        self.cli("send", {"job_id": result["job_id"], "message": message})
        self.wait_for(directory / "received", "literal")
        self.assertIn(message, (directory / "received").read_text(encoding="utf-8"))
        self.assertFalse(Path("/tmp/scufris-must-not-exist").exists())

        inspected = self.cli(
            "inspect", {"job_id": result["job_id"], "include_report": True}
        )["result"]
        self.assertTrue(inspected["window_alive"])
        self.assertEqual(inspected["project"], "current")
        self.assertEqual(inspected["cleanup"], "remove")
        self.assertIn("working: fake harness ready", inspected["events"])

        self.assertEqual(
            self.cli("projects", {})["result"]["projects"], ["projects/target"]
        )
        claude = self.spawn(
            "fedcba987654",
            "claude",
            project="projects/target",
            current_root=self.root,
            feature="fix-cross-project-launch",
            cleanup="retain",
        )
        self.assertEqual(claude["project"], "projects/target")
        self.assertEqual(claude["feature"], "fix-cross-project-launch")
        self.assertEqual(claude["cleanup"], "retain")
        self.assertEqual(claude["tmux_session"], "target_fix-cross-project-launch")
        claude_directory = self.state / "scufris" / "jobs" / claude["job_id"]
        self.wait_for(claude_directory / "status", "working: fake harness ready")
        claude_argv = json.loads(
            (claude_directory / "argv.json").read_text(encoding="utf-8")
        )
        self.assertIn("--dangerously-skip-permissions", claude_argv)
        self.assertIn(
            "sprout sync fix-cross-project-launch",
            (claude_directory / "prompt.md").read_text(encoding="utf-8"),
        )
        self.assertEqual(claude["model"], "opus")
        self.assertEqual(claude["thinking"], "xhigh")
        claude_record = json.loads(
            (claude_directory / "job.json").read_text(encoding="utf-8")
        )
        self.assertEqual(claude_record["cleanup"], "retain")

        self.assertEqual(
            self.cli("stop", {"job_id": result["job_id"]})["result"]["state"], "stopped"
        )
        second_inspection = self.cli(
            "inspect", {"job_id": claude["job_id"], "include_report": False}
        )["result"]
        self.assertTrue(second_inspection["window_alive"])

        self.cli("send", {"job_id": claude["job_id"], "message": "/exit"})
        deadline = time.monotonic() + 8
        while self.poll_request(claude["job_id"])["window_alive"]:
            if time.monotonic() >= deadline:
                self.fail("worker pane did not become dead")
            time.sleep(0.05)
        claude_job = json.loads(
            (claude_directory / "job.json").read_text(encoding="utf-8")
        )
        dead_window = self.run_external(
            [
                "tmux",
                "display-message",
                "-p",
                "-t",
                claude_job["tmux_window_id"],
                "#{pane_dead}",
            ],
            env=self.env,
        )
        self.assertEqual(dead_window.stdout.strip(), "1")
        self.cli("stop", {"job_id": claude["job_id"]})
        removed = self.run_external(
            [
                "tmux",
                "display-message",
                "-p",
                "-t",
                claude_job["tmux_window_id"],
                "#{window_id}",
            ],
            env=self.env,
            check=False,
        )
        self.assertNotEqual(removed.returncode, 0)
        self.assertEqual(
            self.cli("stop", {"job_id": result["job_id"]})["result"]["state"], "stopped"
        )

    def test_guarded_review_snapshot_and_local_landing(self) -> None:
        result = self.spawn("aaa111bbb222", feature="guarded-local-land")
        directory = self.state / "scufris" / "jobs" / result["job_id"]
        self.wait_for(directory / "status", "working: fake harness ready")
        worktree = Path(result["worktree"])
        (worktree / "result.txt").write_text("approved\n", encoding="utf-8")
        self.run_external(["git", "add", "result.txt"], cwd=worktree, env=self.env)
        self.run_external(
            ["git", "commit", "-m", "Add approved result"],
            cwd=worktree,
            env=self.env,
        )
        self.run_external(
            ["sprout", "sync", result["feature"]], cwd=self.project, env=self.env
        )
        extra_window = self.run_external(
            [
                "tmux",
                "new-window",
                "-d",
                "-P",
                "-F",
                "#{window_id}",
                "-t",
                f"={result['tmux_session']}",
                "-n",
                "user-extra",
                "sleep 30",
            ],
            env=self.env,
        ).stdout.strip()
        snapshot = self.cli(
            "review-snapshot",
            {"job_id": result["job_id"], "project_root": str(self.project)},
        )["result"]
        self.assertEqual(snapshot["subject"], "Add approved result")
        self.assertEqual(snapshot["worktree"], str(worktree))

        changed = self.cli(
            "land",
            {
                "job_id": result["job_id"],
                "project_root": str(self.project),
                "landing_sha": "0" * 40,
                "feature_sha": snapshot["feature_sha"],
                "subject": snapshot["subject"],
            },
            check=False,
        )
        self.assertFalse(changed["ok"])
        self.assertIn("approved revisions changed", changed["error"])
        self.assertTrue(worktree.exists())

        sprout_log = self.root / "sprout.log"
        self.env["SCUFRIS_SPROUT_LOG"] = str(sprout_log)
        landed = self.cli(
            "land",
            {
                "job_id": result["job_id"],
                "project_root": str(self.project),
                "landing_sha": snapshot["landing_sha"],
                "feature_sha": snapshot["feature_sha"],
                "subject": snapshot["subject"],
            },
        )["result"]
        self.assertEqual(landed["state"], "landed")
        self.assertEqual(
            (self.project / "result.txt").read_text(encoding="utf-8"), "approved\n"
        )
        self.assertTrue(worktree.exists())
        self.assertEqual(
            self.cli("stop", {"job_id": result["job_id"]})["result"]["state"],
            "stopped",
        )
        self.assertEqual(
            self.run_external(
                ["tmux", "display-message", "-p", "-t", extra_window, "#{window_id}"],
                env=self.env,
            ).stdout.strip(),
            extra_window,
        )
        self.assertEqual(
            self.cli(
                "remove",
                {"job_id": result["job_id"], "project_root": str(self.project)},
            )["result"]["state"],
            "removed",
        )
        self.assertFalse(worktree.exists())
        operations = [
            json.loads(line)
            for line in sprout_log.read_text(encoding="utf-8").splitlines()
        ]
        self.assertEqual(
            operations,
            [
                ["show", result["feature"]],
                ["land", result["feature"], "--dry-run", "-m", "Add approved result"],
                ["show", result["feature"]],
                ["land", result["feature"], "-m", "Add approved result"],
                ["rm", result["feature"]],
            ],
        )
        self.env.pop("SCUFRIS_SPROUT_LOG")
        self.assertNotEqual(
            self.run_external(
                ["tmux", "display-message", "-p", "-t", extra_window, "#{window_id}"],
                env=self.env,
                check=False,
            ).returncode,
            0,
        )

    def test_cross_project_retain_and_cleanup_failure_preserve_landing(self) -> None:
        result = self.spawn(
            "999999999999",
            project="projects/target",
            current_root=self.root,
            feature="cross-project-retain",
            cleanup="retain",
        )
        directory = self.state / "scufris" / "jobs" / result["job_id"]
        self.wait_for(directory / "status", "working: fake harness ready")
        worktree = Path(result["worktree"])
        (worktree / "cross.txt").write_text("landed\n", encoding="utf-8")
        self.run_external(["git", "add", "cross.txt"], cwd=worktree, env=self.env)
        self.run_external(
            ["git", "commit", "-m", "Add cross project result"],
            cwd=worktree,
            env=self.env,
        )
        self.run_external(
            ["sprout", "sync", result["feature"]], cwd=self.project, env=self.env
        )
        snapshot = self.cli(
            "review-snapshot",
            {"job_id": result["job_id"], "project_root": str(self.project)},
        )["result"]
        self.cli(
            "land",
            {
                "job_id": result["job_id"],
                "project_root": str(self.project),
                "landing_sha": snapshot["landing_sha"],
                "feature_sha": snapshot["feature_sha"],
                "subject": snapshot["subject"],
            },
        )
        self.cli("stop", {"job_id": result["job_id"]})
        self.assertTrue(worktree.exists())
        self.assertTrue(
            self.run_external(
                ["git", "show-ref", "--verify", f"refs/heads/{result['feature']}"],
                cwd=self.project,
                env=self.env,
                check=False,
            ).returncode
            == 0
        )

        self.env["SCUFRIS_FAIL_RM"] = "1"
        failed_cleanup = self.cli(
            "remove",
            {"job_id": result["job_id"], "project_root": str(self.project)},
            check=False,
        )
        self.env.pop("SCUFRIS_FAIL_RM")
        self.assertFalse(failed_cleanup["ok"])
        self.assertIn("injected cleanup failure", failed_cleanup["error"])
        self.assertEqual(
            (self.project / "cross.txt").read_text(encoding="utf-8"), "landed\n"
        )
        self.assertTrue(worktree.exists())

    def test_poll_waits_for_lf_and_consumes_malformed_frames_once(self) -> None:
        result = self.spawn("0123456789ab")
        directory = self.state / "scufris" / "jobs" / result["job_id"]
        status_path = directory / "status"
        self.wait_for(status_path, "working: fake harness ready")
        first = self.poll_request(result["job_id"])
        self.assertEqual(first["events"], ["working: fake harness ready"])

        with status_path.open("ab") as stream:
            stream.write(b"working: partial")
        partial = self.poll_request(
            result["job_id"],
            offset=first["offset"],
            tail=first["tail"],
            inode=first["inode"],
        )
        self.assertEqual(partial["events"], [])
        self.assertNotEqual(partial["tail"], "")

        with status_path.open("ab") as stream:
            stream.write(b" complete\nmystery: bad\nunknown: bad\r\n\xffbad\n")
        complete = self.poll_request(
            result["job_id"],
            offset=partial["offset"],
            tail=partial["tail"],
            inode=partial["inode"],
        )
        self.assertEqual(complete["events"], ["working: partial complete"])
        self.assertEqual(
            set(complete["errors"]),
            {
                "status line has invalid grammar or state",
                "status uses CRLF",
                "status line is not valid UTF-8",
            },
        )
        unchanged = self.poll_request(
            result["job_id"],
            offset=complete["offset"],
            tail=complete["tail"],
            inode=complete["inode"],
        )
        self.assertEqual(unchanged["events"], [])
        self.assertEqual(unchanged["errors"], [])

        with status_path.open("ab") as stream:
            stream.write(b"working: " + b"x" * 2048 + b"\n")
        oversized_line = self.poll_request(
            result["job_id"],
            offset=unchanged["offset"],
            tail=unchanged["tail"],
            inode=unchanged["inode"],
        )
        self.assertEqual(oversized_line["errors"], ["status line exceeds 2 KiB"])

        with status_path.open("ab") as stream:
            stream.write(b"x" * (256 * 1024))
        oversized_file = self.poll_request(
            result["job_id"],
            offset=oversized_line["offset"],
            tail=oversized_line["tail"],
            inode=oversized_line["inode"],
        )
        self.assertEqual(oversized_file["errors"], ["status exceeds 256 KiB"])

    def test_tmux_policy_never_changes_clients_or_destroys_containers(self) -> None:
        source = CLI.read_text(encoding="utf-8")
        for command in (
            "attach-session",
            "select-window",
            "switch-client",
            "kill-session",
            "kill-server",
        ):
            self.assertNotIn(f'"{command}"', source)

    def test_rejects_invalid_features_and_feature_collisions(self) -> None:
        for job_id, feature in (
            ("111111111111", "Uppercase"),
            ("222222222222", "two--hyphens"),
            ("333333333333", "a" * 49),
        ):
            rejected = self.cli(
                "spawn",
                {
                    "job_id": job_id,
                    "harness": "pi",
                    "instructions": "Do not run.",
                    "current_root": str(self.project),
                    "feature": feature,
                },
                check=False,
            )
            self.assertFalse(rejected["ok"])
            self.assertEqual(rejected["error"], "invalid feature")

        first = self.spawn("444444444444", feature="shared-feature")
        collision = self.cli(
            "spawn",
            {
                "job_id": "555555555555",
                "harness": "pi",
                "instructions": "Do not replace the first worker.",
                "current_root": str(self.project),
                "feature": "shared-feature",
            },
            check=False,
        )
        self.assertFalse(collision["ok"])
        self.assertEqual(collision["error"], "feature already exists: shared-feature")
        self.assertFalse((self.state / "scufris" / "jobs" / "555555555555").exists())
        self.assertTrue(
            self.cli("inspect", {"job_id": first["job_id"]})["result"]["window_alive"]
        )

        self.run_external(
            ["git", "branch", "reserved-feature"], cwd=self.project, env=self.env
        )
        branch_collision = self.cli(
            "spawn",
            {
                "job_id": "666666666666",
                "harness": "pi",
                "instructions": "Do not reuse an existing branch.",
                "current_root": str(self.project),
                "feature": "reserved-feature",
            },
            check=False,
        )
        self.assertFalse(branch_collision["ok"])
        self.assertEqual(
            branch_collision["error"], "feature already exists: reserved-feature"
        )

        session = "target_session-collision"
        self.run_external(["tmux", "new-session", "-d", "-s", session], env=self.env)
        try:
            session_collision = self.cli(
                "spawn",
                {
                    "job_id": "777777777777",
                    "harness": "pi",
                    "instructions": "Do not reuse an existing session.",
                    "current_root": str(self.project),
                    "feature": "session-collision",
                },
                check=False,
            )
            self.assertFalse(session_collision["ok"])
            self.assertEqual(
                session_collision["error"],
                "feature already exists: session-collision",
            )
            self.assertEqual(
                self.run_external(
                    ["tmux", "has-session", "-t", f"={session}"],
                    env=self.env,
                    check=False,
                ).returncode,
                0,
            )
        finally:
            self.run_external(
                ["tmux", "kill-session", "-t", f"={session}"],
                env=self.env,
                check=False,
            )

    def test_rejects_unknown_project_and_request_fields(self) -> None:
        unknown = self.cli(
            "spawn",
            {
                "job_id": "bbbbbbbbbbbb",
                "harness": "pi",
                "instructions": "Do not run.",
                "current_root": str(self.root),
                "project": "projects/missing",
            },
            check=False,
        )
        self.assertFalse(unknown["ok"])
        self.assertIn("unknown project ID", unknown["error"])

        escaping = self.cli(
            "spawn",
            {
                "job_id": "dddddddddddd",
                "harness": "pi",
                "instructions": "Do not run.",
                "current_root": str(self.root),
                "project": "../target",
            },
            check=False,
        )
        self.assertFalse(escaping["ok"])
        self.assertIn("invalid project ID", escaping["error"])

        outside = self.cli(
            "spawn",
            {
                "job_id": "cccccccccccc",
                "harness": "pi",
                "instructions": "Do not run.",
                "current_root": str(self.root),
            },
            check=False,
        )
        self.assertFalse(outside["ok"])
        self.assertIn("project is required", outside["error"])

        invalid_cleanup = self.cli(
            "spawn",
            {
                "job_id": "eeeeeeeeeeee",
                "harness": "pi",
                "instructions": "Do not run.",
                "current_root": str(self.project),
                "cleanup": "delete",
            },
            check=False,
        )
        self.assertFalse(invalid_cleanup["ok"])
        self.assertEqual(invalid_cleanup["error"], "cleanup must be remove or retain")

        result = self.cli("orphans", {"command": "unsafe operation"}, check=False)
        self.assertFalse(result["ok"])
        self.assertIn("unknown fields", result["error"])


if __name__ == "__main__":
    unittest.main()
