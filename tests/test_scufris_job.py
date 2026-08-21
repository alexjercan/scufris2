from __future__ import annotations

import json
import os
import stat
import subprocess
import tempfile
import time
import unittest
import uuid
from pathlib import Path
from typing import Any

REPOSITORY = Path(__file__).resolve().parents[1]
CLI = REPOSITORY / "scripts" / "scufris-job"

FAKE_HARNESS = r"""#!/usr/bin/env python3
import pathlib
import sys

prompt_arg = sys.argv[-1]
prefix = "Read and follow "
if not prompt_arg.startswith(prefix):
    raise SystemExit("missing prompt pointer")
prompt = pathlib.Path(prompt_arg[len(prefix):])
directory = prompt.parent
with (directory / "status").open("a", encoding="utf-8", newline="") as stream:
    stream.write("working: fake harness ready\n")
with (directory / "argv.json").open("w", encoding="utf-8") as stream:
    import json
    json.dump(sys.argv[1:], stream)
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
        self.socket = f"scufris-test-{uuid.uuid4().hex[:12]}"
        self.env = os.environ.copy()
        self.env.update(
            {
                "PATH": f"{self.bin}:{self.env['PATH']}",
                "XDG_STATE_HOME": str(self.state),
                "XDG_CACHE_HOME": str(self.cache),
                "SCUFRIS_TMUX_SOCKET": self.socket,
                "SCUFRIS_PROJECT_ROOTS": json.dumps([str(self.projects_root)]),
            }
        )
        self.jobs: list[str] = []
        Path("/tmp/scufris-must-not-exist").unlink(missing_ok=True)
        self.default_server = self.default_tmux_server()

    def tearDown(self) -> None:
        for job_id in self.jobs:
            self.cli("stop", {"job_id": job_id}, check=False)
            self.run_external(
                ["tmux", "-L", self.socket, "kill-window", "-t", f"jobs:job-{job_id}"],
                env=self.isolated_env(),
                check=False,
            )
            self.run_external(
                ["sprout", "rm", f"scufris-{job_id}"],
                cwd=self.project,
                env=self.isolated_env(),
                check=False,
            )
        self.assertEqual(self.default_tmux_server(), self.default_server)
        self.temporary.cleanup()

    def isolated_env(self) -> dict[str, str]:
        env = self.env.copy()
        env.pop("TMUX", None)
        tmux_root = self.state / "scufris" / "tmux"
        tmux_root.mkdir(parents=True, exist_ok=True)
        env["TMUX_TMPDIR"] = str(tmux_root)
        return env

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
    ) -> dict[str, Any]:
        request = {
            "job_id": job_id,
            "harness": harness,
            "instructions": "Make a bounded fixture change.",
            "current_root": str(current_root or self.project),
        }
        if project is not None:
            request["project"] = project
        envelope = self.cli(
            "spawn",
            request,
        )
        self.jobs.append(job_id)
        return envelope["result"]

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
        self.assertEqual(result["model"], "openai/gpt-5.6-sol")
        self.assertEqual(result["thinking"], "medium")
        self.assertEqual(result["project"], "current")
        directory = self.state / "scufris" / "jobs" / result["job_id"]
        self.wait_for(directory / "status", "working: fake harness ready")

        self.assertEqual(stat.S_IMODE((directory / "prompt.md").stat().st_mode), 0o400)
        self.assertEqual(stat.S_IMODE((directory / "job.json").stat().st_mode), 0o400)
        self.assertTrue(
            (self.cache / "sprouts" / self.project.name / result["feature"]).is_dir()
        )
        windows = self.run_external(
            [
                "tmux",
                "-L",
                self.socket,
                "list-windows",
                "-t",
                "jobs",
                "-F",
                "#{window_name}",
            ],
            env=self.isolated_env(),
        ).stdout.splitlines()
        self.assertEqual(windows, [f"job-{result['job_id']}"])
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
        self.assertIn("working: fake harness ready", inspected["events"])

        self.assertEqual(
            self.cli("projects", {})["result"]["projects"], ["projects/target"]
        )
        claude = self.spawn(
            "fedcba987654",
            "claude",
            project="projects/target",
            current_root=self.root,
        )
        self.assertEqual(claude["project"], "projects/target")
        claude_directory = self.state / "scufris" / "jobs" / claude["job_id"]
        self.wait_for(claude_directory / "status", "working: fake harness ready")
        claude_argv = json.loads(
            (claude_directory / "argv.json").read_text(encoding="utf-8")
        )
        self.assertIn("--dangerously-skip-permissions", claude_argv)
        self.assertEqual(claude["model"], "opus")
        self.assertEqual(claude["thinking"], "xhigh")

        self.assertEqual(
            self.cli("stop", {"job_id": result["job_id"]})["result"]["state"], "stopped"
        )
        second_inspection = self.cli(
            "inspect", {"job_id": claude["job_id"], "include_report": False}
        )["result"]
        self.assertTrue(second_inspection["window_alive"])
        self.assertEqual(
            self.cli("stop", {"job_id": result["job_id"]})["result"]["state"], "stopped"
        )

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

        result = self.cli("orphans", {"command": "tmux kill-server"}, check=False)
        self.assertFalse(result["ok"])
        self.assertIn("unknown fields", result["error"])


if __name__ == "__main__":
    unittest.main()
