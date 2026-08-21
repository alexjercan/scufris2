---
name: scufris-delegation
description: Delegate long-running coding work to an independent Pi or Claude worker while the foreground conversation continues. Use when the user asks to delegate, requests substantial parallel project work, or needs to inspect, steer, or stop a delegated job.
---

# Scufris delegation

Use Scufris native agent tools. Do not invoke tmux, sprout, Pi, or Claude directly.

## Spawn

1. Confirm the request is coding work in a Git repository.
2. Use the current repository when the request targets it. For another named repository, or when the session is outside Git, call `scufris_agent_projects` and select its exact opaque project ID.
3. Choose `pi` unless the user requests Claude or a specific Claude model.
4. Preserve the user's requirements in `instructions`. Never put filesystem paths or commands in tool arguments.
5. Pass `project` only for a discovered cross-project target. Pass `model` or `thinking` only when the user requests an override.
6. Call `scufris_agent_spawn` once and retain its `job_id`.
7. Tell the user the worker is independent and the foreground conversation remains available.

## Follow-up

- Use `scufris_agent_list` for compact owned-job state.
- Use `scufris_agent_inspect` for events. Include the report only when its result is needed.
- Use `scufris_agent_send` once for short literal steering. A submitted result is not an acknowledgment.
- Use `scufris_agent_stop` when the user asks to cancel or when a worker must be terminated.
- Mediate `needs-decision`, `blocked`, `done`, and `failed` events with the user.
- Do not adopt or infer work from an orphan notification.
