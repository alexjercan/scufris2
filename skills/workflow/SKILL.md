---
name: scufris-workflow
description: Resolve project workflow preferences and run independent project or general jobs. Use for work expected to take minutes.
---

# Scufris workflow

Use Scufris native job tools. Do not invoke tmux, Pi, Claude, or project
workspace commands directly from the foreground session.

## Project jobs

1. Call `scufris_projects` when the opaque project ID is not already known.
2. Call `scufris_project_context` for every new project job. Never reuse a
   context ID, including for another job in the same project.
3. Read the complete returned project context. Follow it unless the user's
   explicit request overrides it or it is impossible.
4. Compose one self-contained worker prompt. Select the preferred harness,
   model, thinking, and workspace from the request and project guidance.
5. Call `scufris_job_spawn` with the single-use context ID.

## General jobs

Use `scufris_job_spawn` without a context ID. Omit execution choices when the
user did not specify them. Scufris then uses Pi with
`openai-codex/gpt-5.6-sol` and medium thinking in a private temporary
workspace. Project tracking, worktrees, review, and landing do not apply unless
the request explicitly introduces them.

## Events

- `working` is quiet progress. Job status arrives through filesystem
  notifications; do not poll merely to repeat it.
- `needs-decision` asks for one user decision. Inspect the report first.
- `blocked` reports an unblock condition. Inspect the report first.
- `ready` describes a completed milestone. It does not request or authorize a
  next action. Inspect the project context, prompt, report, and current state,
  then choose the next tool from the request and preferences.
- `done` is terminal success.
- `failed` is terminal failure.

Use `scufris_job_inspect` to recover bounded evidence after a wake or context
compaction. Use `scufris_job_send` only for literal steering. Use
`scufris_job_stop` only for an owned job. Remove a Sprout workspace only when
its result is no longer needed.

## Optional workflow tools

Project preferences can select optional phases. They never run automatically.

- For an independent review, resolve a fresh context for the same project and
  call `scufris_job_spawn` with `review_of` set to the implementation job. The
  reviewer runs read-only in the exact source workspace.
- Call `scufris_job_quick_review` when project guidance selects the custom
  section-based Quick Review page. It generates an exact-revision walkthrough
  with explanations, non-blocking section comments, an overall review comment,
  approval, and explained change requests.
- Call `scufris_job_plannotator_review` when project guidance selects a
  Plannotator since-base code review. Plannotator and Quick Review are separate
  tools and neither substitutes for the other.
- Call `scufris_job_land` only after the selected workflow has supplied the
  required approval. Landing is never implied by `done` or `ready`.
