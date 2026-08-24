from __future__ import annotations

import json
import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

HELPER = (
    Path(__file__).parents[1]
    / "tools"
    / "quick-review-agent"
    / "scufris-quick-review-agent"
)
BASE = "1" * 40
REVISION = "2" * 40


class QuickReviewAgentTest(unittest.TestCase):
    def test_runs_one_pinned_extension_only_rpc_agent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repo"
            repository.mkdir()
            state = root / "state"
            record = root / "record.json"
            fake = root / "pi"
            fake.write_text(
                textwrap.dedent(
                    f"""\
                    #!/usr/bin/env python3
                    import json
                    import os
                    import sys
                    from pathlib import Path

                    command = json.loads(sys.stdin.readline())
                    Path(os.environ["FAKE_RECORD"]).write_text(json.dumps({{
                        "argv": sys.argv[1:],
                        "cwd": os.getcwd(),
                        "state": os.environ.get("QUICK_REVIEW_STATE_DIR"),
                        "command": command,
                    }}))
                    print(json.dumps({{"id": "open-review", "type": "response", "command": "prompt", "success": True}}), flush=True)
                    event = {{
                        "version": 1,
                        "outcome": "approved",
                        "repository": {json.dumps(str(repository))},
                        "baseRef": {json.dumps(BASE)},
                        "targetRef": {json.dumps(REVISION)},
                        "baseRevision": {json.dumps(BASE)},
                        "revision": {json.dumps(REVISION)},
                        "identity": "3" * 64,
                        "sections": 1,
                        "comments": [],
                        "overallComment": "",
                        "questions": [],
                        "artifact": "/tmp/walkthrough.md",
                        "state": "/tmp/state.json",
                        "completedAt": "2026-08-24T00:00:00.000Z",
                    }}
                    print(json.dumps({{"type": "message_end", "message": {{"customType": "quick-review-outcome", "details": event}}}}), flush=True)
                    """
                )
            )
            fake.chmod(0o700)
            request = {
                "repository": str(repository),
                "base_revision": BASE,
                "revision": REVISION,
                "model": "openai-codex/gpt-5.6-sol",
                "thinking": "medium",
                "state_dir": str(state),
            }
            env = os.environ.copy()
            env["SCUFRIS_QUICK_REVIEW_PI"] = str(fake)
            env["FAKE_RECORD"] = str(record)
            completed = subprocess.run(
                [str(HELPER)],
                input=(json.dumps(request) + "\n").encode(),
                env=env,
                check=True,
                capture_output=True,
                timeout=10,
            )
            messages = [json.loads(line) for line in completed.stdout.splitlines()]
            self.assertEqual(messages[0], {"type": "ready"})
            self.assertEqual(messages[1]["type"], "completed")
            self.assertEqual(messages[1]["event"]["revision"], REVISION)
            invocation = json.loads(record.read_text())
            self.assertEqual(invocation["cwd"], str(repository))
            self.assertEqual(invocation["state"], str(state))
            self.assertEqual(
                invocation["command"]["message"],
                f"/quick-review --base {BASE} --target {REVISION}",
            )
            argv = invocation["argv"]
            self.assertIn("--mode", argv)
            self.assertIn("rpc", argv)
            self.assertIn("--no-extensions", argv)
            self.assertIn("npm:@alexjercan/quick-review@0.1.1", argv)
            self.assertIn("--no-context-files", argv)
            self.assertEqual(argv.count("--extension"), 1)
            self.assertEqual(state.stat().st_mode & 0o777, 0o700)

    def test_rejects_unknown_request_fields(self) -> None:
        completed = subprocess.run(
            [str(HELPER)],
            input=b'{"unexpected":true}\n',
            check=False,
            capture_output=True,
            timeout=5,
        )
        self.assertEqual(completed.returncode, 1)
        self.assertIn(b"schema is invalid", completed.stderr)


if __name__ == "__main__":
    unittest.main()
