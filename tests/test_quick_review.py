import importlib.util
import io
import json
import sys
import threading
import time
import unittest
import urllib.request
from pathlib import Path
from unittest.mock import patch

TOOL = Path(__file__).parents[1] / "tools" / "quick-review" / "quick_review.py"
SPEC = importlib.util.spec_from_file_location("quick_review", TOOL)
assert SPEC and SPEC.loader
quick_review = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = quick_review
SPEC.loader.exec_module(quick_review)


def descriptor() -> dict[str, object]:
    revision = "2" * 40
    return {
        "type": "init",
        "version": 1,
        "document": {
            "title": "Safe <script>alert(1)</script>",
            "summary": "What was <built>.",
            "revision": revision,
            "baseRevision": "1" * 40,
            "files": 1,
            "added": 1,
            "removed": 1,
            "sections": [
                {
                    "id": "safe-change",
                    "importance": "critical",
                    "file": "src/safe.py",
                    "lines": "1-2",
                    "markdown": "A <literal> explanation.",
                    "diff": "@@ -1 +1 @@\n-old <tag>\n+new & safe\n context",
                    "prompt": "Verify <safety>.",
                }
            ],
            "warnings": ["Unsafe <widget> ignored"],
        },
        "state": {
            "version": 1,
            "identity": "a" * 64,
            "revision": revision,
            "sections": {"safe-change": "not-reviewed"},
            "viewed": {"safe-change": False},
            "questions": [],
            "comments": [],
            "changeRequests": [],
            "approved": False,
        },
    }


class QuickReviewTest(unittest.TestCase):
    def test_descriptor_and_render_escape_content_and_style_diff(self) -> None:
        value = quick_review.validate_init(descriptor())
        page = quick_review.render_page(value["document"], value["state"])
        self.assertNotIn("<script>alert", page)
        self.assertIn("&lt;script&gt;alert", page)
        self.assertIn('class="diff-line diff-hunk"', page)
        self.assertIn('class="diff-line diff-del"', page)
        self.assertIn('class="diff-line diff-add"', page)
        self.assertIn('class="diff-line diff-context"', page)
        self.assertNotIn('style="', page)
        self.assertNotIn("<form", page)
        self.assertIn(
            '<pre class="context-view" aria-label="Exact-revision file context" hidden><code></code></pre>',
            page,
        )
        self.assertIn(
            "view.querySelector('code').textContent=result.context", quick_review.JS
        )
        self.assertIn("fetch('action'", quick_review.JS)
        self.assertIn('data-action="add-comment"', page)
        self.assertIn('data-action="mark-viewed"', page)
        self.assertIn('data-action="reopen"', page)
        self.assertIn("Approve with comments", page)
        self.assertNotIn("Create follow-up task", page)
        self.assertIn("classList.toggle('viewed'", quick_review.JS)

    def test_anchored_comments_are_bounded_and_escaped(self) -> None:
        value = descriptor()
        value["state"]["comments"] = [
            {
                "id": "b" * 24,
                "sectionId": "safe-change",
                "file": "src/safe.py",
                "lines": "1-2",
                "body": "Note <img onerror=alert(1)>",
            }
        ]
        page = quick_review.render_page(
            quick_review.validate_init(value)["document"], value["state"]
        )
        self.assertIn("src/safe.py:1-2", page)
        self.assertIn("Note &lt;img onerror=alert(1)&gt;", page)
        self.assertNotIn("<img onerror", page)
        value["state"]["comments"][0]["file"] = "other.py"
        with self.assertRaisesRegex(ValueError, "comments"):
            quick_review.validate_init(value)

    def test_malformed_and_oversized_bridge_messages_fail(self) -> None:
        with self.assertRaisesRegex(ValueError, "malformed"):
            quick_review.read_json_line(io.BytesIO(b"not-json\n"))
        oversized = io.BytesIO(b'"' + b"a" * quick_review.MAX_LINE_BYTES + b'"\n')
        with self.assertRaisesRegex(ValueError, "exceeds"):
            quick_review.read_json_line(oversized)
        init_sized = io.BytesIO(
            b'"' + b"a" * (quick_review.MAX_LINE_BYTES + 1) + b'"\n'
        )
        self.assertIsInstance(
            quick_review.read_json_line(init_sized, quick_review.MAX_INIT_LINE_BYTES),
            str,
        )
        value = descriptor()
        value["document"]["sections"][0]["importance"] = "arbitrary"
        with self.assertRaisesRegex(ValueError, "values"):
            quick_review.validate_init(value)

    def test_browser_opener_receives_only_generated_loopback_url(self) -> None:
        opened: list[str] = []
        output = io.StringIO()
        source = (
            json.dumps(descriptor()) + "\n" + json.dumps({"type": "activate"}) + "\n"
        ).encode()
        activated = threading.Event()

        class FakeServer:
            def __init__(self, address: tuple[str, int], _handler: object) -> None:
                self.server_address = (address[0], 43123)

            def serve_forever(self) -> None:
                activated.wait(1)

            def server_close(self) -> None:
                return

            def request_shutdown(self) -> None:
                return

        def opener(url: str) -> None:
            opened.append(url)
            activated.set()

        with patch.object(quick_review, "ReviewServer", FakeServer):
            self.assertEqual(quick_review.serve(io.BytesIO(source), output, opener), 0)
        self.assertEqual(len(opened), 1)
        self.assertRegex(opened[0], r"^http://127\.0\.0\.1:43123/[A-Za-z0-9_-]+/$")
        ready = json.loads(output.getvalue())
        self.assertEqual(ready["url"], opened[0])

    def test_shutdown_does_not_block_result_for_an_accepted_action(self) -> None:
        value = quick_review.validate_init(descriptor())
        accepted = threading.Event()
        release = threading.Event()

        class DelayedBridge:
            def request(self, _action: dict[str, object]) -> dict[str, object]:
                accepted.set()
                release.wait(2)
                return {
                    "type": "result",
                    "id": "a" * 24,
                    "ok": True,
                    "state": value["state"],
                    "message": "Review updated.",
                }

        server = quick_review.ReviewServer(("127.0.0.1", 0), quick_review.Handler)
        server.document = value["document"]
        server.state = value["state"]
        server.token = "test-token"
        server.bridge = DelayedBridge()
        server.state_lock = threading.Lock()
        server.action_condition = threading.Condition()
        server.active_actions = 0
        server.shutting_down = False
        server.shutdown_thread = None
        serving = threading.Thread(target=server.serve_forever)
        serving.start()
        responses: list[tuple[int, dict[str, object]]] = []

        def post() -> None:
            request = urllib.request.Request(
                f"http://127.0.0.1:{server.server_port}/{server.token}/action",
                data=json.dumps(
                    {"action": "mark-viewed", "section": "safe-change"}
                ).encode(),
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with urllib.request.urlopen(request, timeout=3) as response:
                responses.append((response.status, json.load(response)))

        client = threading.Thread(target=post)
        client.start()
        self.assertTrue(accepted.wait(1))
        started = time.monotonic()
        server.request_shutdown()
        self.assertLess(time.monotonic() - started, 0.2)
        self.assertIsNotNone(server.shutdown_thread)
        release.set()
        client.join(3)
        serving.join(3)
        server.shutdown_thread.join(3)
        server.server_close()
        self.assertFalse(client.is_alive())
        self.assertFalse(serving.is_alive())
        self.assertEqual(responses[0][0], 200)
        self.assertEqual(responses[0][1]["ok"], True)

    def test_bridge_correlates_bounded_action_result(self) -> None:
        # Exercise request/result correlation with a small blocking stream pair.
        class QueueReader:
            def __init__(self) -> None:
                self.lines: list[bytes] = []
                self.condition = threading.Condition()

            def readline(self, _limit: int) -> bytes:
                with self.condition:
                    while not self.lines:
                        self.condition.wait()
                    return self.lines.pop(0)

            def push(self, value: dict[str, object]) -> None:
                with self.condition:
                    self.lines.append(json.dumps(value).encode() + b"\n")
                    self.condition.notify()

        reader = QueueReader()
        writer = io.StringIO()
        shutdown = threading.Event()
        bridge = quick_review.Bridge(reader, writer, shutdown=shutdown.set)
        bridge.start()
        result: list[dict[str, object]] = []
        request_thread = threading.Thread(
            target=lambda: result.append(
                bridge.request({"action": "mark-viewed", "section": "safe-change"})
            )
        )
        request_thread.start()
        while not writer.getvalue():
            threading.Event().wait(0.01)
        action = json.loads(writer.getvalue())
        reader.push({"type": "shutdown"})
        self.assertTrue(shutdown.wait(1))
        reader.push({"type": "result", "id": action["id"], "ok": True})
        request_thread.join(1)
        self.assertEqual(result[0]["ok"], True)


if __name__ == "__main__":
    unittest.main()
