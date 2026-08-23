# Using Scufris

## Conversation

Scufris is a pair-programming companion. It keeps the conversation in the
foreground, synthesizes evidence in its own voice, and stops at meaningful
decisions. It answers conversation and narrow project questions directly and
delegates work expected to take minutes.

Every final answer is one short plain-prose paragraph. Optional Markdown
detail is stored privately beside the session, and the transcript shows one
compact command:

```text
/detail 4f8c7a21d3e64b829e93ab10
```

Run that command to open the artifact in Plannotator. Approval and closure
produce one compact transcript row. Actionable feedback returns to Scufris
privately.

## Delegated jobs

Ask for project work and Scufris spawns an independent worker job. For a
configured project it first loads the project's workflow preferences from
`.scufris.toml` and follows them: task tracking, an isolated Sprout worktree,
the implementation harness and model, review, and the landing gate.

Workers report progress events. `working` is quiet by default. `blocked`,
`done`, and runtime-generated `failed` events wake Scufris, which inspects the
job report and decides what follows. Landing never happens implicitly; the
configured review must approve first, and Scufris then lands with an explicit
guarded operation.

Each worker runs in a named tmux session on the default server. Attach to it
read-only to watch, but do not type into worker panes.

Inspect stored jobs from a shell at any time:

```bash
scripts/scufris-jobs all
scripts/scufris-jobs <id-prefix>
scripts/scufris-jobs all --archived --json
```

## Quick Review

When project preferences select Quick Review, Scufris generates an
exact-revision walkthrough of the implementation and opens a local review page
in the browser. The page shows one section per change with its diff and a
review prompt. For each section you can:

- Mark it viewed, or reopen it.
- Ask the reviewer model a question about the exact code.
- Load exact-revision file context.
- Add a non-blocking comment.

Terminal actions end the review: approve the exact revision, optionally with
comments, or request changes with an explanation. A change request steers the
worker and invalidates the walkthrough. The page refuses to act when the
reviewed revision changed underneath it.

## Dashboard widgets

Ask Scufris to show live information and it opens a native Dashboardd widget.
Widgets and variants are discovered when the session starts. Scufris updates,
focuses, and closes only surfaces it opened. When a widget is closed outside
Scufris, it forgets the surface and does not reopen it unasked.

## Commands

- `/speech on|off|once|replay`: control spoken responses in a voice-capable
  package. `once` arms speech for one response. `replay` repeats the last safe
  paragraph.
- `/calm` inspects Calm mode; `/calm on|off` sets it. Calm hides thinking,
  tool execution rows, and job event noise. It is on by default.
- `/wake` inspects the worker wake mode; `/wake minimal|all` sets it.
  `minimal` keeps `working` updates quiet. `all` also wakes on each `working`
  event. Mandatory continuation events cannot be disabled.
- `/detail <id>` opens a private detail artifact in Plannotator.
- `/scufris-prompt` creates a private artifact with the exact assembled system
  prompt and its ordered provenance, without contacting a provider.

Explicit `/speech`, `/calm`, and `/wake` values are restored with the session.

## Voice

The ordinary voice-capable launcher stays silent until speech is enabled. The
popup launcher starts with speech and Calm on and resumes its dedicated
session. Enabled speech plays each safe settled response once, including
automatic wake turns. Speech input is Pi configuration, not Scufris.
