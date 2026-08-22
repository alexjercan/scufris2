from __future__ import annotations

import datetime as dt
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
CLI = REPOSITORY / "scripts" / "scufris-jobs-prune"
KNOWN_FILES = ("job.json", "prompt.md", "report.md", "status")


class ScufrisJobsPruneIntegrationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="scufris-prune-test-")
        self.root = Path(self.temporary.name)
        self.state = self.root / "state"
        self.jobs = self.state / "scufris" / "jobs"
        self.jobs.mkdir(parents=True)
        self.tmux_root = self.root / "tmux"
        self.tmux_root.mkdir()
        self.env = os.environ.copy()
        self.env.pop("TMUX", None)
        self.env.update(
            {
                "XDG_STATE_HOME": str(self.state),
                "TMUX_TMPDIR": str(self.tmux_root),
            }
        )
        self.sessions: list[str] = []
        self.default_server = self.default_tmux_server()

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
        self, argv: list[str], *, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            argv,
            env=self.env,
            text=True,
            capture_output=True,
            check=check,
            timeout=20,
        )

    def cli(
        self,
        *arguments: str,
        check: bool = True,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            [str(CLI), *arguments],
            env=env or self.env,
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

    def tmux_identity(self, job_id: str, *, alive: bool) -> tuple[str, str, str, str]:
        session = f"fixture_{job_id}"
        identity = self.external(
            [
                "tmux",
                "new-session",
                "-d",
                "-P",
                "-F",
                "#{session_id}\t#{window_id}\t#{pane_id}",
                "-s",
                session,
                "-n",
                f"job-{job_id}",
                "sleep 60",
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
                pane_dead = self.external(
                    ["tmux", "display-message", "-p", "-t", pane_id, "#{pane_dead}"]
                ).stdout.strip()
                if pane_dead == "1":
                    break
                time.sleep(0.02)
            else:
                self.fail("fixture pane did not become dead")
        return session_id, window_id, pane_id, session

    def old_created_at(self, *, days: int = 31, seconds: int = 0) -> str:
        created = dt.datetime.now(dt.timezone.utc) - dt.timedelta(
            days=days, seconds=seconds
        )
        return created.strftime("%Y-%m-%dT%H:%M:%SZ")

    def create_job(
        self,
        job_id: str,
        *,
        alive: bool | None = None,
        created_at: str | None = None,
        malformed: bool = False,
        directory_age_seconds: int | None = None,
    ) -> tuple[Path, dict[str, Any] | None]:
        directory = self.jobs / job_id
        directory.mkdir()
        record: dict[str, Any] | None = None
        if malformed:
            (directory / "job.json").write_text("not json\n", encoding="utf-8")
        else:
            if alive is None:
                identity = ("$999999", "@999999", "%999999", f"fixture_{job_id}")
            else:
                identity = self.tmux_identity(job_id, alive=alive)
            session_id, window_id, pane_id, session = identity
            record = {
                "version": 2,
                "job_id": job_id,
                "harness": "pi",
                "model": "openai-codex/test-model",
                "thinking": "medium",
                "feature": f"fixture-{job_id}",
                "cleanup": "remove",
                "review": {"profile": "none"},
                "project": "current",
                "landing_branch": "master",
                "landing_sha": "1" * 40,
                "tmux_session": session,
                "tmux_session_id": session_id,
                "tmux_window_id": window_id,
                "tmux_pane_id": pane_id,
                "created_at": created_at or self.old_created_at(),
            }
            (directory / "job.json").write_text(
                json.dumps(record, sort_keys=True), encoding="utf-8"
            )
        (directory / "prompt.md").write_text(
            "secret prompt fixture\n", encoding="utf-8"
        )
        (directory / "report.md").write_text(
            "secret report fixture\n", encoding="utf-8"
        )
        (directory / "status").write_text("done: fixture complete\n", encoding="utf-8")
        if directory_age_seconds is not None:
            modified = time.time() - directory_age_seconds
            os.utime(directory, (modified, modified))
        return directory, record

    def tree_snapshot(self) -> dict[str, tuple[int, int, bytes]]:
        snapshot: dict[str, tuple[int, int, bytes]] = {}
        for path in sorted(self.jobs.rglob("*")):
            info = path.lstat()
            content = path.read_bytes() if stat.S_ISREG(info.st_mode) else b""
            snapshot[str(path.relative_to(self.jobs))] = (
                info.st_mode,
                info.st_mtime_ns,
                content,
            )
        return snapshot

    def test_default_preview_is_write_free_deterministic_and_bounded(self) -> None:
        self.create_job("111111111111", alive=False)
        self.create_job("222222222222", alive=True)
        self.create_job(
            "333333333333", malformed=True, directory_age_seconds=31 * 86400
        )
        self.create_job("444444444444", malformed=True, directory_age_seconds=300)
        (self.jobs / "invalid-name").write_text("leave me\n", encoding="utf-8")
        before = self.tree_snapshot()

        first = self.cli().stdout
        locale_env = self.env.copy()
        locale_env["LC_ALL"] = "C"
        second = self.cli(env=locale_env).stdout

        self.assertEqual(first, second)
        self.assertEqual(self.tree_snapshot(), before)
        self.assertIn("candidate 111111111111: valid record; exact pane is dead", first)
        self.assertIn("refuse 222222222222: exact recorded pane is alive", first)
        self.assertIn("candidate 333333333333: malformed record", first)
        self.assertIn("refuse 444444444444: malformed record is newer", first)
        self.assertIn('refuse root entry "invalid-name": name is not an exact', first)
        self.assertIn("Preview: 2 candidates, 3 refused; no changes made.", first)
        self.assertNotIn("secret prompt", first)
        self.assertNotIn("secret report", first)

    def test_apply_zero_deletes_only_dead_and_old_malformed_metadata(self) -> None:
        dead_directory, dead_record = self.create_job("111111111111", alive=False)
        (dead_directory / "reviewer-aaa111bbb222.jsonl").write_text(
            '{"type":"session"}\n', encoding="utf-8"
        )
        old_malformed, _ = self.create_job(
            "222222222222", malformed=True, directory_age_seconds=3605
        )
        live_directory, live_record = self.create_job("333333333333", alive=True)
        recent_malformed, _ = self.create_job(
            "444444444444", malformed=True, directory_age_seconds=3590
        )
        unknown_directory, _ = self.create_job("555555555555", alive=False)
        (unknown_directory / "status").unlink()
        (unknown_directory / "unknown").write_text("refuse\n", encoding="utf-8")
        outside = self.root / "outside"
        outside.mkdir()
        marker = outside / "marker"
        marker.write_text("keep\n", encoding="utf-8")
        (self.jobs / "666666666666").symlink_to(outside, target_is_directory=True)
        mismatch_directory, mismatch_record = self.create_job(
            "777777777777", alive=False
        )
        assert mismatch_record is not None
        mismatch_record["tmux_window_id"] = "@999999"
        (mismatch_directory / "job.json").write_text(
            json.dumps(mismatch_record, sort_keys=True), encoding="utf-8"
        )

        repository = self.root / "repository"
        repository.mkdir()
        self.external(["git", "init", "-b", "master", str(repository)])
        self.external(
            [
                "git",
                "-C",
                str(repository),
                "-c",
                "user.name=Scufris Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "fixture",
            ]
        )
        self.external(["git", "-C", str(repository), "branch", "retained"])
        refs_before = self.external(
            ["git", "-C", str(repository), "show-ref", "--heads"]
        ).stdout
        worktrees_before = self.external(
            ["git", "-C", str(repository), "worktree", "list", "--porcelain"]
        ).stdout

        result = self.cli("--older-than-days", "0", "--apply")

        self.assertFalse(dead_directory.exists())
        self.assertFalse(old_malformed.exists())
        self.assertTrue(live_directory.is_dir())
        self.assertTrue(recent_malformed.is_dir())
        self.assertTrue(unknown_directory.is_dir())
        self.assertTrue((self.jobs / "666666666666").is_symlink())
        self.assertTrue(mismatch_directory.is_dir())
        self.assertEqual(marker.read_text(encoding="utf-8"), "keep\n")
        self.assertIn("deleted 111111111111", result.stdout)
        self.assertIn("deleted 222222222222", result.stdout)
        self.assertIn(
            "refuse 333333333333: exact recorded pane is alive", result.stdout
        )
        self.assertIn("one-hour grace boundary", result.stdout)
        self.assertIn('unknown "unknown"', result.stdout)
        self.assertIn("cannot open safely", result.stdout)
        self.assertIn("recorded pane identity does not match exactly", result.stdout)
        self.assertIn("Apply: 2 deleted, 5 refused, 0 errors.", result.stdout)
        for record in (dead_record, live_record):
            assert record is not None
            pane = self.external(
                [
                    "tmux",
                    "display-message",
                    "-p",
                    "-t",
                    record["tmux_pane_id"],
                    "#{pane_id}",
                ]
            ).stdout.strip()
            self.assertEqual(pane, record["tmux_pane_id"])
        self.assertEqual(
            self.external(["git", "-C", str(repository), "show-ref", "--heads"]).stdout,
            refs_before,
        )
        self.assertEqual(
            self.external(
                ["git", "-C", str(repository), "worktree", "list", "--porcelain"]
            ).stdout,
            worktrees_before,
        )

    def test_age_grace_and_name_boundaries(self) -> None:
        self.create_job(
            "111111111111",
            alive=None,
            created_at=self.old_created_at(days=30, seconds=2),
        )
        self.create_job(
            "222222222222",
            alive=None,
            created_at=self.old_created_at(days=29, seconds=86390),
        )
        self.create_job("333333333333", malformed=True, directory_age_seconds=3602)
        self.create_job("444444444444", malformed=True, directory_age_seconds=3590)
        linked_directory, _ = self.create_job("555555555555", alive=None)
        prompt = linked_directory / "prompt.md"
        prompt.unlink()
        prompt.symlink_to(self.root / "outside-prompt")
        missing_directory, _ = self.create_job("666666666666", alive=None)
        (missing_directory / "status").unlink()

        retained = self.cli("--older-than-days", "30").stdout
        zero_days = self.cli("--older-than-days", "0").stdout

        self.assertIn("candidate 111111111111", retained)
        self.assertIn("refuse 222222222222: valid record is newer", retained)
        self.assertIn("candidate 333333333333", zero_days)
        self.assertIn("refuse 444444444444: malformed record is newer", zero_days)
        self.assertIn("prompt.md: not a regular non-symlink file", retained)
        self.assertIn("missing status", retained)

    def test_apply_refuses_a_toctou_change_before_any_deletion(self) -> None:
        directory, _ = self.create_job("111111111111", alive=False)
        real_tmux = shutil.which("tmux")
        self.assertIsNotNone(real_tmux)
        wrapper_bin = self.root / "race-bin"
        wrapper_bin.mkdir()
        wrapper = wrapper_bin / "tmux"
        wrapper.write_text(
            "#!/usr/bin/env python3\n"
            "import os, pathlib, sys\n"
            "target = pathlib.Path(os.environ['SCUFRIS_RACE_DIRECTORY'])\n"
            "marker = target / 'unknown-during-check'\n"
            "if sys.argv[1:2] == ['display-message'] and not marker.exists():\n"
            "    marker.write_text('race\\n', encoding='utf-8')\n"
            f"os.execv({real_tmux!r}, [{real_tmux!r}, *sys.argv[1:]])\n",
            encoding="utf-8",
        )
        wrapper.chmod(0o755)
        race_env = self.env.copy()
        race_env["PATH"] = f"{wrapper_bin}:{race_env['PATH']}"
        race_env["SCUFRIS_RACE_DIRECTORY"] = str(directory)

        result = self.cli("--older-than-days", "0", "--apply", env=race_env)

        self.assertIn("candidate 111111111111", result.stdout)
        self.assertIn(
            "refuse 111111111111: changed after candidate scan", result.stdout
        )
        self.assertIn("Apply: 0 deleted, 1 refused, 0 errors.", result.stdout)
        self.assertTrue(directory.is_dir())
        for name in KNOWN_FILES:
            self.assertTrue((directory / name).is_file())
        self.assertTrue((directory / "unknown-during-check").is_file())

    def test_help_arguments_and_jobs_root_symlink_fail_closed(self) -> None:
        help_text = self.cli("--help").stdout
        self.assertIn("delete it only with --apply", help_text)
        self.assertIn("one-hour grace", help_text)
        self.assertIn("conventional jobs root", help_text)
        for arguments in (
            ("--older-than-days", "-1"),
            ("--older-than-days", "36501"),
            ("--older-than-days", "1.5"),
            ("--state-root", str(self.state)),
            ("111111111111",),
        ):
            result = self.cli(*arguments, check=False)
            self.assertEqual(result.returncode, 2, arguments)

        outside = self.root / "outside-jobs"
        outside.mkdir()
        marker = outside / "marker"
        marker.write_text("keep\n", encoding="utf-8")
        self.jobs.rmdir()
        self.jobs.symlink_to(outside, target_is_directory=True)
        refused = self.cli("--apply", check=False)
        self.assertEqual(refused.returncode, 1)
        self.assertIn("jobs root: cannot open safely", refused.stderr)
        self.assertEqual(marker.read_text(encoding="utf-8"), "keep\n")


if __name__ == "__main__":
    unittest.main()
