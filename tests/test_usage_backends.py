"""What the subscription backends make of an answer, without asking for one.

Only the pure part is exercised here: the normalising that turns a service's
own shape into the one line the panel reads. Nothing in this file reaches the
network, and the two tests that call `reading()` point the backends at an empty
directory, so they stop at "not signed in" before any request is built.
"""

import importlib.util
import tempfile
import unittest
import unittest.mock
from pathlib import Path
from types import ModuleType

BACKENDS = Path(__file__).parents[1] / "native" / "backends"

#: What a window may say and nothing else. The answers these backends read
#: carry the account behind them, including an email address, and a key that
#: appeared here would be a key on the panel and in the log.
FIELDS = {"label", "percent", "resets"}


def backend(name: str) -> ModuleType:
    """Loads one backend by path, since the directories are not a package."""
    path = BACKENDS / name / "backend.py"
    spec = importlib.util.spec_from_file_location(f"backend_{name}", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


claude = backend("claude")
codex = backend("codex")


class ClaudeTests(unittest.TestCase):
    def answer(self) -> dict[str, object]:
        """The shape the service answers with, as recorded from a live call."""
        return {
            "limits": [
                {
                    "kind": "session",
                    "percent": 11,
                    "resets_at": "2000-01-01T00:00:00+00:00",
                    "scope": None,
                },
                {
                    "kind": "weekly_all",
                    "percent": 74,
                    "resets_at": "2000-01-01T00:00:00+00:00",
                    "scope": None,
                },
                {
                    "kind": "weekly_scoped",
                    "percent": 52,
                    "resets_at": None,
                    "scope": {"model": {"id": None, "display_name": "Fable"}},
                },
            ]
        }

    def test_every_limit_becomes_a_window_in_the_order_given(self) -> None:
        windows = claude.windows(self.answer())
        self.assertEqual(
            [(window["label"], window["percent"]) for window in windows],
            [("session", 11.0), ("weekly", 74.0), ("fable", 52.0)],
        )

    def test_a_window_carries_nothing_but_what_the_panel_draws(self) -> None:
        for window in claude.windows(self.answer()):
            self.assertEqual(set(window), FIELDS)

    def test_a_scoped_limit_is_named_after_the_model_it_scopes(self) -> None:
        # Two limits called "weekly" would say nothing about which is biting.
        self.assertEqual(claude.label({"kind": "weekly_all"}), "weekly")
        self.assertEqual(
            claude.label(
                {"kind": "weekly_scoped", "scope": {"model": {"display_name": "Opus"}}}
            ),
            "opus",
        )
        self.assertEqual(claude.label({"kind": "weekly_scoped"}), "weekly_scoped")

    def test_a_reset_already_past_is_no_time_at_all(self) -> None:
        self.assertEqual(claude.remaining("2000-01-01T00:00:00+00:00"), 0.0)
        # An instant with no zone is read as UTC rather than refused.
        self.assertEqual(claude.remaining("2000-01-01T00:00:00"), 0.0)
        self.assertIsNone(claude.remaining("soon"))
        self.assertIsNone(claude.remaining(None))

    def test_an_answer_that_is_not_the_shape_expected_has_no_windows(self) -> None:
        self.assertEqual(claude.windows({}), [])
        self.assertEqual(claude.windows({"limits": "none"}), [])
        self.assertEqual(claude.windows({"limits": [{"kind": "session"}]}), [])
        self.assertEqual(claude.windows(None), [])

    def test_a_machine_that_never_signed_in_says_so_without_asking(self) -> None:
        with (
            tempfile.TemporaryDirectory() as empty,
            unittest.mock.patch.dict(
                "os.environ", {"CLAUDE_CONFIG_DIR": empty}, clear=False
            ),
        ):
            self.assertEqual(
                claude.reading(),
                {"plan": None, "windows": [], "error": "not signed in"},
            )


class CodexTests(unittest.TestCase):
    def answer(self) -> dict[str, object]:
        """The shape the service answers with, as recorded from a live call."""
        return {
            "email": "someone@example.com",
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 15,
                    "limit_window_seconds": 604800,
                    "reset_after_seconds": 400.0,
                },
                "secondary_window": {
                    "used_percent": 40,
                    "limit_window_seconds": 18000,
                    "reset_after_seconds": 90.0,
                },
            },
        }

    def test_the_window_that_bites_soonest_is_read_first(self) -> None:
        # Which of primary and secondary is the short one depends on the plan.
        windows = codex.windows(self.answer())
        self.assertEqual(
            [(window["label"], window["percent"]) for window in windows],
            [("5h", 40.0), ("weekly", 15.0)],
        )

    def test_a_window_carries_nothing_but_what_the_panel_draws(self) -> None:
        for window in codex.windows(self.answer()):
            self.assertEqual(set(window), FIELDS)

    def test_a_window_length_is_said_the_way_the_limit_is_described(self) -> None:
        self.assertEqual(codex.span(604800), "weekly")
        self.assertEqual(codex.span(86400), "daily")
        self.assertEqual(codex.span(18000), "5h")
        self.assertEqual(codex.span(1800), "30m")
        self.assertEqual(codex.span(None), "limit")

    def test_the_instant_is_preferred_to_the_countdown(self) -> None:
        # A reading that sat in a queue still resets when it said it would.
        self.assertEqual(
            codex.resets({"reset_at": 0, "reset_after_seconds": 500.0}), 0.0
        )
        self.assertEqual(codex.resets({"reset_after_seconds": 500.0}), 500.0)
        self.assertIsNone(codex.resets({}))

    def test_an_answer_that_is_not_the_shape_expected_has_no_windows(self) -> None:
        self.assertEqual(codex.windows({}), [])
        self.assertEqual(codex.windows({"rate_limit": {"primary_window": None}}), [])
        self.assertEqual(codex.windows(None), [])

    def test_a_machine_that_never_signed_in_says_so_without_asking(self) -> None:
        with (
            tempfile.TemporaryDirectory() as empty,
            unittest.mock.patch.dict("os.environ", {"CODEX_HOME": empty}, clear=False),
        ):
            self.assertEqual(
                codex.reading(),
                {"plan": None, "windows": [], "error": "not signed in"},
            )


if __name__ == "__main__":
    unittest.main()
