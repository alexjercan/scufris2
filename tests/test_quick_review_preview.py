import importlib.util
import json
import sys
import unittest
from pathlib import Path
from typing import Any

PREVIEW = Path(__file__).parents[1] / "tools" / "quick-review" / "preview.py"
SPEC = importlib.util.spec_from_file_location("quick_review_preview", PREVIEW)
assert SPEC and SPEC.loader
preview = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = preview
SPEC.loader.exec_module(preview)
quick_review = preview.quick_review


class PreviewBridgeTest(unittest.TestCase):
    def make(self) -> Any:
        init = preview.fixture_init()
        return preview.PreviewBridge(init["document"], init["state"])

    def request(
        self,
        bridge: Any,
        action: str,
        section: str | None = None,
        comment: str | None = None,
    ) -> dict[str, Any]:
        request: dict[str, Any] = {
            "type": "action",
            "id": "a" * 24,
            "action": action,
        }
        if section is not None:
            request["section"] = section
        if comment is not None:
            request["comment"] = comment
        return bridge.handle(request)

    def test_fixture_is_a_valid_bridge_init_message(self) -> None:
        quick_review.validate_init(preview.fixture_init())

    def test_fixture_renders_with_the_production_renderer(self) -> None:
        init = preview.fixture_init()
        page = quick_review.render_page(init["document"], init["state"])
        self.assertIn("Quick Review redesign preview", page)
        self.assertIn('class="card viewed"', page)
        self.assertNotIn("<script>alert", page)

    def test_full_review_flow_reaches_approval_then_terminal(self) -> None:
        bridge = self.make()
        early = self.request(bridge, "approve")
        self.assertFalse(early["ok"])
        explained = self.request(bridge, "explain", "terminal-design-tokens")
        self.assertTrue(explained["ok"])
        self.assertEqual(
            explained["state"]["sections"]["terminal-design-tokens"],
            "needs-explanation",
        )
        for section in bridge.sections:
            self.assertTrue(self.request(bridge, "mark-viewed", section)["ok"])
        comment = self.request(bridge, "add-comment", "docs-note", "Nice <note>")
        self.assertTrue(comment["ok"])
        self.assertEqual(comment["state"]["comments"][-1]["id"], f"{1:024x}")
        approved = self.request(bridge, "approve", comment="ship it")
        self.assertTrue(approved["ok"])
        self.assertTrue(approved["state"]["approved"])
        blocked = self.request(bridge, "mark-viewed", "docs-note")
        self.assertFalse(blocked["ok"])
        self.assertIn("terminal", blocked["error"])

    def test_context_and_full_diff_do_not_mutate_state(self) -> None:
        bridge = self.make()
        before = json.dumps(bridge.state, sort_keys=True)
        result = self.request(bridge, "context", "approval-rollback-protocol")
        self.assertTrue(result["ok"])
        self.assertIn("exact-revision context", result["context"])
        self.assertTrue(self.request(bridge, "full-diff")["ok"])
        self.assertEqual(json.dumps(bridge.state, sort_keys=True), before)

    def test_request_changes_requires_an_explanation_then_terminates(self) -> None:
        bridge = self.make()
        missing = self.request(bridge, "request-changes")
        self.assertFalse(missing["ok"])
        self.assertIn("overall review comment", missing["error"])
        accepted = self.request(bridge, "request-changes", comment="Fix the diff")
        self.assertTrue(accepted["ok"])
        self.assertFalse(self.request(bridge, "approve")["ok"])

    def test_unknown_actions_sections_and_blank_questions_error(self) -> None:
        bridge = self.make()
        self.assertFalse(self.request(bridge, "detonate")["ok"])
        self.assertFalse(self.request(bridge, "mark-viewed", "missing")["ok"])
        self.assertFalse(self.request(bridge, "ask", "docs-note", " ")["ok"])


if __name__ == "__main__":
    unittest.main()
