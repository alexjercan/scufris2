# Project workflows and delegated jobs

Scufris handles work expected to take seconds in the foreground and delegates
work expected to take minutes. Delegation supports project work and general
work such as research or external report creation.

## Project preferences

A project can add `.scufris.toml` at its Git root. The file contains advisory
workflow preferences:

```toml
version = 1

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

Workers append events to their private `status` file:

- `working: <summary>` records quiet progress.
- `needs-decision: <summary>` requests user mediation.
- `blocked: <summary>` reports an unblock condition.
- `ready: <milestone-slug>` reports a completed nonterminal milestone.
- `done: <summary>` reports terminal success.
- `failed: <summary>` reports terminal failure.

A `ready` slug describes what completed, such as
`implementation-complete` or `assets-collected`. It is not a command. The
extension wakes foreground Scufris, which inspects the pinned context, worker
prompt, report, and current state before choosing another tool.

Every worker runs in an owned tmux session. Shutdown targets only resources
recorded for jobs owned by the foreground session.

## Optional tools

Project preferences can guide Scufris to compose additional phases. None are
part of the default job lifecycle.

- A Sprout workspace is selected explicitly when spawning implementation work.
- An independent review is another job with a fresh project context. It runs
  read-only in the source job's exact workspace.
- Quick Review is an explicit human since-base review for a Sprout job. Its
  feedback returns to foreground Scufris and never lands automatically.
- Local landing is an explicit guarded tool call after the selected workflow
  supplies approval.
