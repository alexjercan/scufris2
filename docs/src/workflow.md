# Project workflows and delegated jobs

The workflow extension is Scufris's core engine. It owns foreground identity,
project methodology, delegated-agent state, event delivery, review, and landing
in one lifecycle. Agent spawn, inspect, send, and stop operations remain narrow parts
of that engine rather than a separate loaded extension.

Scufris handles work expected to take seconds in the foreground and delegates
work expected to take minutes. Delegation supports project work and general
work such as research or external report creation.

## Project preferences

A project can add `.scufris.toml` at its Git root. The file contains advisory
workflow preferences:

```toml
[preferences.tracking]
name = "tatr"
guidance = """
Use Tatr for substantial tracked work.
"""

[preferences.workspace]
name = "sprout"

[preferences.implementation]
name = "claude"
options = { model = "opus-5", thinking = "xhigh" }

[preferences.review]
name = "pi"
options = { model = "openai-codex/gpt-5.6-sol", thinking = "medium" }
guidance = """
Review implementation before delivery. Return findings to implementation.
"""
```

Preference keys are open-ended. Each preference accepts optional `name`,
`options`, and `guidance` values. Scufris renders the complete file as prompt
guidance. It follows that guidance unless the explicit request overrides it or
following it is impossible. Missing or malformed files do not block work.

Scufris loads a fresh project context for every new job. The resulting context
ID creates one job and is then consumed. The exact rendered snapshot is stored
as `project-context.md` beside that job. Jobs for different projects never
share preferences.

## General jobs

A general job has no project context. It runs in a private temporary workspace
and does not imply task tracking, Git, a worktree, review, or landing. When the
request does not select execution settings, Scufris uses Pi with
`openai-codex/gpt-5.6-sol` and medium thinking. An explicit external result
path, such as `~/Downloads`, remains part of the worker request.

## Worker events

Workers call the dedicated `scufris_report` tool to append one detailed
Markdown entry and then its validated event. The entry heading is the exact
event line, for example `# working: checking tests`, followed by that event's
evidence. Inspection returns the entries in chronological order. Pi reviewers
retain only read-only code tools plus this reporting tool. Claude workers use
the matching `scufris-report` adapter described in their private prompt.

Each UTF-8 evidence body is at most 512 KiB, each complete entry is at most 516
KiB, and `report.md` is at most 2 MiB. If a new entry would exceed the file
bound, Scufris discards the older history and keeps the new complete entry. A
private lock serializes report operations. Scufris writes and flushes a private
temporary report, atomically replaces `report.md`, flushes the job directory,
and only then appends and flushes the matching status notification. A visible
event therefore always has durable linked evidence, including across rollover
or an interrupted write.

Each worker receives a random report capability that authorizes only its own
job. Foreground orchestration retains a separate random capability for trusted
`failed` publication. A one-use launch capability protects the harness path
that can generate failures, and it is invalidated before the worker starts.
Only capability hashes are stored. Inspection and Quick Review open bounded
report artifacts with no symlink following and require regular files. Report,
lock, status, and authorization files remain private.

The workflow extension watches each owned status file through filesystem
notifications. It reads events only after a change notification; it does not
periodically poll jobs. Scufris calls each meaningful workflow action alone.
After a successful spawn, steering, stop, landing, or review-opening action, it
uses the final-response tool alone to give one short contextual acknowledgment,
then ends the foreground turn. After spawn or steering, that response is the
only permitted follow-up: Scufris never sleeps, waits, polls, inspects, or does
other work before it. The next applicable filesystem event starts a later turn.

Use `/wake` to inspect the session's worker wake mode. `/wake minimal` keeps
`working` updates quiet while `blocked`, `done`, and runtime-generated `failed`
events start foreground turns. `/wake all` also starts a turn for each
`working` event. Mandatory continuation events cannot be disabled. The default
is `minimal`, and an explicit mode is restored with the session.

Workers can report only:

- `working: <summary>` while actively doing assigned work.
- `blocked: <summary>` when work cannot continue without mediation.
- `done: <summary>` when the current assignment is complete.

A `done` event is nonterminal for the worker channel. The worker remains
available for more instructions. The extension wakes foreground Scufris, which
inspects the pinned context, worker prompt, report, and current state before it
decides whether to review, steer more work, open a human review, land, or stop.
Every wake-triggered turn ends with one useful synthesized final response, not a
tool-only turn. A later instruction returns the same worker to `working`.

Workers cannot report `failed`. Trusted orchestration appends a linked
`failed: <summary>` report entry and event only when the harness exits
unexpectedly or the reporting protocol breaks. Runtime failure, explicit stop,
and landing are the only terminal ownership states.

Every worker runs in an owned tmux session. Shutdown targets only resources
recorded for jobs owned by the foreground session.

Run `scripts/scufris-jobs all` to list all stored jobs across foreground
sessions. `--all` is the flag alias, and the original no-argument form remains
supported. The plain-text list uses fixed columns for job ID, latest state,
worker-pane liveness, project, workspace, harness/model, and latest summary. It
uses the same stable layout in a terminal and in a pipe. Column clipping and
padding use Unicode display-cell widths without an external runtime library.
Control and format characters are visibly escaped in all human output so job
content cannot emit terminal control sequences. JSON retains the original
string values.

Run `scripts/scufris-jobs <id>` for one job's events, report, pinned project
context, prompt, worker configuration, and current pane state. A unique
lowercase hexadecimal ID prefix is accepted. Missing and ambiguous prefixes
fail without selecting a job. Add `--json` to either form for structured
output. An empty list is `{"jobs": []}` in JSON and `No Scufris jobs.` in
plain text.

Inspection validates the exact durable record fields, types, identifier
domains, timestamps, workspace relationships, and tmux identities. Invalid
records appear as `invalid` rows in a complete list and fail direct lookup.
Job records and inspected status, report, context, and prompt artifacts are
opened by descriptor with no symlink following and must be regular files. Reads
are bounded: job records are rejected above 64 KiB, status inspection retains a
bounded recent tail, report output is capped at 2 MiB, and context and prompt
output are each capped at 512 KiB. Missing optional project context remains
valid; missing or unsafe required artifacts fail inspection.

## Calm mode

Use `/calm` to inspect the current Calm presentation state. Use `/calm on` or
`/calm off` to set it explicitly. Repeating either command does not invert the
state. Calm remains on by default and restores an explicit value with the
session.

## Optional tools

Project preferences can guide Scufris to compose additional phases. None are
part of the default job lifecycle.

- A Sprout workspace is selected explicitly when spawning implementation work.
- An independent review is another job with a fresh project context. It runs
  read-only in the source job's exact workspace.
- Quick Review is the custom local section-based walkthrough page. It supports
  exact-revision explanations, non-blocking section comments, an overall review
  comment, approval, and explained change requests.
  `tools/quick-review/preview.py` serves the page against a deterministic
  local fixture bridge for visual checks.
- Plannotator review is a separate explicit since-base code-review tool. It
  does not replace Quick Review.
- Local landing is an explicit guarded tool call after the selected workflow
  supplies approval.
