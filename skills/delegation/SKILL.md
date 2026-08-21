---
name: scufris-delegation
description: Full-send bounded project work to an independent Pi or Claude worker, then mediate its decisions, blockers, review, and landing. Use automatically when Pair determines a request is ready for delegation or when an owned job emits an actionable event.
---

# Scufris delegation

Use Scufris native agent tools. Do not invoke tmux, sprout, Pi, Claude, or Plannotator directly.

## Readiness

1. Inspect the smallest relevant project context first: instructions, conventions, docs, task artifacts, relevant code and tests, Git state, recent history, and user style.
2. Full-send only one bounded outcome with no unresolved product decision, no durable architectural decision, known constraints and checks, and a self-contained handoff.
3. Keep consequential, multi-stage, multi-agent, or compaction-sensitive work in a Tatr task folder. Retain distilled decisions, not transcripts. Add design, notes, demos, diagrams, or other artifacts only when useful.
4. Commit accepted design artifacts before delegation.
5. Continue Pair for unresolved decisions. Workers can produce bounded research, prototypes, demos, or diagrams while Pair continues. Workers never spawn workers.

## Spawn

1. Confirm the request is project work in a Git repository.
2. Use the current repository when the request targets it. For another named repository, or outside Git, call `scufris_agent_projects` and select its exact opaque project ID.
3. Choose `pi` unless the user requests Claude or a specific Claude model.
4. Make `instructions` self-contained. Include the exact problem, outcome, scope, accepted design, artifacts, constraints, non-goals, applicable checks, completion conditions, and required task evidence. Never put filesystem paths or commands in tool arguments.
5. For a clear task name, derive a lowercase alphanumeric single-hyphen `feature`, maximum 48 characters. Omit it when no clear name exists. Never request a job-ID feature.
6. Pass `project` only for a discovered cross-project target. Pass `model` or `thinking` only for a user-requested override.
7. Call `scufris_agent_spawn` once and retain its `job_id`.
8. Tell the user that the bounded worker is independent and Pair remains available.

## Mediation

- `working`: UI telemetry only. Do not inspect or mention it unless the user asks.
- `needs-decision`: inspect with `include_report: true`. Present the real decision, worker recommendation, and consequences. Options are not exhaustive. Accept combinations, modifications, annotations, and other proposals. Send the resolved answer to the same worker once.
- `blocked`: inspect with `include_report: true`. Resolve the unblock condition when possible. Otherwise ask only for the foreground decision or external action, with the worker evidence and consequence. Send the resolution to the same worker once.
- `review-ready`: review starts automatically through Plannotator's public event API. Do not open another review or spawn an independent reviewer unless the user requests one.
- Review feedback returns automatically to the same worker. Approval requires its exact `done` acknowledgment before guarded local landing.
- `done`: inspect the report for a non-review result. Reviewed coding work lands automatically only after the required approval acknowledgment.
- `failed` or lifecycle `blocked`: inspect the report and event detail. Give a concrete recovery recommendation.

## Operations

- Use `scufris_agent_list` for compact owned-job state.
- Use `scufris_agent_inspect` for event history. Include the report only when its content is needed.
- Use `scufris_agent_send` once for short literal steering. Submitted delivery is not an acknowledgment.
- Use `scufris_agent_stop` only for cancellation or required termination.
- Do not adopt or infer work from an orphan notification.
