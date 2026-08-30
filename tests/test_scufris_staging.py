"""What the combined and split staging commands arrange and leave alone.

The two binaries are stubs. What is under test is the script: the environment
it hands each process, the staging root it seeds, backend and frontend locks,
and a Ctrl+C that stops exactly the processes that command started. Running the real
service and companion needs a display and a Pi login, so that run is recorded
by hand under the task rather than here.

Every path this touches is inside a temporary directory, including `HOME` and
`XDG_RUNTIME_DIR`, so a test that got isolation wrong fails rather than writing
into the deployed Scufris.
"""

from __future__ import annotations

import json
import os
import shutil
import signal
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[1]
SCRIPT = REPOSITORY / "scripts" / "scufris-staging"

#: Writes what it was given, makes the socket, and waits to be stopped. The
#: environment dump is how the test reads what the script exported.
STUB = """#!/usr/bin/env python3
import json, os, signal, sys, time
from pathlib import Path

role = sys.argv[0].rsplit("/", 1)[-1]
if "--print-config" in sys.argv:
    print("socket=" + os.environ["SCUFRIS_RUNTIME_DIR"] + "/surface.sock")
    print("command_socket=" + os.environ.get("SCUFRIS_DESKTOP_COMMAND_SOCKET", "default"))
    raise SystemExit(0)

report = Path(os.environ["SCUFRIS_STAGING_REPORT"])
report.mkdir(parents=True, exist_ok=True)
profile = os.environ.get("SCUFRIS_STAGING_FRONTEND")
suffix = "-" + profile if role == "desktop" and profile else ""
(report / (role + suffix + ".json")).write_text(
    json.dumps({"pid": os.getpid(), "env": dict(os.environ)})
)
if role == "service":
    Path(os.environ["SCUFRIS_RUNTIME_DIR"], "surface.sock").touch()
if role == "desktop":
    Path(os.environ["SCUFRIS_DESKTOP_COMMAND_SOCKET"]).touch()

signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
while True:
    time.sleep(0.05)
"""


def wait_for(predicate, timeout: float = 20.0) -> bool:
    """True as soon as `predicate` holds, False if it never does in time."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


@unittest.skipUnless(shutil.which("flock"), "flock is not installed")
@unittest.skipUnless(shutil.which("git"), "git is not installed")
class StagingTests(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="scufris-staging-test-"))
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        self.home = self.root / "home"
        self.runtime = self.root / "run"
        self.staging = self.root / "staging"
        self.report = self.root / "report"
        for directory in (self.home, self.runtime):
            directory.mkdir(parents=True)

        # The deployed Pi directory this staging run seeds itself from.
        deployed = self.home / ".pi" / "agent"
        deployed.mkdir(parents=True)
        (deployed / "auth.json").write_text('{"token": "deployed"}\n')
        (deployed / "settings.json").write_text('{"model": "deployed"}\n')
        self.deployed_pi_agent = deployed

        self.bin = self.root / "bin"
        self.bin.mkdir()
        for role in ("service", "desktop"):
            stub = self.bin / role
            stub.write_text(STUB)
            stub.chmod(0o755)
        self.speaker = self.bin / "speak"
        self.speaker.write_text("#!/bin/sh\nexit 0\n")
        self.speaker.chmod(0o755)

    def environment(self, **named: str) -> dict[str, str]:
        env = os.environ.copy()
        env.update(
            {
                "HOME": str(self.home),
                "XDG_RUNTIME_DIR": str(self.runtime),
                "SCUFRIS_STAGING_ROOT": str(self.staging),
                "SCUFRIS_STAGING_SERVICE": str(self.bin / "service"),
                "SCUFRIS_STAGING_DESKTOP": str(self.bin / "desktop"),
                "SCUFRIS_STAGING_REPORT": str(self.report),
            }
        )
        for name in (
            "XDG_STATE_HOME",
            "XDG_DATA_HOME",
            "PI_CODING_AGENT_DIR",
            # A dev shell exports these, and the script reads them as leave to
            # use the working tree's synthesiser. Dropped so a test says what
            # it means rather than what the shell around it happens to hold.
            "SCUFRIS_PIPER_MODEL",
            "SCUFRIS_PIPER_CONFIG",
            "SCUFRIS_DESKTOP_SPEAK_COMMAND",
            "SCUFRIS_STAGING_SPEAK",
        ):
            env.pop(name, None)
        env.update(named)
        return env

    def start(self, *arguments: str, **named: str) -> subprocess.Popen[str]:
        """Starts one staging command and stops it when the test ends."""
        started = subprocess.Popen(
            [str(SCRIPT), *arguments],
            env=self.environment(**named),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            # Its own group, so the SIGINT this test sends is the one the
            # script's trap sees and not one the test runner also gets.
            start_new_session=True,
        )
        self.addCleanup(self.stop, started)
        return started

    def up(self, **named: str) -> subprocess.Popen[str]:
        return self.start("up", **named)

    def stop(self, started: subprocess.Popen[str]) -> None:
        if started.poll() is None:
            started.send_signal(signal.SIGINT)
            try:
                started.wait(timeout=10)
            except subprocess.TimeoutExpired:
                started.kill()
                started.wait(timeout=10)
        if started.stdout is not None and not started.stdout.closed:
            started.stdout.close()

    def reported(self, role: str, profile: str | None = None) -> dict:
        suffix = f"-{profile}" if role == "desktop" and profile else ""
        return json.loads((self.report / f"{role}{suffix}.json").read_text())

    def started_or_fail(self, running: subprocess.Popen[str], *roles: str) -> None:
        """Waits for every `role` to report in, or fails with what the run said.

        Every role, not the first one: the two are started together and either
        can write first, so a test that read one report after waiting for the
        other would pass or fail by timing.

        The output is read only on the failing path. Reading it while the
        stack is up waits for an end that a working run does not have.
        """
        if wait_for(
            lambda: all((self.report / f"{role}.json").is_file() for role in roles)
        ):
            return
        self.stop(running)
        self.fail(f"the stack never started: {running.communicate()[0]}")

    def test_the_stack_runs_on_its_own_paths_and_leaves_the_deployed_one_alone(
        self,
    ) -> None:
        started = self.up()
        self.started_or_fail(started, "service", "desktop")

        for role in ("service", "desktop"):
            env = self.reported(role)["env"]
            self.assertEqual(
                env["SCUFRIS_RUNTIME_DIR"], str(self.runtime / "scufris-staging")
            )
            self.assertEqual(env["XDG_STATE_HOME"], str(self.staging / "state"))
            self.assertEqual(env["XDG_DATA_HOME"], str(self.staging / "data"))
            self.assertEqual(env["PI_CODING_AGENT_DIR"], str(self.staging / "pi-agent"))
            self.assertEqual(
                env["SCUFRIS_PROJECT_ROOTS"], f'["{self.staging / "projects"}"]'
            )
            self.assertEqual(
                env["SCUFRIS_SERVICE_AGENT"],
                str(REPOSITORY / "scripts" / "scufris-agent"),
            )
            # Not Super+D. The deployed companion keeps its own activation key.
            self.assertEqual(env["SCUFRIS_DESKTOP_HOTKEY"], "Super+G")
            # With no synthesiser anywhere staging is silent, and says so
            # rather than assembling a voice a deployment would not have.
            self.assertNotIn("SCUFRIS_DESKTOP_SPEAK_COMMAND", env)

        # The one directory the deployed Scufris looks in was never made.
        self.assertFalse((self.runtime / "scufris").exists())
        self.assertFalse((self.home / ".local" / "state" / "scufris").exists())
        self.assertFalse((self.home / ".local" / "share" / "scufris").exists())
        # Nor was anything of the deployed Pi directory rewritten.
        self.assertEqual(
            (self.deployed_pi_agent / "settings.json").read_text(),
            '{"model": "deployed"}\n',
        )

    def test_the_companion_is_given_a_synthesiser_when_one_can_be_found(self) -> None:
        """Speech is the companion's, so this is the one process that gets it.

        Three sources in one order: a command already named, then the packaged
        one the flake wrapper points at, then the working tree's helper where a
        dev shell has bound the Piper paths it reads.
        """
        packaged = self.bin / "packaged-speak"
        packaged.write_text("#!/bin/sh\nexit 0\n")
        packaged.chmod(0o755)

        started = self.up(SCUFRIS_STAGING_SPEAK=str(self.speaker))
        self.started_or_fail(started, "service", "desktop")
        for role in ("service", "desktop"):
            self.assertEqual(
                self.reported(role)["env"]["SCUFRIS_DESKTOP_SPEAK_COMMAND"],
                str(self.speaker),
            )

        # A command already in the environment is the person saying so, and it
        # outranks what the wrapper names.
        self.stop(started)
        shutil.rmtree(self.report)
        started = self.up(
            SCUFRIS_STAGING_SPEAK=str(packaged),
            SCUFRIS_DESKTOP_SPEAK_COMMAND=str(self.speaker),
        )
        self.started_or_fail(started, "desktop")
        self.assertEqual(
            self.reported("desktop")["env"]["SCUFRIS_DESKTOP_SPEAK_COMMAND"],
            str(self.speaker),
        )

        # And with only the Piper paths bound, the working tree's own helper.
        self.stop(started)
        shutil.rmtree(self.report)
        started = self.up(
            SCUFRIS_PIPER_MODEL="/pinned/model.onnx",
            SCUFRIS_PIPER_CONFIG="/pinned/model.onnx.json",
        )
        self.started_or_fail(started, "desktop")
        self.assertEqual(
            self.reported("desktop")["env"]["SCUFRIS_DESKTOP_SPEAK_COMMAND"],
            str(REPOSITORY / "tools" / "voice" / "scufris-speak"),
        )

    def test_a_synthesiser_that_cannot_be_run_is_refused_rather_than_ignored(
        self,
    ) -> None:
        """A voice that never arrives is the hardest fault to see.

        The companion logs a speak command it cannot run once and stays silent,
        which looks exactly like the silent default. Refusing here names it.
        """
        missing = self.bin / "not-a-speaker"
        refused = subprocess.run(
            [str(SCRIPT), "up"],
            env=self.environment(SCUFRIS_STAGING_SPEAK=str(missing)),
            capture_output=True,
            text=True,
            # The refusal is what is under test.
            check=False,
        )
        self.assertEqual(refused.returncode, 2, refused.stderr)
        self.assertIn("not executable", refused.stderr)

    def test_a_fresh_root_is_seeded_with_a_project_and_a_pi_directory(self) -> None:
        started = self.up()
        self.started_or_fail(started, "service")

        seeded = self.staging / "projects" / "hello"
        self.assertTrue((seeded / "README.md").is_file())
        log = subprocess.run(
            ["git", "-C", str(seeded), "log", "--oneline"],
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(len(log.stdout.splitlines()), 1, log.stdout)

        pi_agent = self.staging / "pi-agent"
        # The login is shared, so a token refreshed on either side is refreshed
        # for both. The settings are a copy, so staging can be pointed
        # somewhere else without editing the deployed file.
        self.assertTrue((pi_agent / "auth.json").is_symlink())
        self.assertEqual(
            (pi_agent / "auth.json").resolve(),
            (self.deployed_pi_agent / "auth.json").resolve(),
        )
        self.assertFalse((pi_agent / "settings.json").is_symlink())
        self.assertEqual(
            (pi_agent / "settings.json").read_text(), '{"model": "deployed"}\n'
        )

    def test_a_second_stack_is_refused_while_one_is_running(self) -> None:
        started = self.up()
        self.started_or_fail(started, "service")

        second = subprocess.run(
            [str(SCRIPT), "up"],
            env=self.environment(),
            capture_output=True,
            text=True,
            # The refusal is what is under test.
            check=False,
        )
        self.assertEqual(second.returncode, 3, second.stderr)
        self.assertIn("already running", second.stderr)
        # And it refused before starting anything, so the first stack is
        # still the one holding the socket.
        self.assertIsNone(started.poll())

    def test_an_interrupt_stops_both_processes_and_ends_the_run(self) -> None:
        started = self.up()
        self.started_or_fail(started, "service", "desktop")
        pids = [self.reported(role)["pid"] for role in ("service", "desktop")]

        started.send_signal(signal.SIGINT)
        output = started.communicate(timeout=20)[0]
        # Ctrl+C is the teardown, not a failure.
        self.assertEqual(started.returncode, 0, output)
        for pid in pids:
            self.assertFalse(alive(pid), f"{pid} outlived the run:\n{output}")
        self.assertFalse(
            (self.runtime / "scufris-staging" / "desktop.sock").exists()
        )
        # The lock is released with the process, so the next run is not
        # refused by a file the last one left behind.
        shutil.rmtree(self.report)
        self.started_or_fail(self.up(), "service")

    def test_backend_and_named_frontends_can_run_independently(self) -> None:
        backend = self.start("backend")
        self.started_or_fail(backend, "service")
        self.assertFalse((self.report / "desktop.json").exists())

        one = self.start("frontend", "one")
        two = self.start(
            "frontend",
            "two",
            SCUFRIS_DESKTOP_HOTKEY="Super+H",
        )
        self.started_or_fail(one, "desktop-one")
        self.started_or_fail(two, "desktop-two")

        one_env = self.reported("desktop", "one")["env"]
        two_env = self.reported("desktop", "two")["env"]
        self.assertEqual(one_env["SCUFRIS_RUNTIME_DIR"], two_env["SCUFRIS_RUNTIME_DIR"])
        self.assertEqual(
            one_env["XDG_STATE_HOME"],
            str(self.staging / "frontends" / "one" / "state"),
        )
        self.assertEqual(
            two_env["XDG_STATE_HOME"],
            str(self.staging / "frontends" / "two" / "state"),
        )
        self.assertNotEqual(one_env["XDG_STATE_HOME"], two_env["XDG_STATE_HOME"])
        self.assertEqual(
            one_env["SCUFRIS_DESKTOP_COMMAND_SOCKET"],
            str(self.runtime / "scufris-staging" / "desktop-one.sock"),
        )
        self.assertEqual(
            two_env["SCUFRIS_DESKTOP_COMMAND_SOCKET"],
            str(self.runtime / "scufris-staging" / "desktop-two.sock"),
        )
        self.assertEqual(two_env["SCUFRIS_DESKTOP_HOTKEY"], "Super+H")

        refused = subprocess.run(
            [str(SCRIPT), "frontend", "one"],
            env=self.environment(),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(refused.returncode, 3, refused.stderr)
        self.assertIn("frontend one is already running", refused.stderr)
        self.assertIsNone(one.poll())
        self.assertIsNone(two.poll())

        self.stop(one)
        self.assertFalse(
            (self.runtime / "scufris-staging" / "desktop-one.sock").exists()
        )
        self.assertTrue(
            (self.runtime / "scufris-staging" / "desktop-two.sock").exists()
        )
        self.assertIsNone(backend.poll())
        self.assertIsNone(two.poll())

    def test_a_frontend_requires_a_backend_and_a_safe_name(self) -> None:
        absent = subprocess.run(
            [str(SCRIPT), "frontend", "one"],
            env=self.environment(),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(absent.returncode, 3, absent.stderr)
        self.assertIn("no staging backend is running", absent.stderr)

        unsafe = subprocess.run(
            [str(SCRIPT), "frontend", "../one"],
            env=self.environment(),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(unsafe.returncode, 2, unsafe.stderr)
        self.assertIn("frontend NAME must match", unsafe.stderr)


def alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


if __name__ == "__main__":
    unittest.main()
