# Add project workflow preferences and generic delegation planning

- STATUS: IN_PROGRESS
- PRIORITY: 100
- TAGS: orchestration, configuration, preferences, delegation

## Context

Scufris currently embeds one coding workflow around Sprout, delegated review,
Quick Review, and local landing. It must also support project-specific coding
preferences and unrelated work such as research or writing an external report.
Configuration must guide contextual workflow inference rather than define or
enforce one procedure.

## Rewrite policy

Treat the existing Scufris implementation only as source material for a
replacement. Preserve no legacy workflow, job schema, status keyword, tool
contract, state record, or compatibility path unless it directly fits the new
design.

- Remove old job records and replace the old implementation in place.
- Implement only the new contracts. Add no compatibility code, aliases,
  migration paths, migration tests, or legacy fixtures.
- Delete obsolete orchestration, helpers, tests, and documentation instead of
  adapting the new design around them.
- Reuse implementation only when it remains the simplest correct component of
  the replacement.
- Breaking the old Scufris is explicitly accepted. Verify only the replacement
  behavior.

## Accepted outcome

- Load optional project preferences from `<project>/.scufris.toml`.
- Do not add user-level workflow configuration.
- Continue through contextual inference when the project file is absent,
  malformed, or the request has no project context. Ignore an unusable project
  configuration and use the built-in execution fallback without blocking the
  job.
- Treat all configured values as advisory. Explicit user instructions take
  precedence.
- Parse, validate, and render usable preferences as a canonical prompt section
  with source provenance. Report parse diagnostics, but do not block work.
- Resolve one opaque project context for each new project job before composing
  its delegated workflow. Pin the selected context and effective preferences
  into that job at spawn.
- Support non-project delegated work without requiring Git, Sprout, review, or
  landing.
- Separate execution container, work context, workspace, and completion policy
  so a tmux worker does not imply a coding workflow.
- Keep reusable workflow capabilities as independent tools under `tools/`.
  Foreground Scufris selects them from the explicit request, project preference
  prompt, and available tool descriptions.
- Document the format, precedence, discovery rules, diagnostics, and failure
  behavior.

## Format decision

Use TOML with open-ended preference keys and one common record shape:

```toml
version = 1

[preferences.tracking]
name = "tatr"
options = {}
guidance = """
Use Tatr for substantial tracked work.
"""

[preferences.implementation]
name = "claude"
options = { model = "opus-5", thinking = "xhigh" }

[preferences.review]
name = "pi"
options = { model = "sol", thinking = "medium" }
guidance = """
Run review after implementation. Return findings to implementation.
"""

[preferences.quick-review]
name = "quick-review"
guidance = """
Use Quick Review after independent review passes and before landing.
"""
```

Each entry has optional `name`, `options`, and `guidance` fields. Preference
keys are open-ended. All values become canonical prompt guidance; the parser
does not assign workflow semantics or pass options verbatim to commands.
Foreground Scufris must follow this project guidance unless the explicit user
request overrides it or following it is impossible.

## Precedence and built-in baseline

1. Explicit instructions in the current request.
2. Selected project `.scufris.toml`.
3. Repository instructions and available project capabilities contribute
   context.
4. Scufris infers a minimal workflow when no preference resolves a choice.

The built-in no-project execution fallback is Pi with
`openai-codex/gpt-5.6-sol` and medium thinking. Built-in defaults select only
execution mechanics. They must not imply tracking, Sprout, implementation,
review, Quick Review, or landing.

Project context is semantic, not derived only from an output path. The
foreground Scufris session is rootless: every new project job resolves its own
opaque project and receives a fresh preference snapshot. General work has no
project preferences. An active job keeps its pinned snapshot even when the
configuration changes or an artifact is written elsewhere.

## Project context loading

Use one native `scufris_project_context` resolver instead of dynamic Pi skills.
Its permanent tool description tells Scufris to call it before planning a new
project job. The resolver reads and validates the selected `.scufris.toml`,
renders bounded canonical Markdown, and returns it with a session-owned,
single-use context ID and configuration fingerprint. A successful spawn
consumes the context ID. A failed spawn leaves it available for retry.

The extension keeps a bounded registry of resolved contexts and active jobs. A
context can create exactly one job, so every new job receives a fresh project
configuration read and its own immutable context. A compact active-job index
can be added to each foreground turn. Full preferences remain available
through context or job inspection rather than being repeated in every prompt.

At spawn, persist the exact rendered snapshot as `project-context.md` in the
job directory. Keep `job.json` as runtime identity and execution metadata. The
existing `prompt.md` is the resolved task and workflow given to the worker; do
not add a separate workflow-plan artifact. The context Markdown is an immutable
prompt artifact, not a registered Pi skill. New jobs reread project
configuration; existing jobs never change implicitly.

## Delegation dimensions

- Execution container: foreground or owned tmux worker.
- Work context: current project, selected project, or general.
- Workspace: Sprout, project directory, external directory, or temporary
  workspace.
- Completion: conversational result, report, artifact, repository revision,
  pull request, local landing, or another inferred outcome.

## Optional workflow tools

Put deterministic reusable capabilities under `tools/<tool>/`. Initial useful
capabilities include Sprout workspace management, independent preflight review,
Quick Review, and guarded local landing. Keep Pi lifecycle ownership and narrow
native tool registration in `extensions/scufris/`; keep each tool's process and
filesystem mechanics in its owning `tools/` directory.

No tool is an implicit phase of every job. `.scufris.toml` names and describes
preferred capabilities as prompt guidance. Foreground Scufris chooses explicit
tool calls after considering that guidance and the current request. Tool
contracts remain narrow and deterministic; unknown preference values never
become commands.

## Generic worker event protocol

Keep the append-only `status` file, but use workflow-neutral event classes:

- `working: <summary>` records quiet progress without waking Scufris.
- `needs-decision: <summary>` wakes Scufris for user mediation.
- `blocked: <summary>` wakes Scufris because the worker cannot continue.
- `ready: <handoff>` wakes Scufris at a nonterminal workflow boundary.
- `done: <summary>` reports terminal success.
- `failed: <summary>` reports terminal failure.

`ready` has an open-ended lowercase slug that describes the milestone just
completed, such as
`implementation-complete`, `draft-complete`, `assets-collected`, or
`deployment-candidate`. Detailed evidence remains in `report.md`.

The slug is a non-authoritative hint, not an executable command or requested
next action. The extension wakes foreground Scufris without routing from the
slug alone. Scufris inspects the pinned project context, worker prompt, report,
and current state, then decides the next action from the request and project
preferences.

## Open design work

- Define the narrow input and output schema for `scufris_project_context` and
  successful context consumption at spawn.
- Define non-Git workspace behavior without adding another workflow-plan
  schema. General workers use a job-owned temporary workspace unless the
  explicit request supplies an external result destination.
- Define narrow independent contracts for the initial optional tools without
  rebuilding a fixed lifecycle between them.
- Define supported TOML value types and duplicate handling. Add no arbitrary
  product-level size policy; use only technical bounds required for safe local
  parsing and provider requests.
- Keep workflow resolution separate from lifecycle notifications and wake-turn
  ordering, which are tracked in `tasks/20260823-121052/TASK.md`.

## Verification plan

- Focused parser, validation, discovery, registry, pinning, and prompt-rendering
  tests.
- Prompt inspection shows exact effective project preferences and provenance.
- Integration coverage for project-only, absent, ignored malformed,
  non-project, and multiple-project cases.
- Exercise a general delegated job outside Git without Sprout or review.
- Exercise generic `ready` handoffs, unknown handoffs, quiet progress, and
  terminal result events.
- Run `npm run check` after the extension integration is complete.
