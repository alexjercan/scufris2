# Clean up complete Scufris workflow resource graphs

- STATUS: CLOSED
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

## Decisions

- A version 2 durable job is the logical resource. It stores a random workflow
  identity, exact root and parent jobs, owner session, canonical project and
  workspace paths with device/inode identity, generation, next event byte,
  status identity, cleanup intent, and an optional replaceable execution.
- `done` and `failed` end every implementation and reviewer execution without
  deleting logical state. Steering increments the generation, rotates launch
  and report authority, validates the workspace, and starts from the pinned
  prompt, report, and foreground `conversation.md`. `blocked` keeps its exact
  execution alive.
- Status is append-only generation-tagged JSONL. Event reads start at the
  durable unacknowledged byte and never advance it. The extension persists a
  wake message with the event ID before exact ordered acknowledgement. Recovery
  recognizes event IDs in Pi session entries. Large backlogs continue in 1 MiB
  batches. Status replacement fails its durable device/inode check.
- Tmux requires an absolute canonical `SCUFRIS_TMUX_SOCKET`; launchers supply a
  private physical default. Every command uses `tmux -S`. Creation stores a
  random execution intent before server mutation and writes matching job,
  token, generation, and phase options in one tmux command queue. Recovery can
  complete either side of the creation and restart crash windows.
- Exact termination is one tmux `if-shell -F` server command. Its condition
  validates the socket-selected session name, session/window/pane IDs, job,
  random execution token, and generation. Only its true branch runs exact
  `kill-session`. Steering uses the same atomic condition. The wrapper rejects
  `kill-server`; there is no ambient selection or broad process match.
- Stop or land records an intent at the root, stops every descendant, validates
  current project configuration and Sprout identity, removes workspaces, then
  removes descendant records before the root. Missing resources are success.
  Any execution, workspace, or descendant deletion error retains the root and
  remaining graph for retry. A cleanup-in-progress graph cannot restart or add
  reviewers.
- Independent reviewers inherit the source workflow and exact workspace. Quick
  Review corrections use the same generation restart result and reinstall the
  status watcher. Quick Review surfaces close before graph cleanup. Foreground
  shutdown exact-stops executions as suspended logical jobs rather than using
  recursive workflow deletion.
- A report-only compatibility path lets workers that were already running from
  strict pre-version-2 records finish reporting. It grants no adoption,
  restart, cleanup, landing, or tmux mutation authority.

## Critical self-review

- Uniformity: implementation, direct reviewer, and reviewer-descendant jobs use
  the same execution, terminal, restart, event, suspension, and cleanup code.
- Tmux: all production mutations select the recorded explicit socket. Exact
  ownership is revalidated in the server command that performs termination or
  input routing. Absent sessions are idempotent; mismatches fail closed.
- Crash consistency: durable intent precedes execution creation, authority
  rotation precedes launch, report replacement precedes status publication,
  status acknowledgement is atomic, cleanup intent precedes land/stop, and the
  root is deleted last.
- Drift and isolation: job/status/socket symlinks and status replacement fail;
  project and workspace identity must match their pinned canonical records;
  recursive selection requires root, workflow token, owner, and project.
- Recovery: terminal and blocked events remain at the durable cursor until
  acknowledged. Persisted Pi event messages suppress only already-delivered
  wakes. New generations cannot replay old terminal events. Interrupted
  creation, lost execution, incomplete cleanup, and suspended sessions remain
  inspectable and retryable.

## Verification

- `python3 -m unittest tests.test_scufris_jobs tests.test_quick_review
tests.test_quick_review_preview tests.test_scufris_artifacts_prune
tests.test_scufris_dashboard` - 50 passed.
- Focused lifecycle module: 24 passed. It uses a unique absolute `tmux -S`
  socket per test. Coverage includes exact terminal teardown, blocked recovery,
  ordered acknowledge and replay, generation restart, status replacement,
  server-side replacement revalidation, creation and restart crash windows,
  report publication faults, root-last partial failure and retry, three-level
  reviewer graphs, recursive reviewer landing, project configuration drift,
  socket symlinks, cross-owner workflows, same-socket unrelated sessions, and
  an unchanged ambient/default server snapshot.
- `npm run check` - TypeScript passed, all 61 tests passed, and Prettier passed.
  The focused orchestration tests include persisted Pi event-ID recovery and
  Quick Review correction watcher restoration.
- Ruff format and lint for the changed Python helper, CLI, reporter, and tests -
  passed.
- ShellCheck and `bash -n scripts/scufris-dev` - passed.
- `git diff --check` - passed.
- `nix flake check` - all 17 compatible-system checks passed. An initial run
  exposed launcher fallback assumptions about `HOME` and runtime `mkdir`; the
  final launcher uses a writable runtime fallback and includes coreutils.

## Residual concerns

- Version 1 records intentionally remain fail-closed for ownership and cleanup
  because they lack a socket and random execution token. A narrow report-only
  compatibility path exists for workers that were already running during this
  upgrade. Manual inspection is required before removing such legacy tmux
  sessions.
- Tmux server-side `if-shell -F` command-queue serialization is the atomicity
  boundary. The focused race test verifies that a changed owner reaches only
  the mismatch branch, but it does not modify tmux itself to force an internal
  scheduler interleaving.
