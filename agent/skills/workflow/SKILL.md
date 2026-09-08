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

## Receipts

A worker's report is prose. A receipt is measured. Say only what the receipt
measured.

Every terminal event carries a receipt, and `scufris_job_inspect`,
`scufris_job_land`, and `scufris_job_stop` return one. It holds `facts`, a
`claims` cross-check against what the worker wrote, and `sentences`, the exact
words to repeat.

- Quote `sentences` verbatim. Do not soften them, and do not reword them into
  something that sounds better.
- `false` is a measurement. `null` with an entry in `unavailable` is not: that
  fact is unknown, and the reason says why. Never report an unknown as a no.
- When `landed` is false, the sentence is "not landed". Use those words.
- When a worker claims a push, a merge, or a release and no field backs it, the
  words are "claimed, not verified". Report the claim and the verdict together.
- Never repeat a worker's own claim about pushing, merging, tagging, or
  releasing as though it happened. Nothing in Scufris pushes or releases.

Landing is still explicit and never implied by `done`. A receipt that says the
work landed is not permission to land the next one.

Use `scufris_job_inspect` to recover bounded evidence after a wake or context
compaction. After reacting to a wake, synthesize one useful short response with
`scufris_final_response`; never end a wake turn with tools only.

Use each meaningful workflow action as the only tool in its batch. Start
everything the request asks for, one action to a batch, including an
instruction that arrives while you are already working: an answer that states
an intention instead of taking the action is a request dropped, because nothing
carries it into a later turn. After a spawn, steering, stop, landing, or
review-opening action, never call shell `sleep`, wait for a worker, poll
status, or inspect that job. Filesystem notifications start later turns. When
nothing the request asked for is left to start, call `scufris_final_response`
as its own tool batch with one short contextual acknowledgment in Scufris's
natural voice. Do not use deterministic canned speech.

If an action tool fails, do not claim success. Call `scufris_final_response`
with one concise explanation and the next safe step. A failed action does not
authorize waiting or polling. One batch holds one meaningful action, but a turn
may hold as many batches as the request has things to start. Use
`scufris_job_stop` only for an owned job, and pass the workflow root; a
descendant ID is refused. It ends that complete workflow graph, including
reviewer descendants, and archives their durable records instead of deleting
them, so each report and conversation stays readable. It removes Sprout
worktrees only when you pass `remove_workspace`. Call it only when no graph
result is still needed. Removal keeps a branch that was never merged: pass
`abandon` only when the user has said to throw the work away. If a stop is
refused because the branch is unmerged, say so and ask, rather than retrying
with `abandon`.

## Optional workflow tools

Project preferences can select optional phases. They never run automatically.

- For an independent review, resolve a fresh context for the same project and
  call `scufris_job_spawn` with `review_of` set to the implementation job.
  Reproduce the configured or explicitly requested review harness, model, and
  thinking. Pi and Claude reviewers run in the exact source workspace with an
  enforced built-in model-tool read allowlist. This is not an OS filesystem
  sandbox. Inspect the returned isolation and trusted-boundary diagnostics;
  Claude managed hook/plugin policy remains part of the trusted host boundary.
- Call `scufris_job_quick_review` after the independent reviewer approves when
  project guidance selects Quick Review. It starts a separate read-only Pi RPC
  agent with the standalone npm extension, so foreground Scufris stays
  responsive while the agent writes the walkthrough and answers page questions.
  Pass the model and thinking values from the `quick-review` preference entry;
  never reuse them for the independent reviewer.
- Call `scufris_job_plannotator_review` when project guidance selects a
  Plannotator since-base code review.
- Call `scufris_job_land` only after the selected workflow has supplied the
  required approval. Landing is never implied by `done`.
