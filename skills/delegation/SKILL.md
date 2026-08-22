---
name: scufris-delegation
description: Full-send project work expected to take minutes to an independent Pi or Claude worker, then mediate its decisions, blockers, review, and landing. Use when Pair determines a request is substantial enough for delegation or when an owned job emits an actionable event.
---

# Scufris delegation

Use Scufris native agent tools. Do not invoke tmux, sprout, Pi, Claude, or Plannotator directly.

## Readiness

1. Route by expected scope and latency. Handle narrow project work that should take seconds in the foreground. Examples: read one named file, inspect a small task record, or answer a focused repository question.
2. Delegate work expected to take minutes. Examples: broad codebase review, substantial research, implementation, full checks, releases, and deployment. Do not delegate only because a request uses project tools.
3. Full-send only one bounded outcome with no unresolved product decision, no durable architectural decision, known constraints and checks, and a self-contained handoff.
4. Require the worker to inspect applicable project instructions, conventions, docs, task artifacts, code, tests, Git state, history, and user style before changing files.
5. Keep consequential or compaction-sensitive accepted design in a Tatr task folder through delegated task maintenance.
6. Continue Pair for unresolved decisions. Workers can produce bounded research, prototypes, demos, or diagrams while Pair continues. Workers never spawn workers.

## Spawn

1. Use delegation for the substantial work defined above. Keep quick foreground inspection bounded; delegate when it reveals broader work.
2. Use the current repository when the request targets it. For another named repository, or outside Git, call `scufris_agent_projects` and select its exact opaque project ID.
3. Choose `pi` unless the user requests Claude or a specific Claude model.
4. Make `instructions` self-contained. Include the exact problem, outcome, scope, accepted design, artifacts, constraints, non-goals, applicable checks, completion conditions, and required task evidence. Require the worker to discover and follow project instructions and context. Never put filesystem paths or commands in tool arguments.
5. For a clear task name, derive a lowercase alphanumeric single-hyphen `feature`, maximum 48 characters. Omit it when no clear name exists. Never request a job-ID feature.
6. Select exactly one required `review` policy:
   - Use `code` for implementation correctness and maintainability.
   - Use `consumer` for documentation, setup, and user outcomes.
   - Use `operations` for deployment, reliability, diagnostics, and rollback.
   - Use `interface` for APIs, schemas, protocols, and caller contracts.
   - Use `none` only when the result is not landable. Do not include a brief for `none`.
7. For a landable profile, write a concise `brief` with the accepted outcome and audience. Exclude implementation claims, worker instructions, and broad quality checklists.
8. Omit `cleanup` for the default `remove` policy. Pass `cleanup: retain` only when the user asks to keep the landed branch and worktree resources. Remove cleanup can evict any remaining windows in the feature tmux session after landing.
9. Pass `project` only for a discovered cross-project target. Pass `model` or `thinking` only for a user-requested override.
10. Call `scufris_agent_spawn` once and retain its `job_id`.
11. Tell the user that the bounded worker is independent and Pair remains available.

## Mediation

- `working`: UI telemetry only. Do not inspect or mention it unless the user asks.
- `needs-decision`: inspect with `include_report: true`. Present the real decision, worker recommendation, and consequences. Options are not exhaustive. Accept combinations, modifications, annotations, and other proposals. Send the resolved answer to the same worker once.
- `blocked`: inspect with `include_report: true`. Resolve the unblock condition when possible. Otherwise ask only for the foreground decision or external action, with the worker evidence and consequence. Send the resolution to the same worker once.
- `review-ready`: Scufris announces an input-disabled `preflight-<review_id>` window in the worker's feature tmux session and runs the actual independent reviewer there. The user can inspect it without steering it. Scufris opens Plannotator only after exact preflight approval. Do not select the window, open another review, or spawn a reviewer.
- Preflight findings and Plannotator feedback return automatically to the same worker. Correction checks reuse the owned reviewer window and saved reviewer session. Human feedback invalidates both before a fresh sequence. A third preflight change request blocks for Pair mediation. Human Plannotator approval still requires the worker's exact `done` acknowledgment before guarded local landing.
- `done`: inspect the report for a non-review result. Reviewed coding work lands automatically only after the required approval acknowledgment.
- `landed-with-retained-resources`: landing succeeded. Do not call it a merge failure or attempt rollback. Inspect the job once and give the manual stop or `sprout rm <feature>` cleanup action from the event context.
- `failed` or lifecycle `blocked`: inspect the report and event detail. Give a concrete recovery recommendation. After resolving a retryable review-precondition blocker, call `scufris_agent_retry_review` once. Do not send a worker message or request a new commit for that retry.

## Operations

- Use `scufris_agent_list` for compact owned-job state.
- Use `scufris_agent_inspect` for event history. Include the report only when its content is needed.
- Use `scufris_agent_send` once for short literal steering. Submitted delivery is not an acknowledgment.
- Use `scufris_agent_retry_review` only after mediating the reported transient review precondition. It takes a fresh snapshot and is not a general review or landing retry.
- Use `scufris_agent_stop` only for cancellation or required termination.
- Do not adopt or infer work from an orphan notification.
