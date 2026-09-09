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

#: Reports where it was run from, so a source's working directory is checked
#: against what it was told rather than against what it said.
LOCATING = """#!/usr/bin/env python3
import os
import pathlib
with pathlib.Path(os.environ["BRIEFING_WHERE"]).open("a") as stream:
    stream.write(os.getcwd() + "\\n")
print(pathlib.Path(os.environ["BRIEFING_ANSWER"]).read_text())
"""

#: Marks when it starts and when it stops, so two sources running at once can
#: be told apart from two running one after the other.
OVERLAPPING = """#!/usr/bin/env python3
import os
import pathlib
import time
where = pathlib.Path(os.environ["BRIEFING_WHERE"])
with where.open("a") as stream:
    stream.write("in\\n")
time.sleep(0.4)
with where.open("a") as stream:
    stream.write("out\\n")
print(pathlib.Path(os.environ["BRIEFING_ANSWER"]).read_text())
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
            {
                "project": "personal/seedzero",
                "slug": "personal-seedzero",
                "harness": "claude",
                "model": "opus",
            },
            "the harness exited 1:\n" + "detail " * 200,
        )
        self.assertEqual(len(entry["headline"]), briefing.MAX_HEADLINE)
        self.assertNotIn("\n", entry["headline"])
        self.assertEqual(entry["status"], "failed")
        self.assertEqual(entry["slug"], "personal-seedzero")

    def test_a_slug_that_is_not_one_path_component_never_names_a_file(self) -> None:
        # The reader hands the slug over and this is what writes a file with
        # it. Anything that is not one component is refused a name of its own.
        for named in ("../escape", "with/slash", "", ".hidden"):
            with self.subTest(slug=named):
                entry = briefing.failed_contribution(
                    {"project": "personal/seedzero", "slug": named}, "no"
                )
                self.assertEqual(entry["slug"], "unknown")

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
            "root": "/tmp",
            "harness": "pi",
            "model": "openai-codex/gpt-5.6-sol",
            "thinking": "medium",
        }
        argv = briefing.harness_argv(source, "the prompt")
        self.assertEqual(argv[0], "pi")
        self.assertIn("--print", argv)
        self.assertIn("--no-session", argv)
        self.assertIn("--no-extensions", argv)
        self.assertEqual(argv[-1], "the prompt")

    def test_a_source_runs_with_every_tool_its_harness_has(self) -> None:
        # There was an allowlist here and it was never what it looked like:
        # `bash` was always in it, so a source that meant to write could
        # always write. What a source may do is what its guidance says, and
        # that is in the prompt where the source can read it.
        for harness in ("pi", "claude"):
            with self.subTest(harness=harness):
                argv = briefing.harness_argv(
                    {
                        "project": "personal/seedzero",
                        "root": "/tmp",
                        "harness": harness,
                        "model": "opus",
                        "thinking": "high",
                    },
                    "the prompt",
                )
                for withheld in (
                    "--tools",
                    "--disallowed-tools",
                    "--exclude-tools",
                    "--no-tools",
                    "--no-skills",
                    "--disable-slash-commands",
                ):
                    self.assertNotIn(withheld, argv)
                self.assertEqual(argv[-1], "the prompt")

    def test_the_second_asking_is_still_given_no_tools(self) -> None:
        # The repair is handed the source's own answer to say again
        # correctly. It reads nothing and runs nothing, and opening the tools
        # up for the first asking must not open them here.
        source = {
            "project": "personal/seedzero",
            "root": "/tmp",
            "harness": "pi",
            "model": "opus",
            "thinking": "high",
        }
        pi_argv = briefing.harness_argv(source, "again", tools=False)
        self.assertIn("--no-tools", pi_argv)
        claude_argv = briefing.harness_argv(
            {**source, "harness": "claude"}, "again", tools=False
        )
        self.assertEqual(claude_argv[claude_argv.index("--tools") + 1], "")
        self.assertIn("--disable-slash-commands", claude_argv)
        # And it is asked cheaply, whatever the source usually costs.
        self.assertEqual(
            claude_argv[claude_argv.index("--effort") + 1], briefing.REPAIR_THINKING
        )

    def test_both_harnesses_answer_without_asking_anyone(self) -> None:
        # Nobody is watching a source run, so a question it cannot ask is a
        # refusal. `dontAsk` sandboxes the shell, and sources reported `gh`
        # and `python3` denied instead of reporting their project.
        source = {
            "project": "personal/seedzero",
            "root": "/tmp",
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
                "root": "/home/x/the-den",
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
        # Every tool is present, so the prompt is where the limit lives. A
        # source is told plainly that its guidance is the whole of what it
        # may do, because nothing else stops it.
        self.assertIn("every tool this harness has", prompt)
        self.assertIn("unless that guidance names it", prompt)
        self.assertIn("the whole of your permission", prompt)
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
        self.where = self.root / "where.txt"
        # The machine's own sources and the home a source with no root runs in
        # are both under the fixture, so no test reads the developer's own.
        self.config = self.root / "config"
        self.home = self.root / "home"
        self.home.mkdir()
        self.answer.write_text(
            f"```json\n{json.dumps(ENVELOPE)}\n```\n", encoding="utf-8"
        )
        self.environment = mock.patch.dict(
            os.environ,
            {
                "PATH": f"{self.bin}:{os.environ['PATH']}",
                "HOME": str(self.home),
                "XDG_STATE_HOME": str(self.root / "state"),
                "XDG_CONFIG_HOME": str(self.config),
                "SCUFRIS_PROJECT_ROOTS": json.dumps([str(self.projects)]),
                "BRIEFING_ANSWER": str(self.answer),
                "BRIEFING_PROMPT": str(self.prompt),
                "BRIEFING_WHERE": str(self.where),
            },
        )
        self.environment.start()
        self.addCleanup(self.environment.stop)
        os.environ.pop("SCUFRIS_CONFIG", None)
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

    def declare(self, name: str, harness: str = "pi", *profiles: str) -> Path:
        """A project that declares one briefing for each profile named."""
        return self.project(
            name,
            "".join(
                f"[briefings.{profile}]\n"
                f'description = "Report {name}."\n'
                f'keywords = {{ harness = "{harness}" }}\n'
                f'guidance = "Read {name} and report it."\n'
                for profile in (profiles or ("morning",))
            ),
        )

    def machine(self, text: str, name: str = "config.toml") -> Path:
        """The user-level file the machine declares its own sources in."""
        path = self.config / "scufris" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    @staticmethod
    def section(name: str, guidance: str, root: str | None = None) -> str:
        placed = f'root = "{root}"\n' if root is not None else ""
        return (
            f"[briefings.morning.{name}]\n"
            f'description = "Report {name}."\n'
            'keywords = { harness = "pi" }\n'
            f'guidance = "{guidance}"\n'
            f"{placed}"
        )

    def control(self, program: str) -> Path:
        """A stand-in for scufris-ctl, which the wake carries the words to."""
        executable = self.bin / "scufris-ctl"
        executable.write_text(program, encoding="utf-8")
        executable.chmod(0o755)
        return executable

    def test_two_sources_offers_become_one_numbered_list_on_the_run(self) -> None:
        # The numbers belong to the run, not to any source. They are assigned
        # once, in source order, and stored, so a pick made later resolves from
        # the file rather than from what the model remembers saying.
        # Both sources answer the same, because sources run in one pool and
        # which of two scripted answers reaches which of them is a race. What
        # is being checked here is the merge, and it is the same either way:
        # every source's offers, in the order the sources were asked.
        self.declare("aaa")
        self.declare("zzz")
        self.answer.write_text(
            json.dumps(
                {
                    **ENVELOPE,
                    "offers": [
                        {"label": "First here", "detail": "a"},
                        {"label": "Second here", "detail": "b"},
                    ],
                }
            ),
            encoding="utf-8",
        )
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual([item["number"] for item in manifest["offers"]], [1, 2, 3, 4])
        self.assertEqual(
            [item["project"] for item in manifest["offers"]],
            ["projects/aaa", "projects/aaa", "projects/zzz", "projects/zzz"],
        )
        self.assertEqual(
            [item["label"] for item in manifest["offers"]],
            ["First here", "Second here", "First here", "Second here"],
        )

    def test_a_run_nobody_offered_anything_for_carries_an_empty_list(self) -> None:
        self.declare("the-den")
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(manifest["offers"], [])
        page_text = (
            briefing.run_dir("2026-08-31", "morning") / "briefing.html"
        ).read_text()
        self.assertNotIn("<h2>Next</h2>", page_text)

    def test_the_offers_a_run_was_collected_with_survive_publish(self) -> None:
        # Publishing writes the prose and renders the page again. It must not
        # renumber or drop the list, or the briefing said in chat and the one
        # stored would disagree about what 1 means.
        self.declare("the-den")
        self.answers(
            json.dumps(
                {**ENVELOPE, "offers": [{"label": "Call the dentist", "detail": "a"}]}
            )
        )
        collected = briefing.collect("2026-08-31", "morning")
        briefing.publish("2026-08-31", "morning", "Today is quiet.")
        after = briefing.read_manifest("2026-08-31", "morning")
        self.assertEqual(after["offers"], collected["offers"])
        page_text = (
            briefing.run_dir("2026-08-31", "morning") / "briefing.html"
        ).read_text()
        self.assertIn("<h2>Next</h2>", page_text)
        self.assertIn("Call the dentist", page_text)

    def test_a_source_is_told_what_it_may_offer(self) -> None:
        self.declare("the-den")
        briefing.collect("2026-08-31", "morning")
        asked = self.prompt.read_text()
        self.assertIn('"offers"', asked)
        self.assertIn("at most 3 things the owner could do next", asked)
        self.assertIn("whoever picks it writes the words for it then", asked)

    def test_the_wake_names_the_numbered_list_only_when_there_is_one(self) -> None:
        quiet = {
            "profile": "morning",
            "date": "2026-08-31",
            "sources": [],
            "offers": [],
        }
        self.assertNotIn("numbered", briefing.wake_message(quiet))
        loud = {
            **quiet,
            "offers": [{"number": 1, "label": "One", "detail": "a"}],
        }
        said = briefing.wake_message(loud)
        self.assertIn("1 thing that could be done next", said)
        self.assertIn("using those numbers exactly", said)

    def test_a_source_the_machine_declares_contributes_with_no_project(self) -> None:
        # A jobs source has no checkout to belong to. It is an ordinary source
        # declared in one user-level file, and nothing else about it differs.
        self.machine(self.section("jobs", "Read the job history."))
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual([item["project"] for item in manifest["sources"]], ["@jobs"])
        self.assertEqual(manifest["state"], "collected")
        self.assertIn("Read the job history.", self.prompt.read_text())
        kept = json.loads(
            (
                briefing.run_dir("2026-08-31", "morning")
                / "contributions"
                / "@jobs.json"
            ).read_text()
        )
        self.assertEqual(kept["body"], ENVELOPE["body"].strip())

    def test_a_machine_source_can_never_take_a_project_contribution_file(
        self,
    ) -> None:
        # The reader assigns the slug and namespaces the machine's own, so a
        # section named for a project cannot be written into that project's
        # file. The collision is not expressible rather than merely noticed.
        self.declare("the-den")
        self.machine(
            self.section("jobs", "Read the job history.")
            + self.section("projects-the-den", "Take the den's file.")
        )
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(
            [item["slug"] for item in manifest["sources"]],
            ["@jobs", "@projects-the-den", "projects-the-den"],
        )
        kept = sorted(
            path.name
            for path in (
                briefing.run_dir("2026-08-31", "morning") / "contributions"
            ).iterdir()
        )
        self.assertEqual(
            kept, ["@jobs.json", "@projects-the-den.json", "projects-the-den.json"]
        )

    def test_a_machine_source_runs_in_home_unless_it_names_a_root(self) -> None:
        elsewhere = self.root / "elsewhere"
        elsewhere.mkdir()
        self.machine(
            self.section("jobs", "Read the job history.")
            + self.section("den", "Read the journal.", root=str(elsewhere))
        )
        self.harness(LOCATING)
        briefing.collect("2026-08-31", "morning")
        self.assertEqual(
            sorted(self.where.read_text().split()),
            sorted([str(elsewhere.resolve()), str(self.home.resolve())]),
        )

    def test_a_configuration_somebody_named_and_is_not_there_is_refused(self) -> None:
        # A typed path that quietly reports no sources is a morning discovered
        # too late. A default path that is absent is a machine with none.
        typo = self.root / "typo.toml"
        with self.assertRaises(briefing.Refused) as caught:
            briefing.declared_sources("morning", str(typo))
        self.assertIn("typo.toml", str(caught.exception))
        self.assertEqual(briefing.declared_sources("morning"), ([], []))

    def test_a_named_configuration_is_read_instead_of_the_default(self) -> None:
        self.machine(self.section("jobs", "Read the job history."))
        other = self.root / "other.toml"
        other.write_text(self.section("named", "Read the named file."), "utf-8")
        sources, _ = briefing.declared_sources("morning", str(other))
        self.assertEqual([item["project"] for item in sources], ["@named"])

    def test_a_malformed_user_file_costs_the_user_file_and_nothing_else(self) -> None:
        self.declare("the-den")
        path = self.machine("this is not = toml [\n")
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(
            [item["project"] for item in manifest["sources"]], ["projects/the-den"]
        )
        self.assertEqual(len(manifest["diagnostics"]), 1)
        diagnostic = manifest["diagnostics"][0]
        self.assertEqual(diagnostic["project"], str(path.resolve()))
        self.assertIn("ignored", diagnostic["diagnostic"])

    def test_the_user_file_declares_briefings_and_nothing_else(self) -> None:
        # A checkout has to work for someone whose machine has none of this,
        # so agents and conventions stay where the project keeps them.
        self.declare("the-den")
        self.machine(
            self.section("jobs", "Read the job history.")
            + '[agents.work]\ndescription = "Implement a change."\n'
        )
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(
            [item["project"] for item in manifest["sources"]], ["projects/the-den"]
        )
        self.assertIn(
            "briefings and nothing else", manifest["diagnostics"][0]["diagnostic"]
        )

    def test_the_first_run_of_a_profile_invents_no_window(self) -> None:
        self.declare("the-den")
        briefing.collect("2026-08-31", "morning")
        self.assertIn("No earlier morning briefing was kept", self.prompt.read_text())

    def test_a_source_is_told_when_its_profile_last_began(self) -> None:
        self.declare("the-den", "pi", "morning", "evening")
        first = briefing.collect("2026-08-31", "morning")
        self.assertIsNotNone(first["started"])
        # A profile's watermark is its own: an evening that never ran has none
        # of the morning's.
        briefing.collect("2026-08-31", "evening")
        self.assertIn("No earlier evening briefing was kept", self.prompt.read_text())
        briefing.collect("2026-09-01", "morning")
        said = self.prompt.read_text()
        # The start and not the finish. The last run's sources were asked at
        # about its start, so a collection that took half an hour would leave
        # that half hour reported by neither briefing.
        self.assertIn(
            f"The last morning briefing asked its sources at {first['started']}", said
        )
        # A fact, not an instruction: guidance that names its own window keeps
        # it, which is what every source written before this said.
        self.assertIn("Where it names its own window, keep it.", said)

    def test_the_watermark_is_the_start_even_when_a_run_never_finished(self) -> None:
        # A run left collecting by a crash still asked its sources. The moment
        # it asked them is the one the next run measures from; a finish it
        # never reached would leave that run's window open forever.
        self.declare("the-den", "pi", "morning")
        first = briefing.collect("2026-08-31", "morning")
        manifest = briefing.read_manifest("2026-08-31", "morning")
        briefing.write_manifest({**manifest, "state": "collecting", "finished": None})
        briefing.collect("2026-09-01", "morning")
        self.assertIn(
            f"The last morning briefing asked its sources at {first['started']}",
            self.prompt.read_text(),
        )

    def test_only_a_project_that_declares_the_profile_is_asked(self) -> None:
        self.declare("the-den")
        self.project(
            "quiet",
            '[agents.work]\ndescription = "Implement a change."\n',
        )
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(
            [item["project"] for item in manifest["sources"]], ["projects/the-den"]
        )
        self.assertEqual(manifest["state"], "collected")
        self.assertIn("Read the-den and report it.", self.prompt.read_text())

    def test_a_contribution_is_kept_beside_a_manifest_that_indexes_it(self) -> None:
        self.declare("the-den")
        manifest = briefing.collect("2026-08-31", "morning")
        entry = manifest["sources"][0]
        self.assertEqual(entry["status"], "attention")
        self.assertEqual(entry["headline"], ENVELOPE["headline"])
        # The index carries what a reader chooses by and not the body.
        self.assertNotIn("body", entry)
        kept = json.loads(
            (
                briefing.run_dir("2026-08-31", "morning")
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
        manifest = briefing.collect("2026-08-31", "morning")
        entry = manifest["sources"][0]
        self.assertEqual(entry["status"], "failed")
        self.assertIn("not one JSON envelope", entry["headline"])
        kept = json.loads(
            (
                briefing.run_dir("2026-08-31", "morning")
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
        manifest = briefing.collect("2026-08-31", "morning")
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
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(manifest["sources"][0]["status"], "attention")
        self.assertIn("longer than 200 characters", self.asked()[1])

    def test_a_source_that_cannot_say_it_twice_is_failed_and_says_so(self) -> None:
        self.declare("seedzero")
        self.answers("I could not read the channel today.")
        manifest = briefing.collect("2026-08-31", "morning")
        entry = manifest["sources"][0]
        self.assertEqual(entry["status"], "failed")
        self.assertIn("not one JSON envelope", entry["headline"])
        self.assertIn("again when asked to correct it", entry["headline"])
        self.assertEqual(len(self.asked()), 2)

    def test_the_second_asking_reads_nothing_and_runs_nothing(self) -> None:
        argv = briefing.harness_argv(
            {
                "project": "personal/seedzero",
                "root": "/tmp",
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
                "root": "/tmp",
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
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(manifest["sources"][0]["status"], "failed")
        self.assertNotIn("again when asked", manifest["sources"][0]["headline"])

    def test_a_source_with_no_time_left_is_not_asked_twice(self) -> None:
        self.declare("seedzero")
        self.answers("not an envelope at all")
        manifest = briefing.collect(
            "2026-08-31", "morning", source_deadline=briefing.REPAIR_FLOOR - 1
        )
        self.assertEqual(manifest["sources"][0]["status"], "failed")
        self.assertEqual(len(self.asked()), 1)

    def test_the_second_asking_is_bounded_like_every_other_run(self) -> None:
        # The first asking answers badly at once; the second never returns.
        self.declare("seedzero")
        self.answers("not an envelope at all", "#sleep")
        with mock.patch.dict(os.environ, {"SCUFRIS_BRIEFING_REPAIR_DEADLINE": "0.4"}):
            manifest = briefing.collect("2026-08-31", "morning")
        entry = manifest["sources"][0]
        self.assertEqual(entry["status"], "failed")
        # The repair timed out, so what is reported is what the source did say.
        self.assertIn("not one JSON envelope", entry["headline"])
        self.assertNotIn("again when asked", entry["headline"])
        self.assertEqual(len(self.asked()), 2)

    def test_bytes_that_are_not_text_are_refused_rather_than_raised(self) -> None:
        self.declare("the-den")
        self.harness(BINARY)
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(manifest["sources"][0]["status"], "failed")
        self.assertEqual(manifest["state"], "failed")

    def test_a_harness_that_exits_badly_is_named_rather_than_guessed(self) -> None:
        self.declare("the-den")
        self.harness(FAILING)
        manifest = briefing.collect("2026-08-31", "morning")
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
            manifest = briefing.collect("2026-08-31", "morning")
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
            manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(manifest["state"], "collected")
        self.assertFalse(
            (briefing.run_dir("2026-08-31", "morning") / "briefing.html").exists()
        )
        said = " ".join(item["diagnostic"] for item in manifest["diagnostics"])
        self.assertIn("the page could not be rendered", said)
        kept = json.loads(
            (briefing.run_dir("2026-08-31", "morning") / "manifest.json").read_text()
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
            manifest = briefing.collect("2026-08-31", "morning")
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
        manifest = briefing.collect("2026-08-31", "morning", source_deadline=0.5)
        self.assertEqual(manifest["state"], "failed")
        self.assertEqual(len(manifest["sources"]), 2)
        for entry in manifest["sources"]:
            self.assertEqual(entry["status"], "failed")
            self.assertIn("did not answer within", entry["headline"])

    def test_a_morning_with_no_source_is_still_a_run(self) -> None:
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(manifest["sources"], [])
        self.assertEqual(manifest["state"], "collected")
        self.assertFalse(briefing.delivered("2026-08-31", "morning"))
        rendered = (
            briefing.run_dir("2026-08-31", "morning") / "briefing.html"
        ).read_text(encoding="utf-8")
        self.assertIn("No project declared this briefing.", rendered)

    def test_a_broken_project_configuration_is_carried_as_a_diagnostic(self) -> None:
        self.project("broken", "[briefings.morning]\nguidance = 12\n")
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(manifest["sources"], [])
        self.assertEqual(len(manifest["diagnostics"]), 1)
        self.assertEqual(manifest["diagnostics"][0]["project"], "projects/broken")

    def test_collection_writes_the_page_before_anyone_writes_the_prose(self) -> None:
        # The page is what the owner opens. Whether the day has one is decided
        # by the collection and not by anything a model chooses to do next.
        self.declare("the-den")
        briefing.collect("2026-08-31", "morning")
        rendered = (
            briefing.run_dir("2026-08-31", "morning") / "briefing.html"
        ).read_text(encoding="utf-8")
        self.assertIn("The Den", rendered)
        self.assertIn("call the dentist", rendered)
        self.assertIn("no prose yet", rendered)

    def test_publishing_keeps_the_prose_and_renders_the_same_run(self) -> None:
        self.declare("the-den")
        briefing.collect("2026-08-31", "morning")
        self.assertFalse(briefing.delivered("2026-08-31", "morning"))
        result = briefing.publish(
            "2026-08-31", "morning", "Good morning. Two tasks are left over."
        )
        self.assertTrue(briefing.delivered("2026-08-31", "morning"))
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
            briefing.publish("2026-08-31", "morning", "Good morning.")
        with self.assertRaises(briefing.Refused):
            briefing.publish("not-a-date", "morning", "Good morning.")

    def test_two_profiles_on_one_date_are_two_runs_and_two_deliveries(self) -> None:
        # One date is not one briefing. A morning and an evening on the same
        # day are two runs, each with its own directory, page and prose.
        self.declare("the-den", "pi", "morning", "evening")
        morning = briefing.collect("2026-08-31", "morning")
        evening = briefing.collect("2026-08-31", "evening")
        self.assertEqual(morning["state"], "collected")
        self.assertEqual(evening["state"], "collected")
        self.assertNotEqual(
            briefing.run_dir("2026-08-31", "morning"),
            briefing.run_dir("2026-08-31", "evening"),
        )
        for profile in ("morning", "evening"):
            directory = briefing.run_dir("2026-08-31", profile)
            self.assertTrue((directory / "manifest.json").is_file())
            self.assertTrue(
                (directory / "contributions" / "projects-the-den.json").is_file()
            )
        first = briefing.publish("2026-08-31", "morning", "The morning.")
        second = briefing.publish("2026-08-31", "evening", "The evening.")
        self.assertNotEqual(first["page"], second["page"])
        self.assertIn("The morning.", Path(first["page"]).read_text(encoding="utf-8"))
        self.assertIn("The evening.", Path(second["page"]).read_text(encoding="utf-8"))
        self.assertTrue(briefing.delivered("2026-08-31", "morning"))
        self.assertTrue(briefing.delivered("2026-08-31", "evening"))

    def test_a_run_without_a_profile_is_the_one_waiting_to_be_written(self) -> None:
        # Naming no profile means the one obvious run. One briefing waiting
        # for its prose is that run; two are two briefings, and guessing
        # between them would put one's prose on the other's page.
        self.declare("the-den", "pi", "morning", "evening")
        briefing.collect("2026-08-31", "morning")
        self.assertEqual(briefing.resolve("2026-08-31"), "morning")
        briefing.collect("2026-08-31", "evening")
        with self.assertRaises(briefing.Refused) as refused:
            briefing.resolve("2026-08-31")
        self.assertIn("evening", str(refused.exception))
        self.assertIn("morning", str(refused.exception))
        # Named, each publishes into its own run and neither touches the
        # other's.
        for profile in ("morning", "evening"):
            briefing.publish("2026-08-31", profile, f"The {profile}.")
            self.assertEqual(
                (briefing.run_dir("2026-08-31", profile) / "briefing.md").read_text(
                    encoding="utf-8"
                ),
                f"The {profile}.\n",
            )
        # A delivered run is never what an unnamed publish lands on.
        with self.assertRaises(briefing.Refused):
            briefing.resolve("2026-08-31", undelivered=True)

    def test_the_last_run_of_a_finished_day_is_read_without_naming_it(self) -> None:
        # A person reading yesterday names a date and nothing else. One run is
        # unambiguous whether or not it was written up.
        self.declare("the-den")
        briefing.collect("2026-08-31", "morning")
        briefing.publish("2026-08-31", "morning", "Good morning.")
        self.assertEqual(briefing.resolve("2026-08-31"), "morning")
        self.assertEqual(
            briefing.read_run("2026-08-31", "morning")["prose"], "Good morning.\n"
        )
        # A date nothing ran on answers with the default, so the refusal a
        # caller reads is about the missing run and not about a missing name.
        self.assertEqual(briefing.resolve("2026-08-30"), "morning")

    def test_a_gathered_run_is_what_a_session_finds_waiting(self) -> None:
        self.declare("the-den", "pi", "morning", "evening")
        briefing.collect("2026-08-31", "morning")
        briefing.collect("2026-08-31", "evening")
        self.assertEqual(
            sorted(item["profile"] for item in briefing.pending("2026-08-31")),
            ["evening", "morning"],
        )
        briefing.publish("2026-08-31", "morning", "The morning.")
        self.assertEqual(
            [item["profile"] for item in briefing.pending("2026-08-31")], ["evening"]
        )

    def test_a_briefing_no_project_declared_is_not_waiting_for_anyone(self) -> None:
        # A run is still recorded, so nothing collects it twice. Nobody is
        # woken for it: a morning nothing declared is not an event.
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(manifest["state"], "collected")
        self.assertEqual(briefing.pending("2026-08-31"), [])

    def test_the_wake_asks_for_one_briefing_and_names_what_could_not_answer(
        self,
    ) -> None:
        said = briefing.wake_message(
            {
                "date": "2026-08-31",
                "profile": "morning",
                "sources": [
                    {"project": "personal/the-den", "status": "ok"},
                    {"project": "personal/seedzero", "status": "failed"},
                ],
            }
        )
        self.assertIn("1 source answered, 1 could not answer", said)
        self.assertIn("scufris_briefing_show", said)
        self.assertIn("scufris_briefing_publish", said)
        self.assertIn("profile morning", said)
        self.assertIn("Do not read the sources out one after another", said)
        self.assertIn("Name any source that could not answer", said)
        quiet = briefing.wake_message(
            {
                "date": "2026-08-31",
                "profile": "morning",
                "sources": [
                    {"project": "personal/the-den", "status": "ok"},
                    {"project": "personal/seedzero", "status": "attention"},
                ],
            }
        )
        self.assertIn("2 sources answered.", quiet)
        self.assertNotRegex(quiet, r", \d+ could not answer")

    def test_a_gathered_run_is_carried_to_the_conversation(self) -> None:
        self.declare("the-den")
        briefing.collect("2026-08-31", "morning")
        kept = self.root / "woken.json"
        self.control(
            "#!/usr/bin/env python3\n"
            "import json, os, pathlib, sys\n"
            f"pathlib.Path({str(kept)!r}).write_text(json.dumps(sys.argv[1:]))\n"
        )
        answer = briefing.wake("2026-08-31", "morning")
        self.assertTrue(answer["woken"], answer)
        argv = json.loads(kept.read_text())
        self.assertEqual(argv[0], "wake")
        self.assertIn("morning briefing for 2026-08-31", argv[1])
        self.assertEqual(argv[2:4], ["--custom-type", briefing.BRIEFING_WAKE])
        self.assertEqual(
            json.loads(argv[5]),
            {"date": "2026-08-31", "profile": "morning", "sources": 1},
        )

    def test_a_run_where_every_source_failed_still_reaches_the_conversation(
        self,
    ) -> None:
        # One source failing out of five was reported. Five out of five used to
        # be silence: `wake` refused the state, `pending` left it out, and the
        # unit exited 0, so the briefing simply did not arrive.
        self.declare("the-den")
        self.harness("#!/usr/bin/env python3\nraise SystemExit(1)\n")
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(manifest["state"], "failed")
        kept = self.root / "woken.json"
        self.control(
            "#!/usr/bin/env python3\n"
            "import json, pathlib, sys\n"
            f"pathlib.Path({str(kept)!r}).write_text(json.dumps(sys.argv[1:]))\n"
        )
        answer = briefing.wake("2026-08-31", "morning")
        self.assertTrue(answer["woken"], answer)
        said = json.loads(kept.read_text())[1]
        self.assertIn("did not collect", said)
        self.assertIn("the-den", said)
        self.assertIn("Claim nothing about what they would have said", said)

    def test_a_run_records_the_bounds_it_was_actually_given(self) -> None:
        # The profile's numbers live in the timer unit's environment. A run
        # started any other way - the `scufris_briefing_run` tool under the
        # service, or a shell - gets the code defaults instead, and nothing
        # said so: a source cut off at 15 minutes looked the same as one cut
        # off at the 8 hours the profile asks for.
        self.declare("the-den")
        self.harness("#!/usr/bin/env python3\nprint('all clear')\n")
        manifest = briefing.collect("2026-08-31", "morning")
        self.assertEqual(
            manifest["bounds"],
            {
                "source_deadline": briefing.SOURCE_DEADLINE,
                "run_deadline": briefing.RUN_DEADLINE,
                "parallel": 1,
            },
        )
        with mock.patch.dict(
            os.environ,
            {
                "SCUFRIS_BRIEFING_SOURCE_DEADLINE": "120",
                "SCUFRIS_BRIEFING_DEADLINE": "240",
            },
        ):
            bounded = briefing.collect("2026-09-01", "morning")
        self.assertEqual(bounded["bounds"]["source_deadline"], 120.0)
        self.assertEqual(bounded["bounds"]["run_deadline"], 240.0)

    def test_a_failed_run_is_still_waiting_when_nothing_was_connected(self) -> None:
        # The wake is only the delivery. The 23:00 nightly with nothing
        # connected was refused its wake and then excluded from `pending`, so
        # the session-start read - the one thing that recovers a refused wake -
        # never saw it, and the run sat on disk failed forever.
        self.declare("the-den")
        self.harness("#!/usr/bin/env python3\nraise SystemExit(1)\n")
        self.assertEqual(briefing.collect("2026-08-31", "morning")["state"], "failed")
        self.control(
            "#!/usr/bin/env python3\n"
            "import sys\n"
            'print("scufris-ctl: agent_unavailable: no agent", file=sys.stderr)\n'
            "raise SystemExit(1)\n"
        )
        self.assertFalse(briefing.wake("2026-08-31", "morning")["woken"])
        waiting = briefing.pending("2026-08-31")
        self.assertEqual([item["profile"] for item in waiting], ["morning"])
        # And it closes the way a collected run closes, so it is asked about
        # once rather than every session: there is no other way to end it.
        self.assertIn("scufris_briefing_publish", briefing.failure_message(waiting[0]))
        briefing.publish(
            "2026-08-31", "morning", "No briefing: the-den could not answer."
        )
        self.assertEqual(briefing.pending("2026-08-31"), [])
        self.assertEqual(briefing.run_state("2026-08-31", "morning"), "delivered")

    def test_a_refused_wake_leaves_the_run_gathered_for_later(self) -> None:
        # Losing a gathered briefing because the agent happened to be down is
        # the failure this must not have. The run is the durable half; the
        # wake is only the delivery.
        self.declare("the-den")
        briefing.collect("2026-08-31", "morning")
        self.control(
            "#!/usr/bin/env python3\n"
            "import sys\n"
            'print("scufris-ctl: agent_unavailable: no agent", file=sys.stderr)\n'
            "raise SystemExit(1)\n"
        )
        answer = briefing.wake("2026-08-31", "morning")
        self.assertFalse(answer["woken"])
        self.assertIn("agent_unavailable", answer["reason"])
        self.assertEqual(briefing.run_state("2026-08-31", "morning"), "collected")
        self.assertEqual(
            [item["profile"] for item in briefing.pending("2026-08-31")], ["morning"]
        )

    def test_a_wake_with_no_control_client_is_reported_and_not_raised(self) -> None:
        self.declare("the-den")
        briefing.collect("2026-08-31", "morning")
        answer = briefing.wake("2026-08-31", "morning", ctl=str(self.root / "missing"))
        self.assertFalse(answer["woken"])
        self.assertIn("could not be run", answer["reason"])
        self.assertEqual(briefing.run_state("2026-08-31", "morning"), "collected")

    def test_a_delivered_run_is_never_woken_for_twice(self) -> None:
        self.declare("the-den")
        briefing.collect("2026-08-31", "morning")
        briefing.publish("2026-08-31", "morning", "Good morning.")
        self.control("#!/usr/bin/env python3\nraise SystemExit(0)\n")
        answer = briefing.wake("2026-08-31", "morning")
        self.assertFalse(answer["woken"])
        self.assertIn("delivered", answer["reason"])

    def test_only_the_last_runs_are_kept(self) -> None:
        root = briefing.state_root()
        root.mkdir(parents=True)
        for day in range(1, 9):
            (root / f"2026-08-0{day}" / "morning").mkdir(parents=True)
            (root / f"2026-08-0{day}" / "morning" / "manifest.json").write_text("{}")
        (root / "not-a-run").mkdir()
        briefing.prune(keep=3)
        kept = sorted(path.name for path in root.iterdir())
        self.assertEqual(kept, ["2026-08-06", "2026-08-07", "2026-08-08", "not-a-run"])

    def test_a_deployment_may_keep_more_days(self) -> None:
        # A source is told when its profile last ran, and that is read back
        # through the days still kept. A profile that runs weekly is told
        # nothing true unless the days outlast it.
        root = briefing.state_root()
        root.mkdir(parents=True)
        for day in range(1, 9):
            (root / f"2026-08-0{day}" / "nightly").mkdir(parents=True)
            (root / f"2026-08-0{day}" / "nightly" / "manifest.json").write_text("{}")
        with mock.patch.dict(os.environ, {"SCUFRIS_BRIEFING_KEEP_DAYS": "6"}):
            briefing.prune()
        self.assertEqual(len(list(root.iterdir())), 6)

    def test_a_profile_may_cap_how_many_sources_run_at_once(self) -> None:
        # Two agents that each fan out into review lanes are already wide at
        # the moment a group is under review. A third project declaring the
        # profile should not quietly widen the night.
        self.declare("aaa", "pi", "nightly")
        self.declare("bbb", "pi", "nightly")
        self.harness(OVERLAPPING)
        with mock.patch.dict(os.environ, {"SCUFRIS_BRIEFING_PARALLEL": "1"}):
            briefing.collect(profile="nightly")
        self.assertEqual(self.where.read_text().split(), ["in", "out", "in", "out"])

    def test_a_profile_run_by_hand_gets_the_deployment_numbers(self) -> None:
        # The bounds only ever reached a run the timer started, because the
        # timer unit exports them. A briefing asked for by hand got the code
        # defaults and said nothing, so a night that allows a source eight hours
        # was cut at fifteen minutes and the work was lost.
        (self.config / "scufris").mkdir(parents=True, exist_ok=True)
        (self.config / "scufris" / briefing.PROFILE_BOUNDS_FILE).write_text(
            json.dumps(
                {
                    "nightly": {
                        "deadline": 28800,
                        "source_deadline": 28800,
                        "parallel": 2,
                        "max_offers": 8,
                        "max_body": 65536,
                        "keep_days": 30,
                    },
                    "morning": {"deadline": 1800, "parallel": None},
                }
            ),
            encoding="utf-8",
        )
        # Three sources and a cap of two, so the cap is what the run records
        # rather than however many happened to declare the profile.
        for name in ("aaa", "bbb", "ccc"):
            self.declare(name, "pi", "nightly")
        # What the CLI does when it resolves the profile a run is asked for.
        briefing.apply_profile_bounds("nightly")
        gathered = briefing.collect(profile="nightly")
        self.assertEqual(
            gathered["bounds"],
            {"source_deadline": 28800.0, "run_deadline": 28800.0, "parallel": 2},
        )
        self.assertEqual(briefing.max_offers(), 8)
        self.assertEqual(briefing.max_body(), 65536)

    def test_a_number_asked_for_by_hand_beats_the_deployment(self) -> None:
        (self.config / "scufris").mkdir(parents=True, exist_ok=True)
        (self.config / "scufris" / briefing.PROFILE_BOUNDS_FILE).write_text(
            json.dumps({"nightly": {"deadline": 28800, "source_deadline": 28800}}),
            encoding="utf-8",
        )
        self.declare("aaa", "pi", "nightly")
        with mock.patch.dict(os.environ, {"SCUFRIS_BRIEFING_DEADLINE": "240"}):
            briefing.apply_profile_bounds("nightly")
            gathered = briefing.collect(profile="nightly")
        self.assertEqual(gathered["bounds"]["run_deadline"], 240.0)

    def test_an_unusable_profile_file_reads_as_if_it_said_nothing(self) -> None:
        (self.config / "scufris").mkdir(parents=True, exist_ok=True)
        for written in ("", "not json", "[]", '{"nightly": 5}', '{"nightly": {}}'):
            (self.config / "scufris" / briefing.PROFILE_BOUNDS_FILE).write_text(
                written, encoding="utf-8"
            )
            for variable in briefing.PROFILE_BOUNDS.values():
                os.environ.pop(variable, None)
            briefing.apply_profile_bounds("nightly")
            self.assertEqual(
                [
                    variable
                    for variable in briefing.PROFILE_BOUNDS.values()
                    if variable in os.environ
                ],
                [],
                f"a briefing refused to run over {written!r}",
            )

    def test_every_source_runs_at_once_when_nothing_caps_them(self) -> None:
        self.declare("aaa", "pi", "morning")
        self.declare("bbb", "pi", "morning")
        self.harness(OVERLAPPING)
        briefing.collect()
        self.assertEqual(self.where.read_text().split(), ["in", "in", "out", "out"])


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


class Offers(unittest.TestCase):
    """What a source may offer, and how one run's offers get their numbers."""

    def envelope(self, *offers: dict) -> str:
        return json.dumps({**ENVELOPE, "offers": list(offers)})

    def test_an_offer_is_a_label_and_a_detail(self) -> None:
        found = briefing.parse_contribution(
            self.envelope({"label": "Call the dentist", "detail": "Left over."})
        )
        self.assertEqual(
            found["offers"],
            [{"label": "Call the dentist", "detail": "Left over."}],
        )

    def test_a_source_that_offers_nothing_carries_an_empty_list(self) -> None:
        # Nothing to do is the ordinary case, and it must cost the envelope
        # nothing: an absent key is a source with no next step, not a bad one.
        self.assertEqual(
            briefing.parse_contribution(json.dumps(ENVELOPE))["offers"], []
        )

    def test_more_offers_than_the_bound_is_refused_by_name(self) -> None:
        text = self.envelope(
            *({"label": f"Do {n}", "detail": "Because."} for n in range(4))
        )
        with self.assertRaises(briefing.Unusable) as caught:
            briefing.parse_contribution(text)
        self.assertIn("at most 3", str(caught.exception))

    def test_an_offer_without_its_label_or_detail_is_refused_by_name(self) -> None:
        for offer, named in (
            ({"detail": "Because."}, "an offer label"),
            ({"label": "Do it"}, "an offer detail"),
        ):
            with self.subTest(offer=offer):
                with self.assertRaises(briefing.Unusable) as caught:
                    briefing.parse_contribution(self.envelope(offer))
                self.assertIn(named, str(caught.exception))

    def test_an_offer_with_a_key_of_its_own_is_refused(self) -> None:
        # The shape is the whole contract. A source that sends a prompt for
        # someone to run is doing the thing this design took out, so it is
        # refused rather than quietly stripped.
        with self.assertRaises(briefing.Unusable):
            briefing.parse_contribution(
                self.envelope(
                    {"label": "Do it", "detail": "Because.", "prompt": "rm -rf /"}
                )
            )

    def test_offers_are_numbered_across_sources_in_source_order(self) -> None:
        numbered = briefing.numbered_offers(
            [
                {
                    "project": "the-den",
                    "slug": "the-den",
                    "offers": [
                        {"label": "One", "detail": "a"},
                        {"label": "Two", "detail": "b"},
                    ],
                },
                {
                    "project": "seedzero",
                    "slug": "seedzero",
                    "offers": [
                        {"label": "Three", "detail": "c"},
                        {"label": "Four", "detail": "d"},
                    ],
                },
            ]
        )
        self.assertEqual([item["number"] for item in numbered], [1, 2, 3, 4])
        self.assertEqual(
            [item["label"] for item in numbered], ["One", "Two", "Three", "Four"]
        )
        self.assertEqual(numbered[2]["project"], "seedzero")

    def test_a_source_that_offered_nothing_takes_no_numbers(self) -> None:
        numbered = briefing.numbered_offers(
            [
                {"project": "quiet", "slug": "quiet", "offers": []},
                {
                    "project": "loud",
                    "slug": "loud",
                    "offers": [{"label": "Only", "detail": "a"}],
                },
            ]
        )
        self.assertEqual(
            numbered,
            [
                {
                    "number": 1,
                    "project": "loud",
                    "slug": "loud",
                    "label": "Only",
                    "detail": "a",
                }
            ],
        )


class OffersPage(unittest.TestCase):
    """The block the numbered list is drawn as."""

    def block(self, *offers: dict) -> str:
        return page.offers_block(list(offers))

    def test_a_run_with_no_offers_draws_nothing(self) -> None:
        # Not an empty heading. Most mornings need nothing started, and a
        # standing "Next" with nothing under it reads as a failure.
        self.assertEqual(self.block(), "")

    def test_an_offer_is_drawn_with_its_number_and_its_source(self) -> None:
        drawn = self.block(
            {"number": 2, "project": "the-den", "label": "Call", "detail": "Left over."}
        )
        self.assertIn('<span class="number">2</span>', drawn)
        self.assertIn(">Call<", drawn)
        self.assertIn(">the-den<", drawn)
        self.assertIn("Left over.", drawn)

    def test_html_in_an_offer_stays_text_and_markdown_is_read(self) -> None:
        drawn = self.block(
            {
                "number": 1,
                "project": "p",
                "label": "Bump `checkout` to v5",
                "detail": "Node 20 is <b>deprecated</b> and *ends* soon.",
            }
        )
        self.assertIn("<code>checkout</code>", drawn)
        self.assertIn("<em>ends</em>", drawn)
        self.assertIn("&lt;b&gt;deprecated&lt;/b&gt;", drawn)
        self.assertNotIn("<b>", drawn)


class Bounds(unittest.TestCase):
    """The limits a profile may raise, in the prompt and in the answer."""

    def source(self) -> dict[str, object]:
        return {
            "project": "personal/nova-protocol",
            "root": "/home/x/nova",
            "description": "Review today.",
            "guidance": "Read the day's commits.",
            "harness": "claude",
            "model": "opus",
            "thinking": "high",
        }

    def test_the_offer_bound_is_the_same_number_in_both_places(self) -> None:
        # Stated in the prompt and checked against the answer. A source told
        # it may make eight and refused at four would be refused for obeying.
        with mock.patch.dict(os.environ, {"SCUFRIS_BRIEFING_MAX_OFFERS": "8"}):
            prompt = briefing.contribution_prompt(
                self.source(), "nightly", "2026-09-08"
            )
            self.assertIn("at most 8 things", prompt)
            offers = [{"label": f"Fix {n}", "detail": "why"} for n in range(8)]
            self.assertEqual(len(briefing.parse_offers(offers)), 8)

    def test_the_raised_offer_bound_still_ends(self) -> None:
        with mock.patch.dict(os.environ, {"SCUFRIS_BRIEFING_MAX_OFFERS": "8"}):
            offers = [{"label": f"Fix {n}", "detail": "why"} for n in range(9)]
            with self.assertRaises(briefing.Unusable) as refused:
                briefing.parse_offers(offers)
        self.assertIn("at most 8", str(refused.exception))

    def test_the_default_offer_bound_holds_when_nothing_is_set(self) -> None:
        with mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop("SCUFRIS_BRIEFING_MAX_OFFERS", None)
            with self.assertRaises(briefing.Unusable):
                briefing.parse_offers(
                    [{"label": f"Fix {n}", "detail": "why"} for n in range(4)]
                )

    def test_a_repair_is_shown_everything_it_is_asked_to_keep(self) -> None:
        # The repair is handed its own answer and told to keep every finding.
        # A fixed 32 KiB quote against a raised body bound cut the answer and
        # asked for all of it anyway, so the last third of a nightly report
        # went missing while the run recorded `ok`.
        with mock.patch.dict(os.environ, {"SCUFRIS_BRIEFING_MAX_BODY": "65536"}):
            answer = "x" * 60_000
            prompt = briefing.repair_prompt(
                self.source(), "nightly", "status is invalid", answer
            )
            self.assertIn(answer, prompt)
            self.assertNotIn("too long to quote back", prompt)
            # It still ends. An answer past the budget is cut, and the cut says
            # not to invent what is missing.
            cut = briefing.repair_prompt(
                self.source(), "nightly", "status", "y" * 200_000
            )
            self.assertIn("too long to quote back", cut)
            self.assertIn("do not invent the rest", cut)

    def test_a_source_is_told_it_gets_one_turn(self) -> None:
        # `harness_argv` builds `--print`, which ends the process when the
        # model's turn ends. A source once dispatched its lanes and ended the
        # turn saying it would wait for them; they died with the process and
        # 1676 seconds produced nothing. The prompt states every other property
        # of the harness a source must respect.
        prompt = briefing.contribution_prompt(self.source(), "nightly", "2026-09-08")
        self.assertIn("exactly one turn", prompt)
        self.assertIn("dies with it", prompt)
        self.assertIn("never end a turn intending to continue", prompt)

    def test_the_body_bound_is_the_same_number_in_both_places(self) -> None:
        with mock.patch.dict(os.environ, {"SCUFRIS_BRIEFING_MAX_BODY": "40000"}):
            prompt = briefing.contribution_prompt(
                self.source(), "nightly", "2026-09-08"
            )
            self.assertIn("at most 40000 characters", prompt)
            envelope = {
                "title": "Nova",
                "status": "ok",
                "headline": "The day is reviewed.",
                "facts": [],
                "body": "x" * 20_000,
            }
            parsed = briefing.parse_contribution(
                f"```json\n{json.dumps(envelope)}\n```"
            )
            self.assertEqual(len(parsed["body"]), 20_000)

    def test_a_body_over_the_raised_bound_is_still_refused(self) -> None:
        envelope = {
            "title": "Nova",
            "status": "ok",
            "headline": "The day is reviewed.",
            "facts": [],
            "body": "x" * 20_000,
        }
        with self.assertRaises(briefing.Unusable) as refused:
            briefing.parse_contribution(f"```json\n{json.dumps(envelope)}\n```")
        self.assertIn("longer than 16384", str(refused.exception))

    def test_an_unreadable_bound_reads_as_the_default(self) -> None:
        # A typo in a unit file must not stop the morning. It reads as if the
        # deployment had said nothing.
        for said in ("", "eight", "-3", "0"):
            with (
                self.subTest(said=said),
                mock.patch.dict(os.environ, {"SCUFRIS_BRIEFING_MAX_OFFERS": said}),
            ):
                self.assertEqual(briefing.max_offers(), briefing.MAX_OFFERS)


if __name__ == "__main__":
    unittest.main()
