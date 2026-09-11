"""What `scufris-terminal` hands to Pi.

Both programs it runs are stubs. What is under test is the script: which
session the terminal starts on, what it does when the service says nothing,
and that the caller's own arguments and exit code survive.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[1]
SCRIPT = REPOSITORY / "scripts" / "scufris-terminal"
BASH = shutil.which("bash")

#: Writes the command line and the environment it was started with, so the
#: test reads exactly what the script chose.
PI_STUB = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path

Path(os.environ["TERMINAL_TEST_REPORT"]).write_text(
    json.dumps({"argv": sys.argv[1:], "env": dict(os.environ)})
)
raise SystemExit(int(os.environ.get("TERMINAL_TEST_PI_EXIT", "0")))
"""

CTL_STUB = """#!/usr/bin/env python3
import os, sys

if sys.argv[1:] != ["state"]:
    print("unexpected: " + " ".join(sys.argv[1:]), file=sys.stderr)
    raise SystemExit(2)
answer = os.environ.get("TERMINAL_TEST_STATE", "")
if answer:
    print(answer)
raise SystemExit(int(os.environ.get("TERMINAL_TEST_CTL_EXIT", "0")))
"""


@unittest.skipUnless(BASH, "bash is not installed")
class TerminalLauncherTests(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="scufris-terminal-test-"))
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name, body in (("pi", PI_STUB), ("scufris-ctl", CTL_STUB)):
            stub = self.bin / name
            stub.write_text(body)
            stub.chmod(0o755)
        self.report = self.root / "report.json"
        self.sessions = self.root / "sessions"
        self.sessions.mkdir()
        self.lineage = self.sessions / "live.jsonl"
        self.lineage.write_text('{"type":"session","id":"live"}\n')

    def run_launcher(
        self, *arguments: str, **named: str
    ) -> subprocess.CompletedProcess:
        # Only the stubs and the interpreter behind them. The deployed `pi` is
        # on the real PATH, and a test that reached it would start a terminal
        # rather than report what the script chose.
        env = os.environ.copy()
        env.update(
            {
                "PATH": f"{self.bin}:{Path(sys.executable).parent}",
                "TERMINAL_TEST_REPORT": str(self.report),
            }
        )
        env.update(named)
        env.pop("SCUFRIS_TERMINAL", None)
        return subprocess.run(
            [str(BASH), str(SCRIPT), *arguments],
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )

    def started(self) -> dict:
        return json.loads(self.report.read_text())

    def test_a_lineage_file_is_forked_into_this_directory(self) -> None:
        result = self.run_launcher(
            "--model",
            "opus",
            TERMINAL_TEST_STATE=f"idle\nholder: managed\nsessions: {self.sessions}\nlineage: {self.lineage}\n",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        started = self.started()
        self.assertEqual(
            started["argv"],
            [
                "--session-dir",
                str(self.sessions),
                "--fork",
                str(self.lineage),
                "--model",
                "opus",
            ],
        )
        self.assertEqual(started["env"]["SCUFRIS_TERMINAL"], "1")

    def test_without_a_lineage_the_terminal_writes_the_first_session(self) -> None:
        result = self.run_launcher(
            TERMINAL_TEST_STATE=f"idle\nholder: managed\nsessions: {self.sessions}\n",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.started()["argv"], ["--session-dir", str(self.sessions)])

    def test_an_unreadable_lineage_file_is_not_forked(self) -> None:
        missing = self.sessions / "gone.jsonl"
        result = self.run_launcher(
            TERMINAL_TEST_STATE=f"idle\nsessions: {self.sessions}\nlineage: {missing}\n",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.started()["argv"], ["--session-dir", str(self.sessions)])
        self.assertIn("catch-up", result.stderr)

    def test_a_service_that_does_not_answer_still_starts_pi(self) -> None:
        result = self.run_launcher(
            "hello",
            TERMINAL_TEST_STATE="cannot connect",
            TERMINAL_TEST_CTL_EXIT="1",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.started()["argv"], ["hello"])
        self.assertEqual(self.started()["env"]["SCUFRIS_TERMINAL"], "1")
        self.assertIn("did not answer", result.stderr)

    def test_a_session_the_caller_chose_is_left_alone(self) -> None:
        result = self.run_launcher(
            "--continue",
            TERMINAL_TEST_STATE=f"idle\nsessions: {self.sessions}\nlineage: {self.lineage}\n",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.started()["argv"], ["--continue"])

    def test_the_exit_code_of_pi_is_the_exit_code_of_the_launcher(self) -> None:
        result = self.run_launcher(
            TERMINAL_TEST_STATE=f"idle\nsessions: {self.sessions}\n",
            TERMINAL_TEST_PI_EXIT="3",
        )
        self.assertEqual(result.returncode, 3, result.stderr)

    def test_a_missing_pi_is_named_rather_than_guessed(self) -> None:
        (self.bin / "pi").unlink()
        result = self.run_launcher(TERMINAL_TEST_STATE="idle\n")
        self.assertEqual(result.returncode, 2)
        self.assertIn("pi is not on PATH", result.stderr)


if __name__ == "__main__":
    unittest.main()
