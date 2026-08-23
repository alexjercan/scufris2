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
5. Call `scufris_job_spawn` with the single-use context ID as the only tool in
   that batch. Then call `scufris_final_response` as the only follow-up with one
   short natural acknowledgment, and end the foreground turn.

## General jobs

Use `scufris_job_spawn` without a context ID. Omit execution choices when the
user did not specify them. Scufris then uses Pi with
`openai-codex/gpt-5.6-sol` and medium thinking in a private temporary
workspace. After spawning, call `scufris_final_response` with one short natural
acknowledgment, then end. Project tracking, worktrees, review, and landing do
not apply unless the request explicitly introduces them.

## Events

- `working` means the worker is actively doing assigned work. It is the only
  event that keeps an execution running. Job status arrives through filesystem
  notifications; do not poll merely to repeat it.
- `blocked` and `done` both end that execution generation and release its tmux
  window. The logical job stays steerable. `blocked` means the worker needs
  mediation; `done` means the assignment is complete.
- After either one, inspect the project context, prompt, report, conversation,
  and current state, then decide whether to review, continue with guidance,
  open a human review, land, or stop.
- `scufris_job_send` continues a job. It restores the worker's own harness
  session in a new window and appends your guidance, so the worker keeps its
  full conversation. Spawn a new job only when you want a genuinely fresh
  agent.
- `failed` is generated only when trusted orchestration detects that the worker
  can no longer work. Workers cannot report it themselves.

Each worker report is chronological Markdown. Every entry starts with the exact
status line as a heading and contains evidence for that event. Inspect the full
report after a wake instead of treating it as only the latest worker snapshot.

Use `scufris_job_inspect` to recover bounded evidence after a wake or context
compaction. After reacting to a wake, synthesize one useful short response with
`scufris_final_response`; never end a wake turn with tools only.

Use each meaningful workflow action as the only tool in its batch. After a
successful spawn, steering, stop, landing, or review-opening action, the only
permitted follow-up is `scufris_final_response` as its own tool batch. Give one
short contextual acknowledgment in Scufris's natural voice, then end. Do not
use deterministic canned speech. After spawn or steering, never call shell
`sleep`, wait for a worker, poll status, inspect, or do any other work before
the final response. Filesystem notifications start later turns.

If an action tool fails, do not claim success. Call `scufris_final_response`
with one concise explanation and the next safe step. A failed action does not
authorize waiting or polling. Do not batch multiple meaningful actions; finish
one acknowledgment boundary before another user-directed action. Use
`scufris_job_stop` only for an owned job, and pass the workflow root; a
descendant ID is refused. It ends that complete workflow graph, including
reviewer descendants, and archives their durable records instead of deleting
them, so each report and conversation stays readable. It removes Sprout
worktrees only when you pass `remove_workspace`. Call it only when no graph
result is still needed.

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
