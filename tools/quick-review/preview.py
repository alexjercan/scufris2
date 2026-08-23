#!/usr/bin/env python3
"""Serve the Quick Review page against a deterministic in-process bridge.

The preview feeds the real Quick Review server a fixed walkthrough fixture and
answers bridge actions with deterministic local state transitions that mirror
the foreground walkthrough semantics. Every response is validated with the
production result validator before it reaches the page.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
import threading
import webbrowser
from pathlib import Path
from typing import Any

_TOOL = Path(__file__).with_name("quick_review.py")
_SPEC = importlib.util.spec_from_file_location("quick_review", _TOOL)
assert _SPEC and _SPEC.loader
quick_review = importlib.util.module_from_spec(_SPEC)
sys.modules[_SPEC.name] = quick_review
_SPEC.loader.exec_module(quick_review)

REVISION = "3f9c0d17ab" * 4
BASE_REVISION = "0d17e5b2c4" * 4

PREVIEW_ANSWER = (
    "Preview answer pinned to this exact revision.\n\n"
    "The rollback persists approved: false before another attempt is allowed, "
    "so a failed finalization can never leave a durable approval behind."
)

PREVIEW_CONTEXT = "\n".join(
    [
        "// exact-revision context (preview fixture)",
        *(
            f"{number:>4}  {text}"
            for number, text in enumerate(
                [
                    "try {",
                    "  await actions.approved([...state.comments], comment);",
                    "  terminal = 'committed';",
                    "} catch (error) {",
                    "  state.approved = false;",
                    "  actions.persist(state);",
                    "  throw error;",
                    "}",
                    "// A deliberately long context line to exercise horizontal scrolling: "
                    + "verify(revision) && persist(state) && rollback(approved) && "
                    + "report(feedback) && ".join(["never-truncate"] * 8),
                ],
                start=540,
            )
        ),
    ]
)


def fixture_document() -> dict[str, Any]:
    return {
        "title": "Quick Review redesign preview: terminal-styled review page",
        "summary": (
            "A deterministic fixture for checking the redesigned Quick Review "
            "page. It exercises **structured prose**, readable diffs, long "
            "content, and every review control against a local preview bridge."
        ),
        "revision": REVISION,
        "baseRevision": BASE_REVISION,
        "files": 3,
        "added": 214,
        "removed": 96,
        "sections": [
            {
                "id": "approval-rollback-protocol",
                "importance": "critical",
                "file": "extensions/scufris/workflow/walkthrough.ts",
                "lines": "608-635",
                "markdown": (
                    "### Rollback protocol\n"
                    "\n"
                    "The terminal approval path persists a durable "
                    "`approved: false` compensating rollback **before** another "
                    "attempt is allowed. A failed finalization is *retryable* "
                    "and keeps the review open.\n"
                    "\n"
                    "- persist state before routing the approval\n"
                    "- roll back durably when finalization fails\n"
                    "- surface the authoritative error in the acting scope\n"
                    "\n"
                    "1. verify the exact revision\n"
                    "2. apply the terminal transition\n"
                    "3. persist or roll back\n"
                    "\n"
                    "> Approval is durable only after exact-revision "
                    "finalization succeeds.\n"
                    "\n"
                    "```ts\n"
                    "try {\n"
                    "  await actions.approved([...state.comments], comment);\n"
                    "} catch (error) {\n"
                    "  state.approved = false; // durable compensating rollback\n"
                    "}\n"
                    "```\n"
                    "\n"
                    "See [walkthrough.ts](extensions/scufris/workflow/walkthrough.ts) "
                    "for the full protocol."
                ),
                "diff": (
                    "@@ -608,9 +608,14 @@\n"
                    '       } else if (action === "approve") {\n'
                    "-        await actions.approved([...state.comments], comment);\n"
                    '-        terminal = "committed";\n'
                    "+        try {\n"
                    "+          await actions.approved([...state.comments], comment);\n"
                    '+          terminal = "committed";\n'
                    "+        } catch (error) {\n"
                    "+          state.approved = false;\n"
                    "+          actions.persist(state); "
                    "// durable rollback before any retry is allowed on this exact revision\n"
                    "+          throw error;\n"
                    "+        }\n"
                    "       }"
                ),
                "prompt": (
                    "Verify that a failed terminal finalization always persists "
                    "approved: false before a retry is possible."
                ),
            },
            {
                "id": "terminal-design-tokens",
                "importance": "important",
                "file": (
                    "tools/quick-review/very-long-nested-directory-for-preview-"
                    "robustness/theme/quick_review_terminal_theme_tokens_and_"
                    "interaction_states.py"
                ),
                "lines": "1-96",
                "markdown": (
                    "#### Coherent tokens\n"
                    "\n"
                    "One token set drives both themes: `--ink`, `--line`, "
                    "`--accent`, and the diff pair `--add-bg` / `--del-bg`. "
                    "Escaping stays strict, so literal markup like "
                    "`<script>alert(1)</script>` renders as text.\n"
                    "\n"
                    "A deliberately long unbroken token checks wrapping: "
                    "`--accent-visited-state-extremely-long-custom-property-"
                    "name-for-wrap-testing`."
                ),
                "diff": (
                    "@@ -1,4 +1,6 @@\n"
                    "-:root{--blue:#0969da;--shadow:0 1px 2px rgba(31,35,40,.08)}\n"
                    "+:root {\n"
                    "+  --accent: #00587a; /* single restrained accent */\n"
                    "+}\n"
                    " * { box-sizing: border-box; }"
                ),
                "prompt": (
                    "Check that both themes read from the same token set and no "
                    "control uses rounded corners or shadows."
                ),
            },
            {
                "id": "docs-note",
                "importance": "supporting",
                "file": "docs/src/workflow.md",
                "lines": "157-161",
                "markdown": (
                    "Documentation keeps the Quick Review description accurate: "
                    "a custom local section-based walkthrough page with "
                    "exact-revision explanations and an overall review comment."
                ),
                "diff": (
                    "@@ -157,3 +157,4 @@\n"
                    " - Quick Review is the custom local section-based walkthrough page.\n"
                    "+  A deterministic preview bridge exists for visual checks.\n"
                ),
                "prompt": "Confirm the documentation matches the shipped behavior.",
            },
        ],
        "warnings": ["Unsupported directive: metrics"],
    }


def fixture_state(document: dict[str, Any]) -> dict[str, Any]:
    sections = [section["id"] for section in document["sections"]]
    return {
        "version": 1,
        "identity": "e" * 64,
        "revision": document["revision"],
        "sections": {
            sections[0]: "not-reviewed",
            sections[1]: "needs-explanation",
            sections[2]: "looks-good",
        },
        "viewed": {sections[0]: False, sections[1]: False, sections[2]: True},
        "questions": [
            {
                "sectionId": sections[1],
                "question": "Why is one accent color enough?",
                "answer": PREVIEW_ANSWER,
            }
        ],
        "comments": [
            {
                "id": "c0ffee" * 4,
                "sectionId": sections[0],
                "file": document["sections"][0]["file"],
                "lines": document["sections"][0]["lines"],
                "body": (
                    "Preview note: the rollback ordering reads well.\n"
                    "Second line checks pre-wrap rendering."
                ),
            }
        ],
        "changeRequests": [],
        "approved": False,
    }


def fixture_init() -> dict[str, Any]:
    document = fixture_document()
    return {
        "type": "init",
        "version": 1,
        "document": document,
        "state": fixture_state(document),
    }


class PreviewBridge:
    """Deterministic local stand-in for the foreground walkthrough bridge."""

    def __init__(self, document: dict[str, Any], state: dict[str, Any]) -> None:
        self.document = document
        self.state = state
        self.sections = {section["id"]: section for section in document["sections"]}
        self.terminal = False
        self.counter = 0

    def handle(self, request: dict[str, Any]) -> dict[str, Any]:
        try:
            payload = self._apply(
                request.get("action"),
                request.get("section") or "",
                (request.get("comment") or "").strip(),
            )
            result: dict[str, Any] = {
                "type": "result",
                "id": request["id"],
                "ok": True,
                "state": self.state,
                **payload,
            }
        except ValueError as error:
            result = {
                "type": "result",
                "id": request["id"],
                "ok": False,
                "state": self.state,
                "error": str(error),
            }
        return quick_review.validate_result(result, self.document)

    def _apply(self, action: Any, section_id: str, comment: str) -> dict[str, str]:
        if self.terminal:
            raise ValueError("review already has a terminal action")
        section_actions = {
            "mark-viewed",
            "reopen",
            "add-comment",
            "explain",
            "context",
            "ask",
        }
        section = self.sections.get(section_id)
        if action in section_actions:
            if section is None:
                raise ValueError("unknown section")
        elif action not in {"approve", "request-changes", "full-diff"}:
            raise ValueError("unknown action")
        state = self.state
        if action in {"mark-viewed", "reopen"}:
            state["approved"] = False
            state["viewed"][section_id] = action == "mark-viewed"
            if (
                action == "mark-viewed"
                and state["sections"][section_id] != "change-requested"
            ):
                state["sections"][section_id] = "looks-good"
            elif action == "reopen" and state["sections"][section_id] == "looks-good":
                state["sections"][section_id] = "not-reviewed"
        elif action == "add-comment":
            if not comment:
                raise ValueError("Add comment requires a review note")
            if len(state["comments"]) >= quick_review.MAX_SECTIONS:
                raise ValueError("review notes exceed the bounded review limit")
            self.counter += 1
            state["comments"].append(
                {
                    "id": f"{self.counter:024x}",
                    "sectionId": section_id,
                    "file": section["file"],
                    "lines": section["lines"],
                    "body": comment,
                }
            )
        elif action == "explain":
            state["sections"][section_id] = "needs-explanation"
            state["questions"].append(
                {
                    "sectionId": section_id,
                    "question": section["prompt"],
                    "answer": PREVIEW_ANSWER,
                }
            )
        elif action == "ask":
            if not comment:
                raise ValueError("Ask reviewer requires a question")
            state["questions"].append(
                {
                    "sectionId": section_id,
                    "question": comment,
                    "answer": PREVIEW_ANSWER,
                }
            )
        elif action == "context":
            return {
                "message": "Exact-revision context loaded.",
                "context": PREVIEW_CONTEXT,
            }
        elif action == "full-diff":
            return {"message": "Opened the exact full diff."}
        elif action == "request-changes":
            if not comment:
                raise ValueError("Request changes requires an overall review comment")
            self.terminal = True
        elif action == "approve":
            if any(not value for value in state["viewed"].values()):
                raise ValueError("all sections must be marked viewed before approval")
            if state["changeRequests"]:
                raise ValueError("blocking feedback must be sent as requested changes")
            state["approved"] = True
            self.terminal = True
        return {"message": "Review updated."}


class QueueReader:
    """Blocking line reader fed by the preview bridge."""

    def __init__(self) -> None:
        self.lines: list[bytes] = []
        self.condition = threading.Condition()

    def readline(self, _limit: int = -1) -> bytes:
        with self.condition:
            while not self.lines:
                self.condition.wait()
            return self.lines.pop(0)

    def push(self, value: dict[str, Any]) -> None:
        with self.condition:
            self.lines.append(json.dumps(value).encode() + b"\n")
            self.condition.notify()


class BridgeWriter:
    """Dispatch server output lines to the deterministic preview bridge."""

    def __init__(
        self,
        reader: QueueReader,
        bridge: PreviewBridge,
        announce: Any,
    ) -> None:
        self.reader = reader
        self.bridge = bridge
        self.announce = announce
        self.buffer = ""

    def write(self, text: str) -> int:
        self.buffer += text
        while "\n" in self.buffer:
            line, self.buffer = self.buffer.split("\n", 1)
            self._handle(json.loads(line))
        return len(text)

    def flush(self) -> None:
        return

    def _handle(self, message: dict[str, Any]) -> None:
        if message.get("type") == "ready":
            self.announce(message["url"])
            self.reader.push({"type": "activate"})
            return
        self.reader.push(self.bridge.handle(message))


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Serve a deterministic Quick Review preview."
    )
    parser.add_argument("--no-open", action="store_true", help="do not open a browser")
    args = parser.parse_args()
    init = fixture_init()
    quick_review.validate_init(init)
    bridge = PreviewBridge(init["document"], init["state"])
    reader = QueueReader()

    def announce(url: str) -> None:
        print(f"Quick Review preview: {url}", flush=True)
        print("Press Ctrl-C to stop.", flush=True)

    writer = BridgeWriter(reader, bridge, announce)
    reader.push(init)
    opener = (
        (lambda _url: None)
        if args.no_open
        else (lambda url: webbrowser.open(url, new=2))
    )
    try:
        return quick_review.serve(reader, writer, opener)
    except KeyboardInterrupt:
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
