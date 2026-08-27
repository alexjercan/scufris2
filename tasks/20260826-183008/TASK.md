# Scufris does what is asked, not a workflow

- STATUS: CLOSED
- PRIORITY: 75
- TAGS: identity, workflow

## Goal

Scufris stops running a fixed pipeline and starts delegating literally.
"Implement feature X in project Y" means work, and only work. A review
happens when the user asks for a review. Nothing else is added because
the project could do it.

## The problem

Today Scufris reads `.scufris.toml` and treats it as a plan to carry
out: it finds the project can review, so it reviews; it finds a landing
gate, so it gates. The file describes what a project supports, and
Scufris reads it as what a request means. The result is that one verb
from the user turns into a pipeline the user did not ask for.

Alex, 2026-08-26: "the `.scufris.toml` just explains how things can be
used/done but scufris should do as I say".

## Scope

- `.scufris.toml` declares agent types rather than a workflow. Each one
  names what it is for and how it is run: work, review, quick-review,
  and whatever a project adds. The file is a menu, not a program.
- Scufris delegates to the agents the request names, and to no others.
  One verb is one job. "Implement X" is the work agent. "Implement X and
  review it" is the work agent and then the review agent.
- Conventions are inferred, overrides are obeyed. Whether to open a
  Tatr task, whether to work in a Sprout worktree or on `master`, and
  which harness to use are inferred from the project's preferences
  unless the user says otherwise in the request. "Do it directly on
  master" wins over the file.
- One job, archived, reported back. Scufris finishes what was asked,
  records it, and says what it did. It does not queue follow-on work of
  its own.
- The identity and the delegation prompt change with the file. Both
  currently describe the pipeline.

## Verification

- "Implement X" runs work and nothing else, and the transcript shows no
  review agent started.
- "Implement X, then review it" runs both, in that order.
- "Review the last change" runs review alone against existing work.
- A request naming `master` works on `master` in a project whose
  preferences ask for a Sprout worktree.
- A project whose `.scufris.toml` declares an agent Scufris has never
  seen can still be delegated to by name.

## Notes

Queued by Alex on 2026-08-26 while the widget runtime was being built.
Not to be started until the widget runtime increments and the i3 binding
mode task are done.

## Decisions, 2026-08-27

- The file keeps two top-level tables and nothing else. `conventions` is
  what Scufris infers when the request is silent. `agents.<name>` is one
  agent type. Every other top-level table makes the file unusable.
- Each agent entry needs a `description`: one short printable line that
  says what the agent is for. `keywords` say how it is run and stay flat
  scalars. `guidance` carries prose judgement. The description is what
  lets an unfamiliar agent be delegated to by name.
- The menu declares no order and no gate. `order`, `gate`, and the
  implied review-then-land chain are gone. Order comes from the request.
- The retired `preferences` shape is refused, not half-read. `context`
  returns the diagnostic below and the job degrades to inference. This
  is deliberate: a half-read workflow file is what produced the
  unrequested pipeline.

  ```text
  ignored .scufris.toml: the preferences workflow shape was retired;
  declare conventions and agents tables
  ```

- The real `.scufris.toml` is untouched here. Alex converts it. Until
  then this project renders that diagnostic and Scufris infers.

### A later round steers the job it already owns

Alex, 2026-08-27: a fresh review agent every round never converges. The
reviewer "wants to be useful so it will always find something wrong",
and a new job has no record of what it already accepted, so it
re-derives fault from scratch and the cycle runs forever.

- The `agents.review` guidance now says to keep one review job for the
  work: spawn the first round, then steer that same job with
  `scufris_job_send` for each later round. A second review job is for
  work no owned review job covers.
- This needs no new mechanism. `scufris_job_send` appends the guidance
  to `conversation.md` and restarts the job, and the helper restores the
  worker's own harness session rather than replaying `prompt.md`, so the
  reviewer resumes with everything it already accepted.
- The rule is stated once per audience. `literalDelegationPolicy` and
  the `scufris_job_spawn` guidelines state it for every agent, not only
  the reviewer; the menu entry states it for this project's reviewer.

## Required `.scufris.toml` shape

`tests/fixtures/scufris-menu.toml` holds this exact text and a test
renders it, so the snippet and the parser stay in step. Paste it over
`.scufris.toml`:

```toml
# Scufris reads this file as a menu, not a workflow. Conventions are inferred
# when the request is silent. Agents run only when the request names them.

[conventions]
keywords = { tracking = "tatr", scope = "one-task-per-request", workspace = "sprout", base = "master", typescript = "npm run check", packaging = "nix flake check", landing = "explicit" }
guidance = """
Use Tatr for substantial tracked work. Keep one task for one request and its
follow-up work. Record decisions and verification evidence under the task.
Keep the main checkout on master and do project work in a Sprout worktree.
Run the cheapest relevant focused check first. Land a Sprout only when the
request asks for it.
"""

[agents.work]
description = "Implement a change in the project."
keywords = { harness = "pi", model = "openai-codex/gpt-5.6-sol", thinking = "medium" }
guidance = """
Keep orchestration narrow. Prefer product skills and small deterministic
helpers over extension complexity. Follow repository AGENTS.md instructions and
existing conventions.
"""

[agents.review]
description = "Read finished work independently and report findings."
keywords = { harness = "pi", model = "openai-codex/gpt-5.6-sol", thinking = "medium", spawn = "review_of" }
guidance = """
Read-only against the implementation job's exact workspace. Report concrete
findings with paths and lines. Keep one review job for the work: spawn it for
the first round, then steer that same job with scufris_job_send for each later
round, because a fresh reviewer has no record of what it already accepted.
Spawn a second review job only when no owned review job covers this work.
Repeat the implement-then-review cycle only for as long as the request asked
for.
"""

[agents.quick-review]
description = "Open the Quick Review walkthrough page for the user to approve."
keywords = { tool = "scufris_job_quick_review", harness = "pi", mode = "rpc", model = "openai-codex/gpt-5.6-sol", thinking = "medium", extension = "npm:@alexjercan/quick-review@0.1.1" }
guidance = """
Start the standalone Quick Review extension in its separate Pi RPC agent. Keep
foreground Scufris responsive while that agent writes the walkthrough and
answers page questions. Do not use these Pi RPC settings for the review agent.
"""
```

Add an agent by adding a table. `[agents.plannotator-review]` fits the
`scufris_job_plannotator_review` tool the same way. A name Scufris has
never seen renders like any other entry and is delegated to with that
entry's keywords.

## What changed

- `tools/jobs/scufris-jobs`: `render_menu` replaces the `preferences`
  reader. It renders a Conventions section and an Agents menu, validates
  each entry's adapter tuple, and closes with the literal-delegation
  rule. The worker prompt section is now `## Project context`.
- `extensions/scufris/workflow/identity.ts`: the identity says
  "delegate literally: one verb is one job", "the project file is a
  menu, not a workflow", "start no agent the request did not name", and
  "queue no follow-on work". The vague "Native workflow orchestration
  remains available" clause is gone.
- `extensions/scufris/workflow/orchestration.ts`: the delegation prompt
  is the exported `literalDelegationPolicy`. It names the two worked
  examples from the Scope section, keeps unfamiliar agents delegable,
  and states that the request outranks a convention. Worker, Quick
  Review, and Plannotator wake messages now ask for a report to the
  user instead of "decide what follows from the project preferences".
- Docs: `docs/src/dev/jobs.md`, `docs/src/guide/using.md`,
  `docs/src/overview.md`, and `docs/src/dev/extensions.md`.

## Verification, 2026-08-27

- `TMPDIR=/tmp npm run check`: typecheck, 73 tests, Prettier all pass.
- `TMPDIR=/tmp python3 -m unittest tests.test_scufris_jobs`: 30 tests
  pass.
- Verification bullets 2, 3, and 4 are prompt behavior, asserted as
  exact prompt text in `tests/agents.test.ts` and `tests/identity.test.ts`
  rather than by running a live agent.
- Bullet 5 is asserted in `tests/test_scufris_jobs.py`: an
  `[agents.fuzz]` entry renders with its description and keywords and
  returns no diagnostic.
- The repository's own `.scufris.toml` was rendered with the new parser
  and returns the retired-shape diagnostic, as expected until Alex
  pastes the snippet above.

## Closed (2026-08-27)

The last open item was the one this task deliberately left to Alex: the
repository's own `.scufris.toml` still had the retired `preferences`
shape, so the project rendered the diagnostic and Scufris inferred.

Converted in `3333ea4`, and `35326a7` pointed `agents.review` at
`/scufris-review`, this project's own panel, rather than at a bare
read-only reviewer. `context` now renders the menu with no diagnostic.

Nothing else was outstanding. The parser, the identity, the delegation
policy, and the documentation landed with the verification recorded
above.
