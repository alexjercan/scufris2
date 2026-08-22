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
CLI = REPOSITORY / "scripts" / "scufris-jobs"


class ScufrisJobsIntegrationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="scufris-jobs-test-")
        self.root = Path(self.temporary.name)
        self.state = self.root / "state"
        self.cache = self.root / "cache"
        self.jobs = self.state / "scufris" / "jobs"
        self.jobs.mkdir(parents=True)
        self.tmux_root = self.root / "tmux"
        self.tmux_root.mkdir()
        self.env = os.environ.copy()
        self.env.pop("TMUX", None)
        self.env.update(
            {
                "XDG_STATE_HOME": str(self.state),
                "XDG_CACHE_HOME": str(self.cache),
                "TMUX_TMPDIR": str(self.tmux_root),
            }
        )
        self.default_server = self.default_tmux_server()
        self.sessions: list[str] = []

    def tearDown(self) -> None:
        for session in self.sessions:
            self.external(["tmux", "kill-session", "-t", f"={session}"], check=False)
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

    def external(
        self,
        argv: list[str],
        *,
        cwd: Path | None = None,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            argv,
            cwd=cwd,
            env=self.env,
            text=True,
            capture_output=True,
            check=check,
            timeout=20,
        )

    def cli(
        self, *arguments: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            [str(CLI), *arguments],
            env=self.env,
            text=True,
            capture_output=True,
            check=False,
            timeout=20,
        )
        if check and result.returncode != 0:
            self.fail(
                f"CLI failed ({result.returncode}): stdout={result.stdout!r} "
                f"stderr={result.stderr!r}"
            )
        return result

    def git(self, root: Path, *arguments: str) -> str:
        return self.external(["git", *arguments], cwd=root).stdout.strip()

    def create_job(
        self,
        job_id: str,
        feature: str,
        *,
        alive: bool = True,
        status: bytes = b"working: fixture ready\n",
        report: bytes = b"# Fixture report\n",
        created_at: str = "2026-08-21T16:01:23Z",
    ) -> tuple[Path, Path, dict[str, Any]]:
        worktree = self.cache / "sprouts" / "target" / feature
        worktree.mkdir(parents=True)
        self.git(worktree, "init", "-b", feature)
        self.git(worktree, "config", "user.email", "test@example.invalid")
        self.git(worktree, "config", "user.name", "Scufris Test")
        (worktree / "README.md").write_text("# Fixture\n", encoding="utf-8")
        self.git(worktree, "add", "README.md")
        self.git(worktree, "commit", "-m", "fixture")
        revision = self.git(worktree, "rev-parse", "HEAD")

        session = f"target_{feature}"
        pane_format = "#{session_id}\t#{window_id}\t#{pane_id}"
        identity = self.external(
            [
                "tmux",
                "new-session",
                "-d",
                "-P",
                "-F",
                pane_format,
                "-s",
                session,
                "-n",
                f"job-{job_id}",
                "sleep 30",
            ]
        ).stdout.strip()
        self.sessions.append(session)
        session_id, window_id, pane_id = identity.split("\t")
        if not alive:
            self.external(
                ["tmux", "set-option", "-w", "-t", window_id, "remain-on-exit", "on"]
            )
            self.external(["tmux", "respawn-pane", "-k", "-t", pane_id, "true"])
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline:
                dead = self.external(
                    ["tmux", "display-message", "-p", "-t", pane_id, "#{pane_dead}"]
                ).stdout.strip()
                if dead == "1":
                    break
                time.sleep(0.02)
            else:
                self.fail("fixture pane did not stop")

        directory = self.jobs / job_id
        directory.mkdir()
        record = {
            "version": 2,
            "job_id": job_id,
            "harness": "pi",
            "model": "openai-codex/test-model",
            "thinking": "medium",
            "feature": feature,
            "cleanup": "remove",
            "review": {
                "profile": "code",
                "brief": "Audience: maintainers. Outcome: inspect the fixture.",
            },
            "project": "current",
            "landing_branch": "master",
            "landing_sha": revision,
            "tmux_session": session,
            "tmux_session_id": session_id,
            "tmux_window_id": window_id,
            "tmux_pane_id": pane_id,
            "created_at": created_at,
        }
        (directory / "job.json").write_text(
            json.dumps(record, sort_keys=True), encoding="utf-8"
        )
        (directory / "status").write_bytes(status)
        (directory / "report.md").write_bytes(report)
        return directory, worktree, record

    def test_direct_human_json_all_detail_and_report(self) -> None:
        live_dir, worktree, _ = self.create_job(
            "abc123def456",
            "live-feature",
            status=b"working: ready\nreview-ready: review me\n",
        )
        self.create_job("000000000000", "old-feature", alive=False)

        human = self.cli().stdout
        self.assertIn("abc123def456", human)
        self.assertIn("review me", human)
        self.assertNotIn("000000000000", human)
        all_jobs = json.loads(self.cli("--all", "--json").stdout)
        self.assertEqual(
            [job["job_id"] for job in all_jobs["jobs"]],
            ["000000000000", "abc123def456"],
        )
        self.assertEqual(all_jobs["jobs"][0]["pane_liveness"], "dead")
        self.assertEqual(all_jobs["jobs"][1]["pane_liveness"], "alive")

        before = {
            path: (path.stat().st_mtime_ns, stat.S_IMODE(path.stat().st_mode))
            for path in (
                live_dir / "job.json",
                live_dir / "status",
                live_dir / "report.md",
                worktree / ".git" / "index",
            )
        }
        detail = json.loads(self.cli("abc123def456", "--report", "--json").stdout)
        self.assertEqual(detail["state"], "review-ready")
        self.assertEqual(detail["metadata"]["review"]["profile"], "code")
        self.assertEqual(detail["status"]["events"][-1], "review-ready: review me")
        self.assertEqual(detail["report"]["content"], "# Fixture report\n")
        self.assertEqual(detail["git"]["branch"], "live-feature")
        self.assertTrue(detail["git"]["clean"])
        self.assertEqual(
            detail["git"]["revision"], self.git(worktree, "rev-parse", "HEAD")
        )
        self.assertTrue(detail["git"]["recorded_landing_revision_valid"])
        after = {
            path: (path.stat().st_mtime_ns, stat.S_IMODE(path.stat().st_mode))
            for path in before
        }
        self.assertEqual(after, before)
        self.assertNotIn("Fixture report", self.cli("abc123def456").stdout)
        self.assertIn("Report content:", self.cli("abc123def456", "--report").stdout)

    def test_malformed_bounded_and_stale_diagnostics(self) -> None:
        directory, worktree, record = self.create_job(
            "111111111111",
            "broken-feature",
            status=(
                b"working: accepted\n"
                b"mystery: rejected\n"
                b"working: crlf\r\n"
                b"working: incomplete"
            ),
        )
        (worktree / "dirty.txt").write_text("dirty\n", encoding="utf-8")
        record["landing_sha"] = "0" * 40
        (directory / "job.json").write_text(json.dumps(record), encoding="utf-8")
        (directory / "reviewer-aaa111bbb222.json").write_text("{}\n", encoding="utf-8")
        detail = json.loads(self.cli("111111111111", "--json").stdout)
        self.assertEqual(detail["status"]["events"], ["working: accepted"])
        self.assertIsNone(detail["reviewer"])
        self.assertFalse(detail["git"]["clean"])
        self.assertFalse(detail["git"]["recorded_landing_revision_valid"])
        diagnostics = "\n".join(detail["diagnostics"])
        self.assertIn("invalid grammar", diagnostics)
        self.assertIn("uses CRLF", diagnostics)
        self.assertIn("incomplete final line", diagnostics)
        self.assertIn("recorded landing revision is missing", diagnostics)
        self.assertIn(
            "reviewer: ownership and session evidence do not match", diagnostics
        )

        recorded_window = record["tmux_window_id"]
        record["tmux_window_id"] = "@999999"
        (directory / "job.json").write_text(json.dumps(record), encoding="utf-8")
        mismatch = json.loads(self.cli("111111111111", "--json").stdout)
        self.assertEqual(mismatch["pane_liveness"], "identity-mismatch")
        record["tmux_window_id"] = recorded_window
        (directory / "job.json").write_text(json.dumps(record), encoding="utf-8")

        malformed = self.jobs / "222222222222"
        malformed.mkdir()
        (malformed / "job.json").write_text("not json", encoding="utf-8")
        oversized_record = self.jobs / "444444444444"
        oversized_record.mkdir()
        (oversized_record / "job.json").write_bytes(b"x" * (64 * 1024 + 1))
        linked_record = self.jobs / "555555555555"
        linked_record.mkdir()
        (linked_record / "job.json").symlink_to(directory / "job.json")
        all_jobs = json.loads(self.cli("--all", "--json").stdout)
        malformed_jobs = {
            job["job_id"]: job for job in all_jobs["jobs"] if not job["valid"]
        }
        self.assertIn("invalid JSON", malformed_jobs["222222222222"]["diagnostics"][0])
        self.assertIn("exceeds", malformed_jobs["444444444444"]["diagnostics"][0])
        self.assertIn(
            "cannot open regular file",
            malformed_jobs["555555555555"]["diagnostics"][0],
        )
        exact = self.cli("222222222222", "--json", check=False)
        self.assertEqual(exact.returncode, 1)
        self.assertIn("invalid JSON", json.loads(exact.stdout)["error"])

        (directory / "status").write_bytes(b"x" * (256 * 1024 + 1))
        (directory / "report.md").write_bytes(b"x" * (1024 * 1024 + 1))
        oversized = json.loads(self.cli("111111111111", "--report", "--json").stdout)
        self.assertIsNone(oversized["report"]["content"])
        oversized_diagnostics = "\n".join(oversized["diagnostics"])
        self.assertIn("status: exceeds", oversized_diagnostics)
        self.assertIn("report.md: exceeds", oversized_diagnostics)

    def test_symlinks_dead_identity_missing_worktree_and_invalid_forms(self) -> None:
        directory, worktree, _ = self.create_job("333333333333", "symlink-feature")
        status = directory / "status"
        status.unlink()
        status.symlink_to(directory / "report.md")
        shutil.rmtree(worktree)
        detail = json.loads(self.cli("333333333333", "--json").stdout)
        diagnostics = "\n".join(detail["diagnostics"])
        self.assertIn("cannot open regular file", diagnostics)
        self.assertIn("worktree: missing", diagnostics)

        for arguments in (
            ("--report",),
            ("--all", "333333333333"),
            ("333333333333", "extra"),
            ("--state-root", str(self.state)),
        ):
            result = self.cli(*arguments, check=False)
            self.assertEqual(result.returncode, 2, arguments)
        invalid_id = self.cli("short", check=False)
        self.assertEqual(invalid_id.returncode, 1)

        help_text = self.cli("--help").stdout
        self.assertIn("Private read-only diagnostics", help_text)
        self.assertIn("No path, tmux", help_text)
        self.assertIn("target, or state-root input", help_text)


if __name__ == "__main__":
    unittest.main()
