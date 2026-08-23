# Add explicit wake and Calm commands

- STATUS: OPEN
- PRIORITY: 80
- TAGS: workflow

## Goal

Add explicit, inspectable controls for optional worker-progress wakes and Calm
mode.

## Wake command

- Add `/wake minimal` and `/wake all`.
- `/wake` without an argument reports the current mode without changing it.
- Default to `minimal`.
- In `minimal`, `blocked`, `done`, and runtime-generated `failed` always start a
  real foreground Scufris turn. `working` remains a quiet UI update.
- In `all`, `working` also starts a foreground turn so Scufris can acknowledge
  or speak progress.
- Mandatory wake events cannot be disabled because workflow continuation relies
  on them.
- Persist and restore the mode with session-aware extension state, following
  the speech command pattern.

## Calm command

- Replace toggle-only behavior with `/calm on` and `/calm off`.
- `/calm` without an argument reports the current value without changing it.
- Preserve the configured default and session restoration behavior.
- Reject unknown arguments with clear usage text.

## Acceptance

- Wake events create actual agent turns, not only notifications.
- Speech can announce a worker event when speech is enabled and that event wakes
  Scufris.
- `minimal` and `all` behavior is deterministic and tested.
- Repeating `/calm on` or `/calm off` is idempotent.
- Both commands report their current state when called without arguments.

## Dependencies

Run after `20260823-153411` because wake policy depends on the simplified state
model.
