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
5. Call `scufris_job_spawn` with the single-use context ID, then end the
   foreground turn immediately.

## General jobs

Use `scufris_job_spawn` without a context ID. Omit execution choices when the
user did not specify them. Scufris then uses Pi with
`openai-codex/gpt-5.6-sol` and medium thinking in a private temporary
workspace. End the foreground turn immediately after spawning. Project
tracking, worktrees, review, and landing do not apply unless the request
explicitly introduces them.

## Events

- `working` means the worker is actively doing assigned work. Job status
  arrives through filesystem notifications; do not poll merely to repeat it.
- `blocked` means the worker cannot continue without mediation. Inspect the
  report, then resolve it directly or ask the user when a user decision is
  necessary.
- `done` means the current assignment is complete. It does not stop the worker
  or authorize a next action. Inspect the project context, prompt, report, and
  current state, then decide whether to review, steer more work, open a human
  review, land, or stop.
- `failed` is generated only when trusted orchestration detects that the worker
  can no longer work. Workers cannot report it themselves.

Use `scufris_job_inspect` to recover bounded evidence after a wake or context
compaction. Use `scufris_job_send` only for literal steering, then end the
foreground turn immediately. Never call shell `sleep`, wait for a worker, poll
status, or repeatedly inspect a job for progress. Filesystem notifications
start later turns. Use `scufris_job_stop` only for an owned job. Remove a Sprout
workspace only when its result is no longer needed.

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
  required approval. Landing is never implied by `done`.
