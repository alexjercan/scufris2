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
CLI = REPOSITORY / "scripts" / "scufris-dashboard"

FAKE_DASHBOARDCTL = r"""#!/usr/bin/env python3
import json
import os
import pathlib
import sys
import time

log = os.environ.get("FAKE_DASHBOARD_LOG")
if log:
    with pathlib.Path(log).open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(sys.argv[1:]) + "\n")
mode = os.environ.get("FAKE_DASHBOARD_MODE", "success")
if mode == "sleep":
    time.sleep(30)
if mode == "oversized":
    sys.stdout.write("x" * 70000)
    raise SystemExit(0)
if mode == "malformed":
    print("not json")
    raise SystemExit(0)
if mode == "wrong-version":
    print(json.dumps({"version": 3, "status": "ok", "result": {}}))
    raise SystemExit(0)
if mode == "failed":
    print(json.dumps({
        "version": 2,
        "status": "failed",
        "error": {"code": "surface_not_found", "message": "surface is gone"},
    }))
    raise SystemExit(1)
command = sys.argv[1]
if command == "discover":
    result = {
        "widgets": [{
            "id": "cpu",
            "name": "CPU",
            "description": "Processor usage",
            "variants": [{"id": "full", "name": "Full"}],
            "options": [],
            "inputs": [],
        }]
    }
elif command == "open":
    result = {"surface_id": "surface-42", "instance_id": "instance-42"}
elif command == "list":
    result = {"surfaces": [{
        "surface_id": "surface-42",
        "widget_id": "cpu",
        "variant_id": "full",
        "presentation": "focus",
    }]}
else:
    result = {"surface_id": sys.argv[2]}
print(json.dumps({"version": 2, "status": "ok", "result": result}))
"""


class ScufrisDashboardIntegrationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="scufris-dashboard-")
        self.root = Path(self.temporary.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        dashboardctl = self.bin / "dashboardctl"
        dashboardctl.write_text(FAKE_DASHBOARDCTL, encoding="utf-8")
        dashboardctl.chmod(0o755)
        self.log = self.root / "argv.log"
        self.env = os.environ.copy()
        self.env.update(
            {
                "PATH": f"{self.bin}:{self.env['PATH']}",
                "FAKE_DASHBOARD_LOG": str(self.log),
            }
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def cli(
        self,
        command: str,
        request: dict[str, Any],
        *,
        mode: str = "success",
        timeout: float = 10,
    ) -> tuple[subprocess.CompletedProcess[str], dict[str, Any]]:
        env = self.env.copy()
        env["FAKE_DASHBOARD_MODE"] = mode
        result = subprocess.run(
            [str(CLI), command],
            input=json.dumps(request),
            env=env,
            text=True,
            capture_output=True,
            check=False,
            timeout=timeout,
        )
        return result, json.loads(result.stdout)

    def logged_argv(self) -> list[list[str]]:
        if not self.log.exists():
            return []
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_discover_returns_protocol_result(self) -> None:
        result, envelope = self.cli("discover", {})
        self.assertEqual(result.returncode, 0)
        self.assertTrue(envelope["ok"])
        self.assertEqual(envelope["result"]["widgets"][0]["id"], "cpu")
        self.assertEqual(self.logged_argv(), [["discover"]])

    def test_open_uses_literal_argument_array(self) -> None:
        options = {"label": "$(touch /tmp/scufris-dashboard-must-not-exist)"}
        Path("/tmp/scufris-dashboard-must-not-exist").unlink(missing_ok=True)
        result, envelope = self.cli(
            "open",
            {
                "widget_id": "cpu",
                "variant_id": "full",
                "options": options,
                "inputs": {},
                "presentation": "tile",
            },
        )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(envelope["result"]["surface_id"], "surface-42")
        argv = self.logged_argv()[0]
        self.assertEqual(argv[:4], ["open", "cpu", "--variant", "full"])
        self.assertEqual(json.loads(argv[argv.index("--options") + 1]), options)
        self.assertFalse(Path("/tmp/scufris-dashboard-must-not-exist").exists())

    def test_update_list_focus_and_close_use_fixed_verbs(self) -> None:
        requests = [
            ("update", {"surface_id": "surface-42", "presentation": "tile"}),
            ("list", {}),
            ("focus", {"surface_id": "surface-42"}),
            ("close", {"surface_id": "surface-42"}),
        ]
        for command, request in requests:
            result, envelope = self.cli(command, request)
            self.assertEqual(result.returncode, 0)
            self.assertTrue(envelope["ok"])
        self.assertEqual(
            self.logged_argv(),
            [
                ["update", "surface-42", "--presentation", "tile"],
                ["list"],
                ["focus", "surface-42"],
                ["close", "surface-42"],
            ],
        )

    def test_rejects_invalid_requests_before_dashboardctl(self) -> None:
        result, envelope = self.cli("update", {"surface_id": "surface-42"})
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(envelope["ok"])
        self.assertIn("requires inputs or presentation", envelope["error"])
        self.assertEqual(self.logged_argv(), [])

    def test_preserves_dashboard_error_code(self) -> None:
        result, envelope = self.cli(
            "close", {"surface_id": "surface-42"}, mode="failed"
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(envelope["error_code"], "surface_not_found")
        self.assertEqual(envelope["error"], "surface is gone")

    def test_rejects_protocol_mismatch_and_malformed_output(self) -> None:
        for mode, message in (
            ("wrong-version", "protocol version must be 2"),
            ("malformed", "invalid dashboardctl response"),
        ):
            result, envelope = self.cli("list", {}, mode=mode)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(message, envelope["error"])

    def test_bounds_response_and_runtime(self) -> None:
        result, envelope = self.cli("list", {}, mode="oversized")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exceeds 64 KiB", envelope["error"])

        started = time.monotonic()
        result, envelope = self.cli("list", {}, mode="sleep")
        elapsed = time.monotonic() - started
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("timed out", envelope["error"])
        self.assertLess(elapsed, 7)


if __name__ == "__main__":
    unittest.main()
