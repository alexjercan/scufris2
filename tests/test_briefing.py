"""The briefing library: what a source may answer, and what a run keeps.

Every test runs against a temporary state directory and a temporary project
root. Nothing here reads the real journal, the real projects, or a real model:
the harness is a script that answers whatever the test told it to.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPOSITORY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPOSITORY / "tools" / "briefing"))

import briefing
import page

ANSWERING = """#!/usr/bin/env python3
import os
import pathlib
import sys
prompt = sys.argv[-1]
pathlib.Path(os.environ["BRIEFING_PROMPT"]).write_text(prompt)
print(pathlib.Path(os.environ["BRIEFING_ANSWER"]).read_text())
"""

SLEEPING = """#!/usr/bin/env python3
import time
time.sleep(30)
"""

#: Answers from a list of files, one for each time it is asked, and keeps the
#: prompt of every run. The last file answers every run after it.
IN_TURN = """#!/usr/bin/env python3
import os
import pathlib
import sys
counter = pathlib.Path(os.environ["BRIEFING_COUNT"])
asked = int(counter.read_text()) if counter.exists() else 0
counter.write_text(str(asked + 1))
pathlib.Path(f'{os.environ["BRIEFING_PROMPT"]}.{asked}').write_text(sys.argv[-1])
answers = os.environ["BRIEFING_ANSWERS"].split(os.pathsep)
said = pathlib.Path(answers[min(asked, len(answers) - 1)]).read_text()
if said.startswith("#sleep"):
    import time

    time.sleep(30)
print(said)
"""

#: Writes bytes that are not text at all.
BINARY = """#!/usr/bin/env python3
import sys
sys.stdout.buffer.write(b"\\xff\\xfe not text")
"""

FAILING = """#!/usr/bin/env python3
import sys
print("nothing to report")
print("the token expired", file=sys.stderr)
raise SystemExit(3)
"""

ENVELOPE = {
    "title": "The Den",
    "status": "attention",
    "headline": "Two tasks are left over from yesterday.",
    "facts": [{"label": "Restant", "value": "2 tasks"}],
    "body": "### Yesterday\n\n- call the dentist\n- book the trip\n",
}


class Envelope(unittest.TestCase):
    def test_the_last_fenced_block_is_the_answer(self) -> None:
        text = (
            'Here is a draft.\n\n```json\n{"title": "draft"}\n```\n\n'
            "On reflection:\n\n```json\n" + json.dumps(ENVELOPE) + "\n```\n"
        )
        found = briefing.parse_contribution(text)
        self.assertEqual(found["title"], "The Den")
        self.assertEqual(found["status"], "attention")
        self.assertEqual(found["facts"], [{"label": "Restant", "value": "2 tasks"}])

    def test_a_body_that_fences_code_of_its_own_is_read_whole(self) -> None:
        # A body is Markdown, so it may fence a diff or a status listing. The
        # closing fence of the answer is then not the first ``` after the
        # opening one, and matching fences against each other cuts the answer
        # off mid-string. A real nova-protocol answer was lost this way.
        quoted = {
            **ENVELOPE,
            "body": "## The tree\n\n```\n M Cargo.toml\n?? scripts/gen.py\n```\n\nDone.",
        }
        text = "Here is the morning.\n\n```json\n" + json.dumps(quoted) + "\n```\n"
        found = briefing.parse_contribution(text)
        self.assertEqual(found["headline"], ENVELOPE["headline"])
        self.assertIn("?? scripts/gen.py", found["body"])
        self.assertTrue(found["body"].endswith("Done."))

    def test_an_envelope_quoted_inside_a_body_is_not_the_answer(self) -> None:
        # A source may show the shape it was asked for inside its own body.
        # That block starts inside the answer, so it was quoted by it.
        quoted = {
            **ENVELOPE,
            "body": 'I was asked for:\n\n```json\n{"title": "shape"}\n```\n',
        }
        text = "```json\n" + json.dumps(quoted) + "\n```\n"
        self.assertEqual(briefing.parse_contribution(text)["title"], "The Den")

    def test_a_bare_envelope_without_a_fence_is_accepted(self) -> None:
        found = briefing.parse_contribution(json.dumps(ENVELOPE))
        self.assertEqual(found["headline"], ENVELOPE["headline"])

    def test_a_stray_quotation_mark_is_refused_and_not_raised(self) -> None:
        # The real seedzero answer: a quotation mark inside the body ended the
        # JSON string early. The reader must say so, not fall over.
        broken = (
            '```json\n{"title": "Seed Zero", "status": "ok", '
            '"headline": "Six shorts are out.", "facts": [], '
            '"body": "The comment ("plinko") is unchanged."}\n```\n'
        )
        with self.assertRaises(briefing.Unusable) as caught:
            briefing.parse_contribution(broken)
        self.assertIn("not one JSON envelope", str(caught.exception))

    def test_an_answer_nested_past_the_stack_is_refused_and_not_raised(self) -> None:
        # A decoder that runs out of stack raises something that is not a
        # decode error. A source cannot be allowed to end the run that way.
        deep = "[" * 200_000 + "]" * 200_000
        with self.assertRaises(briefing.Unusable):
            briefing.parse_contribution('{"body": ' + deep + "}")

    def test_an_empty_or_silent_answer_is_refused_by_name(self) -> None:
        for text in ("", "   \n\t ", "```\n```\n", "```sh\nls -la\n```\n"):
            with self.assertRaises(briefing.Unusable) as caught:
                briefing.parse_contribution(text)
            self.assertIn("not one JSON envelope", str(caught.exception), repr(text))

    def test_a_fence_is_read_however_it_was_written(self) -> None:
        body = json.dumps(ENVELOPE)
        for opening in ("```json", "```JSON", "```json  ", "```"):
            for ending in ("\n", "\r\n"):
                text = f"{opening}{ending}{body}{ending}```{ending}"
                found = briefing.parse_contribution(text)
                self.assertEqual(found["title"], "The Den", f"{opening!r} {ending!r}")

    def test_words_around_the_block_are_not_the_answer(self) -> None:
        text = (
            "I read the journal.\n\n```json\n"
            + json.dumps(ENVELOPE)
            + "\n```\n\nSay the word and I will file the two tasks.\n"
        )
        self.assertEqual(briefing.parse_contribution(text)["title"], "The Den")

    def test_a_value_exactly_at_its_limit_is_kept(self) -> None:
        found = briefing.parse_contribution(
            json.dumps(
                {
                    **ENVELOPE,
                    "title": "t" * briefing.MAX_TITLE,
                    "headline": "h" * briefing.MAX_HEADLINE,
                    "body": "b" * briefing.MAX_BODY,
                    "facts": [
                        {
                            "label": "l" * briefing.MAX_LABEL,
                            "value": "v" * briefing.MAX_VALUE,
                        }
                    ],
                }
            )
        )
        self.assertEqual(len(found["headline"]), briefing.MAX_HEADLINE)
        self.assertEqual(len(found["body"]), briefing.MAX_BODY)

    def test_a_fact_that_is_not_a_label_and_a_value_is_refused(self) -> None:
        for facts, expected in (
            ("two of them", "facts must be a list"),
            (["Restant: 2"], "one label and one value"),
            ([{"label": 2, "value": "b"}], "a fact label must be text"),
            ([{"label": "a", "value": ""}], "a fact value must be text"),
            ([{"label": "a"}], "a fact value must be text"),
        ):
            with self.assertRaises(briefing.Unusable) as caught:
                briefing.parse_contribution(json.dumps({**ENVELOPE, "facts": facts}))
            self.assertIn(expected, str(caught.exception), repr(facts))

    def test_a_headline_of_whitespace_is_not_a_headline(self) -> None:
        for field in ("title", "headline"):
            for value in ("", "   ", None, 7):
                with self.assertRaises(briefing.Unusable):
                    briefing.parse_contribution(json.dumps({**ENVELOPE, field: value}))

    def test_an_answer_that_is_not_an_object_is_refused(self) -> None:
        for text in ("```json\n[1, 2]\n```", "7", '"a headline"', "null", "true"):
            with self.assertRaises(briefing.Unusable) as caught:
                briefing.parse_contribution(text)
            self.assertIn("not one JSON envelope", str(caught.exception), repr(text))

    def test_a_failure_is_recorded_as_a_headline_and_not_a_log_line(self) -> None:
        # The page lays out a sentence. A harness message can be a paragraph,
        # so the runner's own headline is bounded like any other.
        entry = briefing.failed_contribution(
            {"project": "personal/seedzero", "harness": "claude", "model": "opus"},
            "the harness exited 1:\n" + "detail " * 200,
        )
        self.assertEqual(len(entry["headline"]), briefing.MAX_HEADLINE)
        self.assertNotIn("\n", entry["headline"])
        self.assertEqual(entry["status"], "failed")
        self.assertEqual(entry["slug"], "personal-seedzero")

    def test_a_source_missing_its_own_fields_can_still_be_named(self) -> None:
        # This is what a failure is recorded with, so it must not be the thing
        # that fails.
        entry = briefing.failed_contribution({}, "")
        self.assertEqual(entry["project"], "unknown")
        self.assertEqual(entry["slug"], "unknown")
        self.assertEqual(entry["headline"], "the source could not answer")
        self.assertEqual(entry["title"], "unknown")

    def test_only_the_runner_may_call_a_source_failed(self) -> None:
        with self.assertRaises(briefing.Unusable):
            briefing.parse_contribution(json.dumps({**ENVELOPE, "status": "failed"}))

    def test_an_answer_that_is_not_an_envelope_is_refused_by_name(self) -> None:
        for text, expected in (
            ("I could not find the data.", "not one JSON envelope"),
            (json.dumps([ENVELOPE]), "not one JSON envelope"),
            (json.dumps({**ENVELOPE, "mood": "good"}), "unexpected keys: mood"),
            (json.dumps({**ENVELOPE, "status": "fine"}), "status must be one of"),
            (
                json.dumps(
                    {**ENVELOPE, "facts": [{"label": "a", "value": "b", "why": "c"}]}
                ),
                "one label and one value",
            ),
            (
                json.dumps({**ENVELOPE, "facts": [{"label": "a", "value": "b"}] * 7}),
                "at most 6 entries",
            ),
            (json.dumps({**ENVELOPE, "title": "t" * 200}), "longer than 80"),
            (
                json.dumps({**ENVELOPE, "body": "b" * (briefing.MAX_BODY + 1)}),
                "longer than",
            ),
        ):
            with self.subTest(text=text[:40]):
                with self.assertRaises(briefing.Unusable) as caught:
                    briefing.parse_contribution(text)
                self.assertIn(expected, str(caught.exception))

    def test_a_headline_is_flattened_onto_one_line(self) -> None:
        found = briefing.parse_contribution(
            json.dumps({**ENVELOPE, "headline": "Two tasks\n  are  left."})
        )
        self.assertEqual(found["headline"], "Two tasks are left.")


class Command(unittest.TestCase):
    def test_a_source_is_asked_once_and_keeps_nothing(self) -> None:
        source = {
            "project": "personal/the-den",
            "project_root": "/tmp",
            "harness": "pi",
            "model": "openai-codex/gpt-5.6-sol",
            "thinking": "medium",
        }
        argv = briefing.harness_argv(source, "the prompt")
        self.assertEqual(argv[0], "pi")
        self.assertIn("--print", argv)
        self.assertIn("--no-session", argv)
        self.assertEqual(argv[-1], "the prompt")
        tools = set(argv[argv.index("--tools") + 1].split(","))
        self.assertEqual(tools & {"edit", "write"}, set())

    def test_the_claude_harness_is_denied_the_edit_tools(self) -> None:
        argv = briefing.harness_argv(
            {
                "project": "personal/seedzero",
                "project_root": "/tmp",
                "harness": "claude",
                "model": "opus",
                "thinking": "high",
            },
            "the prompt",
        )
        self.assertEqual(argv[0], "claude")
        self.assertIn("--print", argv)
        denied = set(argv[argv.index("--disallowed-tools") + 1].split(","))
        self.assertIn("Edit", denied)
        self.assertIn("Write", denied)

    def test_both_harnesses_answer_without_asking_anyone(self) -> None:
        # Nobody is watching a source run, so a question it cannot ask is a
        # refusal. `dontAsk` sandboxes the shell, and sources reported `gh`
        # and `python3` denied instead of reporting their project.
        source = {
            "project": "personal/seedzero",
            "project_root": "/tmp",
            "harness": "claude",
            "model": "opus",
            "thinking": "high",
        }
        argv = briefing.harness_argv(source, "the prompt")
        self.assertEqual(argv[argv.index("--permission-mode") + 1], "bypassPermissions")
        pi_argv = briefing.harness_argv({**source, "harness": "pi"}, "the prompt")
        self.assertIn("--approve", pi_argv)

    def test_the_prompt_carries_the_project_guidance_and_the_shape(self) -> None:
        prompt = briefing.contribution_prompt(
            {
                "project": "personal/the-den",
                "project_root": "/home/x/the-den",
                "description": "Report yesterday.",
                "guidance": "Run scufris-den restant.",
                "harness": "pi",
                "model": "m",
                "thinking": "medium",
            },
            "morning",
            "2026-08-31",
        )
        self.assertIn("Run scufris-den restant.", prompt)
        self.assertIn("/home/x/the-den", prompt)
        self.assertIn("one fenced `json` block", prompt)
        self.assertIn("Never estimate a number you", prompt)
        # A source reads unless its own project asked it for something more.
        self.assertIn("unless the guidance below names it", prompt)
        # A limit the runner enforces is a limit the source is told. A source
        # that wrote a 227 character headline lost a whole good answer to one
        # it was never given.
        said = " ".join(prompt.split())
        for limit in (
            briefing.MAX_TITLE,
            briefing.MAX_HEADLINE,
            briefing.MAX_LABEL,
            briefing.MAX_VALUE,
            briefing.MAX_BODY,
        ):
            self.assertIn(f"at most {limit} characters", said)


class Run(unittest.TestCase):
    def setUp(self) -> None:
        self.room = tempfile.TemporaryDirectory(prefix="scufris-briefing-")
        self.addCleanup(self.room.cleanup)
        self.root = Path(self.room.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.projects = self.root / "projects"
        self.answer = self.root / "answer.txt"
        self.prompt = self.root / "prompt.txt"
        self.answer.write_text(
            f"```json\n{json.dumps(ENVELOPE)}\n```\n", encoding="utf-8"
        )
        self.environment = mock.patch.dict(
            os.environ,
            {
                "PATH": f"{self.bin}:{os.environ['PATH']}",
                "XDG_STATE_HOME": str(self.root / "state"),
                "SCUFRIS_PROJECT_ROOTS": json.dumps([str(self.projects)]),
                "BRIEFING_ANSWER": str(self.answer),
                "BRIEFING_PROMPT": str(self.prompt),
            },
        )
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.harness(ANSWERING)

    def harness(self, program: str) -> None:
        for name in ("pi", "claude"):
            executable = self.bin / name
            executable.write_text(program, encoding="utf-8")
            executable.chmod(0o755)

    def project(self, name: str, configuration: str) -> Path:
        root = self.projects / name
        root.mkdir(parents=True)
        subprocess.run(
            ["git", "init", "-b", "master"], cwd=root, check=True, capture_output=True
        )
        (root / ".scufris.toml").write_text(configuration, encoding="utf-8")
        return root

    def answers(self, *texts: str) -> None:
        """Answer with each of these in turn, keeping every prompt."""
        files = []
        for index, text in enumerate(texts):
            written = self.root / f"answer-{index}.txt"
            written.write_text(text, encoding="utf-8")
            files.append(str(written))
        environment = mock.patch.dict(
            os.environ,
            {
                "BRIEFING_ANSWERS": os.pathsep.join(files),
                "BRIEFING_COUNT": str(self.root / "asked.txt"),
            },
        )
        environment.start()
        self.addCleanup(environment.stop)
        self.harness(IN_TURN)

    def asked(self) -> list[str]:
        """Every prompt the harness was given, in order."""
        return [
            path.read_text(encoding="utf-8")
            for path in sorted(
                self.root.glob("prompt.txt.*"), key=lambda item: int(item.suffix[1:])
            )
        ]

    def declare(self, name: str, harness: str = "pi") -> Path:
        return self.project(
            name,
            "[briefings.morning]\n"
            f'description = "Report {name}."\n'
            f'keywords = {{ harness = "{harness}" }}\n'
            f'guidance = "Read {name} and report it."\n',
        )

    def test_only_a_project_that_declares_the_profile_is_asked(self) -> None:
        self.declare("the-den")
        self.project(
            "quiet",
            '[agents.work]\ndescription = "Implement a change."\n',
        )
        manifest = briefing.collect("morning", "2026-08-31")
        self.assertEqual(
            [item["project"] for item in manifest["sources"]], ["projects/the-den"]
        )
        self.assertEqual(manifest["state"], "collected")
        self.assertIn("Read the-den and report it.", self.prompt.read_text())

    def test_a_contribution_is_kept_beside_a_manifest_that_indexes_it(self) -> None:
        self.declare("the-den")
        manifest = briefing.collect("morning", "2026-08-31")
        entry = manifest["sources"][0]
        self.assertEqual(entry["status"], "attention")
        self.assertEqual(entry["headline"], ENVELOPE["headline"])
        # The index carries what a reader chooses by and not the body.
        self.assertNotIn("body", entry)
        kept = json.loads(
            (
                briefing.run_dir("2026-08-31")
                / "contributions"
                / "projects-the-den.json"
            ).read_text()
        )
        self.assertEqual(kept["body"], ENVELOPE["body"].strip())
        self.assertEqual(kept["harness"], "pi")

    def test_a_source_that_answers_with_prose_is_failed_and_its_words_are_kept(
        self,
    ) -> None:
        self.declare("the-den")
        self.answer.write_text("I could not read the journal today.", encoding="utf-8")
        manifest = briefing.collect("morning", "2026-08-31")
        entry = manifest["sources"][0]
        self.assertEqual(entry["status"], "failed")
        self.assertIn("not one JSON envelope", entry["headline"])
        kept = json.loads(
            (
                briefing.run_dir("2026-08-31")
                / "contributions"
                / "projects-the-den.json"
            ).read_text()
        )
        self.assertIn("could not read the journal", kept["raw"])

    def test_a_source_that_mis_said_its_report_is_asked_to_say_it_again(self) -> None:
        # It read the project and spent whatever its guidance allowed. The
        # report exists; only the saying of it was wrong.
        self.declare("seedzero")
        self.answers(
            '```json\n{"title": "Seed Zero", "status": "ok", "headline": "Six are out.",'
            ' "facts": [], "body": "The comment ("plinko") is unchanged."}\n```\n',
            "```json\n" + json.dumps(ENVELOPE) + "\n```\n",
        )
        manifest = briefing.collect("morning", "2026-08-31")
        entry = manifest["sources"][0]
        self.assertEqual(entry["status"], "attention")
        self.assertEqual(entry["headline"], ENVELOPE["headline"])
        first, second = self.asked()
        self.assertIn("Read seedzero and report it.", first)
        # The second run is given the rejection and its own words, and is told
        # to gather nothing: the work is already done.
        self.assertIn("not one JSON envelope", second)
        self.assertIn("plinko", second)
        self.assertIn("Read nothing, run nothing", second)
        self.assertNotIn("Read seedzero and report it.", second)

    def test_a_source_over_a_limit_is_asked_to_shorten_it(self) -> None:
        # Not only unreadable answers. A good report with one field over its
        # limit is a report, and the source is the one who can shorten it.
        self.declare("seedzero")
        self.answers(
            "```json\n" + json.dumps({**ENVELOPE, "headline": "h" * 227}) + "\n```\n",
            "```json\n" + json.dumps(ENVELOPE) + "\n```\n",
        )
        manifest = briefing.collect("morning", "2026-08-31")
        self.assertEqual(manifest["sources"][0]["status"], "attention")
        self.assertIn("longer than 200 characters", self.asked()[1])

    def test_a_source_that_cannot_say_it_twice_is_failed_and_says_so(self) -> None:
        self.declare("seedzero")
        self.answers("I could not read the channel today.")
        manifest = briefing.collect("morning", "2026-08-31")
        entry = manifest["sources"][0]
        self.assertEqual(entry["status"], "failed")
        self.assertIn("not one JSON envelope", entry["headline"])
        self.assertIn("again when asked to correct it", entry["headline"])
        self.assertEqual(len(self.asked()), 2)

    def test_the_second_asking_reads_nothing_and_runs_nothing(self) -> None:
        argv = briefing.harness_argv(
            {
                "project": "personal/seedzero",
                "project_root": "/tmp",
                "harness": "pi",
                "model": "opus",
                "thinking": "high",
            },
            "the prompt",
            tools=False,
        )
        self.assertIn("--no-tools", argv)
        self.assertNotIn("--tools", argv)
        claude = briefing.harness_argv(
            {
                "project": "personal/seedzero",
                "project_root": "/tmp",
                "harness": "claude",
                "model": "opus",
                "thinking": "high",
            },
            "the prompt",
            tools=False,
        )
        self.assertEqual(claude[claude.index("--tools") + 1], "")

    def test_a_source_that_never_answered_is_not_asked_to_correct_itself(self) -> None:
        # Nothing was said, so there is nothing to correct. Asking again would
        # spend the deadline of a source that has already failed to start.
        self.declare("the-den")
        self.harness(FAILING)
        manifest = briefing.collect("morning", "2026-08-31")
        self.assertEqual(manifest["sources"][0]["status"], "failed")
        self.assertNotIn("again when asked", manifest["sources"][0]["headline"])

    def test_a_source_with_no_time_left_is_not_asked_twice(self) -> None:
        self.declare("seedzero")
        self.answers("not an envelope at all")
        manifest = briefing.collect(
            "morning", "2026-08-31", source_deadline=briefing.REPAIR_FLOOR - 1
        )
        self.assertEqual(manifest["sources"][0]["status"], "failed")
        self.assertEqual(len(self.asked()), 1)

    def test_the_second_asking_is_bounded_like_every_other_run(self) -> None:
        # The first asking answers badly at once; the second never returns.
        self.declare("seedzero")
        self.answers("not an envelope at all", "#sleep")
        with mock.patch.dict(os.environ, {"SCUFRIS_BRIEFING_REPAIR_DEADLINE": "0.4"}):
            manifest = briefing.collect("morning", "2026-08-31")
        entry = manifest["sources"][0]
        self.assertEqual(entry["status"], "failed")
        # The repair timed out, so what is reported is what the source did say.
        self.assertIn("not one JSON envelope", entry["headline"])
        self.assertNotIn("again when asked", entry["headline"])
        self.assertEqual(len(self.asked()), 2)

    def test_bytes_that_are_not_text_are_refused_rather_than_raised(self) -> None:
        self.declare("the-den")
        self.harness(BINARY)
        manifest = briefing.collect("morning", "2026-08-31")
        self.assertEqual(manifest["sources"][0]["status"], "failed")
        self.assertEqual(manifest["state"], "failed")

    def test_a_harness_that_exits_badly_is_named_rather_than_guessed(self) -> None:
        self.declare("the-den")
        self.harness(FAILING)
        manifest = briefing.collect("morning", "2026-08-31")
        entry = manifest["sources"][0]
        self.assertEqual(entry["status"], "failed")
        self.assertIn("exited 3", entry["headline"])
        self.assertIn("the token expired", entry["headline"])

    def test_a_source_that_breaks_the_runner_does_not_take_the_morning(self) -> None:
        # `ask` answers rather than raises, so this is a way to fail nobody has
        # thought of yet. The morning still publishes, and the source is named.
        self.declare("the-den")
        self.declare("seedzero")
        real = briefing.ask

        def explode(source: dict[str, object], *rest: object) -> dict[str, object]:
            if source["project"].endswith("seedzero"):
                raise ZeroDivisionError("a way nobody thought of")
            return real(source, *rest)

        with mock.patch.object(briefing, "ask", explode):
            manifest = briefing.collect("morning", "2026-08-31")
        self.assertEqual(manifest["state"], "collected")
        by_project = {item["project"]: item for item in manifest["sources"]}
        self.assertEqual(by_project["projects/the-den"]["status"], "attention")
        broken = by_project["projects/seedzero"]
        self.assertEqual(broken["status"], "failed")
        self.assertIn("could not ask this source", broken["headline"])
        self.assertIn("ZeroDivisionError", broken["headline"])

    def test_a_page_that_cannot_be_laid_out_does_not_unmake_the_run(self) -> None:
        # The record is written before the page. A morning that was collected
        # stays collected, and the reason the page is missing is kept with it.
        self.declare("the-den")
        with mock.patch.object(
            page, "render_page", side_effect=RuntimeError("no layout")
        ):
            manifest = briefing.collect("morning", "2026-08-31")
        self.assertEqual(manifest["state"], "collected")
        self.assertFalse((briefing.run_dir("2026-08-31") / "briefing.html").exists())
        said = " ".join(item["diagnostic"] for item in manifest["diagnostics"])
        self.assertIn("the page could not be rendered", said)
        kept = json.loads(
            (briefing.run_dir("2026-08-31") / "manifest.json").read_text()
        )
        self.assertEqual(kept["state"], "collected")

    def test_a_contribution_that_cannot_be_kept_costs_one_source(self) -> None:
        self.declare("the-den")
        self.declare("seedzero")
        real = briefing.atomic_write

        def refuse(path: object, data: str) -> None:
            if str(path).endswith("projects-seedzero.json"):
                raise OSError("the disk is full")
            real(path, data)

        with mock.patch.object(briefing, "atomic_write", refuse):
            manifest = briefing.collect("morning", "2026-08-31")
        self.assertEqual(manifest["state"], "collected")
        self.assertEqual(
            [item["project"] for item in manifest["sources"]], ["projects/the-den"]
        )
        said = " ".join(item["diagnostic"] for item in manifest["diagnostics"])
        self.assertIn("the contribution could not be kept", said)

    def test_one_slow_source_costs_its_own_deadline_and_no_other(self) -> None:
        self.declare("the-den")
        slow = self.declare("seedzero")
        (slow / "pi-slow").write_text("", encoding="utf-8")
        # Both run the same fake harness, so the slow one is made by giving the
        # whole run a deadline the sleeper cannot meet.
        self.harness(SLEEPING)
        manifest = briefing.collect("morning", "2026-08-31", source_deadline=0.5)
        self.assertEqual(manifest["state"], "failed")
        self.assertEqual(len(manifest["sources"]), 2)
        for entry in manifest["sources"]:
            self.assertEqual(entry["status"], "failed")
            self.assertIn("did not answer within", entry["headline"])

    def test_a_morning_with_no_source_is_still_a_run(self) -> None:
        manifest = briefing.collect("morning", "2026-08-31")
        self.assertEqual(manifest["sources"], [])
        self.assertEqual(manifest["state"], "collected")
        self.assertFalse(briefing.delivered("2026-08-31"))
        rendered = (briefing.run_dir("2026-08-31") / "briefing.html").read_text(
            encoding="utf-8"
        )
        self.assertIn("No project declared this briefing.", rendered)

    def test_a_broken_project_configuration_is_carried_as_a_diagnostic(self) -> None:
        self.project("broken", "[briefings.morning]\nguidance = 12\n")
        manifest = briefing.collect("morning", "2026-08-31")
        self.assertEqual(manifest["sources"], [])
        self.assertEqual(len(manifest["diagnostics"]), 1)
        self.assertEqual(manifest["diagnostics"][0]["project"], "projects/broken")

    def test_collection_writes_the_page_before_anyone_writes_the_prose(self) -> None:
        # The page is what the owner opens. Whether the day has one is decided
        # by the collection and not by anything a model chooses to do next.
        self.declare("the-den")
        briefing.collect("morning", "2026-08-31")
        rendered = (briefing.run_dir("2026-08-31") / "briefing.html").read_text(
            encoding="utf-8"
        )
        self.assertIn("The Den", rendered)
        self.assertIn("call the dentist", rendered)
        self.assertIn("no prose yet", rendered)

    def test_publishing_keeps_the_prose_and_renders_the_same_run(self) -> None:
        self.declare("the-den")
        briefing.collect("morning", "2026-08-31")
        self.assertFalse(briefing.delivered("2026-08-31"))
        result = briefing.publish(
            "2026-08-31", "Good morning. Two tasks are left over."
        )
        self.assertTrue(briefing.delivered("2026-08-31"))
        markdown = Path(result["markdown"]).read_text(encoding="utf-8")
        rendered = Path(result["page"]).read_text(encoding="utf-8")
        self.assertEqual(markdown, "Good morning. Two tasks are left over.\n")
        self.assertIn("Two tasks are left over.", rendered)
        self.assertIn("The Den", rendered)
        self.assertIn("call the dentist", rendered)
        # The page collection wrote is replaced, not left beside the prose.
        self.assertNotIn("no prose yet", rendered)

    def test_publishing_a_run_that_is_not_there_is_refused(self) -> None:
        with self.assertRaises(briefing.Refused):
            briefing.publish("2026-08-31", "Good morning.")
        with self.assertRaises(briefing.Refused):
            briefing.publish("not-a-date", "Good morning.")

    def test_only_the_last_runs_are_kept(self) -> None:
        root = briefing.state_root()
        root.mkdir(parents=True)
        for day in range(1, 9):
            (root / f"2026-08-0{day}").mkdir()
            (root / f"2026-08-0{day}" / "manifest.json").write_text("{}")
        (root / "not-a-run").mkdir()
        briefing.prune(keep=3)
        kept = sorted(path.name for path in root.iterdir())
        self.assertEqual(kept, ["2026-08-06", "2026-08-07", "2026-08-08", "not-a-run"])


class Page(unittest.TestCase):
    def run_of(self, *contributions: dict, prose: str | None = None) -> dict:
        return {
            "manifest": {
                "version": 1,
                "profile": "morning",
                "date": "2026-08-31",
                "state": "delivered",
                "started": "2026-08-31T07:30:00+03:00",
                "finished": "2026-08-31T07:33:00+03:00",
                "sources": [
                    {key: item[key] for key in ("project", "status", "headline")}
                    for item in contributions
                ],
                "diagnostics": [],
            },
            "contributions": list(contributions),
            "prose": prose,
        }

    def contribution(self, **overrides: object) -> dict:
        return {
            "project": "personal/the-den",
            "slug": "personal-the-den",
            "title": "The Den",
            "status": "ok",
            "headline": "Nothing needs you.",
            "facts": [],
            "body": "",
            **overrides,
        }

    def test_what_a_source_writes_can_never_become_markup(self) -> None:
        rendered = page.render_page(
            self.run_of(
                self.contribution(
                    title="<script>alert(1)</script>",
                    headline="a & b <b>bold</b>",
                    body="<img src=x onerror=alert(1)>\n\n[go](javascript:alert(1))",
                )
            )
        )
        self.assertNotIn("<script>alert", rendered)
        self.assertNotIn("<img src=x", rendered)
        self.assertNotIn("javascript:", rendered)
        self.assertIn("&lt;script&gt;", rendered)
        # The words of an unsafe link survive; only the link is dropped.
        self.assertIn("go", rendered)

    def test_the_page_needs_nothing_from_the_network(self) -> None:
        rendered = page.render_page(self.run_of(self.contribution()))
        for reached in ("<script", "http://", "https://", "@import", "src="):
            self.assertNotIn(reached, rendered)

    def test_every_source_and_its_facts_are_on_the_page(self) -> None:
        rendered = page.render_page(
            self.run_of(
                self.contribution(
                    facts=[{"label": "Volume", "value": "4200 kg"}],
                    body="A **hard** session.",
                ),
                self.contribution(
                    project="personal/seedzero",
                    title="Seed Zero",
                    status="failed",
                    headline="the source did not answer within 900 seconds",
                ),
                prose="Good morning.",
            )
        )
        self.assertIn("Good morning.", rendered)
        self.assertIn("Volume", rendered)
        self.assertIn("4200 kg", rendered)
        self.assertIn("<strong>hard</strong>", rendered)
        self.assertIn('class="pill failed"', rendered)
        self.assertIn("did not answer within 900 seconds", rendered)
        self.assertIn("Monday, 31 August 2026", rendered)

    def test_a_run_without_prose_says_so_rather_than_inventing_one(self) -> None:
        rendered = page.render_page(self.run_of(self.contribution(), prose=None))
        self.assertIn("no prose yet", rendered)

    def test_a_link_whose_label_is_a_path_in_backticks_survives(self) -> None:
        # Most links a briefing writes look like this, and cutting the code
        # spans out first used to leave the whole thing as literal Markdown.
        self.assertEqual(
            page.inline("[`docs/a.md`](https://x.invalid/a)"),
            '<a href="https://x.invalid/a" rel="noreferrer"><code>docs/a.md</code></a>',
        )
        # A target this page will not follow keeps its words and loses the link.
        self.assertEqual(page.inline("[go](javascript:alert(1)) after"), "go after")
        # A target with brackets in it is one target, not a truncated one.
        self.assertIn(
            'href="https://x.invalid/Set_(maths)"',
            page.inline("[set](https://x.invalid/Set_(maths))"),
        )
        # Markdown inside a code span stays text.
        self.assertEqual(page.inline("`[x](y)`"), "<code>[x](y)</code>")

    def test_the_markdown_a_briefing_writes(self) -> None:
        rendered = page.markdown(
            "## Yesterday\n\n"
            "- one\n- two\n\n"
            "1. first\n2. second\n\n"
            "> a quote\n\n"
            "---\n\n"
            "```\nplain code\n```\n\n"
            "A line with `code` and a [link](https://example.invalid/x)."
        )
        self.assertIn("<h4>Yesterday</h4>", rendered)
        self.assertIn("<ul>\n<li>one</li>", rendered)
        self.assertIn("<ol>\n<li>first</li>", rendered)
        self.assertIn("<blockquote>a quote</blockquote>", rendered)
        self.assertIn("<hr>", rendered)
        self.assertIn("<pre><code>plain code</code></pre>", rendered)
        self.assertIn("<code>code</code>", rendered)
        self.assertIn('href="https://example.invalid/x"', rendered)


if __name__ == "__main__":
    unittest.main()
