# Clean up complete Scufris workflow resource graphs

- STATUS: OPEN
- PRIORITY: 100
- TAGS: scufris, workflow, lifecycle

## Problem

Scufris cleans up only the job or tmux session that reports last. A workflow can
leave implementation workers, reviewers, workspaces, and durable job records
behind. The user must then find and remove them manually.

## Goal

Model each implementation and review job as part of one owned workflow resource
graph. Apply one lifecycle policy to every worker and reviewer:

- Stop the exact owned execution session when a job reaches `done` or `failed`.
- Keep only the durable state required for inspection, review, steering, and
  landing.
- Recreate execution for the same logical job when later steering is required,
  using pinned prompt, report history, foreground conversation context, and a
  new execution generation.
- On workflow land or explicit stop, recursively remove all related execution
  sessions, review processes, temporary and Sprout workspaces, and durable job
  directories.
- Leave no records for the completed workflow in the jobs store.
- Never use broad process matching or kill a tmux server. Mutate only resources
  proven to belong to the workflow.

## Required design properties

- Explicit root and parent ownership for implementation and reviewer jobs.
- Uniform terminal behavior with no harness or review exceptions.
- Exact tmux socket, session, window, pane, job, and ownership validation.
- Atomic ownership validation and termination, with no check-then-kill race.
- Crash-consistent session creation, restart, terminal publication, and cleanup.
- Generation-aware event cursors that do not lose, replay, or skip terminal and
  blocked events across foreground restarts.
- Retry-safe recursive cleanup. Keep the root record until every descendant is
  removed successfully.
- Durable canonical project and workspace identity. Refuse cleanup or landing
  after configuration drift unless the recorded resource still matches.
- Safe stale-resource reconciliation, partial-failure reporting, idempotence,
  symlink refusal, and cross-workflow isolation.
- Tests must use dedicated explicit tmux sockets and prove unrelated sessions
  and workflows survive.

## Workflow

Implement directly on `master` as requested. Inspect the prior task and review
history for context, but design and implement this task from scratch. Record
new decisions and verification evidence here.

