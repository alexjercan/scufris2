# Add explicit wake and Calm commands

- STATUS: CLOSED
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

## Decisions

- Store Wake and Calm values as versioned Pi custom session entries. Restore the
  latest valid entry on session start and tree navigation.
- Keep wake delivery in the workflow extension. Every selected wake uses a
  `followUp` custom message with `triggerTurn: true`; quiet `working` events use
  only the existing UI notification path.
- Do not append duplicate state entries for idempotent commands. State queries
  and invalid arguments also do not mutate the session.
- Preserve Calm's existing enabled default. A stored session value takes
  precedence over that default.

## Verification

- `node --experimental-strip-types --test --test-concurrency=1 tests/agents.test.ts tests/calm.test.ts` - 10 tests passed.
- `npm run typecheck` - passed.
- `npm run check` - passed: type checking, all 55 tests, and repository format
  verification.
- `npm run format:check` - passed after formatting the final workflow edit.
- `git diff --check` - passed.

### Independent review follow-up

- Added a production-path delivery helper test for `working`, `blocked`, `done`,
  and runtime-generated `failed` updates in both Wake modes. It asserts the
  exact `deliverAs: "followUp"` and `triggerTurn: true` options, event message
  metadata, mandatory wakes, and the quiet `working` notification in `minimal`.
- `node --experimental-strip-types --test --test-concurrency=1 tests/agents.test.ts` - 9 tests passed.
- `npm run check` - passed with all 55 tests.
