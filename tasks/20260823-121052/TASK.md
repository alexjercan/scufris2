# Fix delegated lifecycle wake ordering and completion notifications

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, orchestration, notifications, response

## Problem

Delegated lifecycle replies feel out of order and incomplete. Scufris can wake
when `review-ready` is observed before the asynchronous review transition is
ready, while later milestones use only transient UI notifications. In
particular, Quick Review readiness and successful landing may not produce a
foreground Scufris update. Some jobs therefore appear silent or stuck even
when their lifecycle continues.

## Current evidence

- `extensions/scufris/agents.ts` sends a wake-triggering `review-ready` job
  event and then starts `beginReview()` without awaiting it.
- Preflight start, preflight findings, Quick Review readiness, approval, and
  successful landing use different combinations of `ui.notify` and job events.
- Successful landing uses `ui.notify` but does not send a wake-triggering job
  event.
- Errors and several terminal states do trigger foreground turns, so success
  and failure have different visibility.
- `working` events are intentionally quiet, but there is no explicit contract
  that separates routine progress, user notification, and assistant wake.

## Accepted outcome

- Define one ordered lifecycle event contract for delegated work and review,
  using generic `working`, `needs-decision`, `blocked`, `ready`, `done`, and
  `failed` worker events rather than the workflow-specific `review-ready`.
- Distinguish persisted/displayed events, transient UI notifications, and
  events that start a foreground assistant turn.
- Wake only after the corresponding externally meaningful state is ready.
  Keep `working` quiet and wake for decision, blocked, handoff, success, and
  failure events.
- Notify the user when review becomes actionable, when feedback returns to a
  worker, when landing begins or completes as appropriate, and when terminal
  success or failure occurs.
- Avoid duplicate, stale, or out-of-order foreground replies.
- Ensure every asynchronous review and landing path reaches a visible terminal
  or actionable state instead of appearing stuck.
- Preserve concise spoken responses for automatic wake turns.

## Replacement result

The fixed lifecycle was deleted. Workers now append only generic progress,
decision, blocked, milestone, success, and failure events. Poll offsets suppress
duplicate delivery. Quiet progress uses UI notification; every actionable or
terminal event sends one ordered follow-up that wakes foreground Scufris after
the event exists. Quick Review returns through the same wake path. Landing is
an explicit awaited tool result and never starts silently in the background.

## Verification evidence

- Focused TypeScript tests verify event parsing and wake selection.
- Python integration tests exercise ordered append-only worker events, project
  and general jobs, steering, inspection, and independent review jobs.
- `npm run check` passes.
- `python3 -m unittest discover -s tests -p 'test_*.py'` passes 13 tests.
- `nix flake check` passes all supported-system checks.
- `git diff --check` passes.

## Relationship

Workflow preference work is tracked separately in
`tasks/20260823-120412/TASK.md`. Its task-plan and lifecycle boundaries may
clarify this notification contract, but this bug must remain independently
verifiable.
