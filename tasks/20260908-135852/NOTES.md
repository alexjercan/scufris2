# Notes

## What was built

`control.wake` on the control socket, forwarded to the agent as `agent.wake`,
delivered by the service extension as
`pi.sendMessage({ customType, content, details, display: true },
{ deliverAs: "followUp", triggerTurn: true })`.

Chain: `scufris-ctl wake` -> `control.sock` -> `Service::control_wake` ->
`agent.sock` -> `AgentClient` -> `pi.sendMessage` follow-up.

## Decisions the task did not fix

- **Wire field names.** `custom_type`, `text`, and optional `details`, in that
  shape on both `control.wake` and `agent.wake`. One validator
  (`service.rs::wake`) checks both, so what control accepts is exactly what the
  agent is handed.
- **`details` is a JSON object, not a string.** Every wake handler in the tree
  reads structured details (`orchestration.ts:181` reads
  `details?.event_id`; `briefing.ts:145` sends `{date, profile, sources}`).
  A string would have broken the "existing wake handlers keep working"
  requirement. It is bounded by `MAX_DETAILS_BYTES` over its serialized bytes
  and must be an object; `null`, arrays, and scalars are refused.
- **Positive acknowledgment.** Added `control.wake_ack { id }`, named after the
  existing `surface.message_ack`. Without it the caller could not tell a
  delivered wake from a dropped one.
- **Refusal code.** `agent_unavailable`, reusing the code
  `surface.message` and `surface.abort` already return for the same condition.
  `scufris-ctl wake` exits non-zero and prints `agent_unavailable: ...`.
- **`--custom-type` has a default of `scufris-wake`.** The caller can still
  name any identifier. A default keeps a one-argument `scufris-ctl wake "..."`
  usable from a timer unit.
- **`--details` is parsed and rejected client-side.** An invalid control
  message closes the connection with no response, so a socket-side rejection
  would have surfaced as the misleading "update the host and client together".
- **`AgentClientOptions.wake` is required, not optional.** An embedder that
  forgets it would drop wakes silently, which is the exact failure the task
  rules out.
- **Swift protocol bumped too.** `tools/release/check_versions.py` requires the
  Rust, TypeScript, and Swift protocol constants to agree, so
  `surfaces/ios/Sources/Protocol.swift` went to 6 with its tests.
- **Prose version bumps.** Every "protocol v5" claim in source comments and
  `docs/` was updated. Names that are not version claims
  (`scufris-response-v5`, test temp-directory prefixes) were left alone.

## Verification

`cargo test`: pass.

```
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 321 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

New Rust tests:

- `service::a_wake_reaches_the_agent_and_is_neither_recorded_nor_echoed`
- `service::a_wake_leaves_the_response_association_alone`
- `service::a_wake_with_no_agent_is_refused_with_a_code_and_a_detail`
- `service::a_wake_carries_the_same_bounds_on_control_and_agent_channels`
  (shared/control)
- `service::only_the_control_channel_carries_a_wake` (shared/control)
- `websocket_payloads_use_the_strict_surface_decoder` now also asserts the
  gateway decoder refuses a `control.wake` line.

`python3 -m unittest discover -s tests -p 'test_*.py'`: pass.

```
Ran 291 tests in 46.919s

OK
```

`nix flake check`: pass.

```
running 42 flake checks...
all checks passed!
warning: The check omitted these incompatible systems: aarch64-darwin, aarch64-linux
```

`npm run check`: version check, typecheck, and 105 tests pass; `format:check`
fails on two task files this task must not touch.

```
product 2.1.7; surface protocol 6
...
1..105
# tests 105
# pass 105
# fail 0
...
Checking formatting...
[warn] tasks/20260908-103403/TASK.md
[warn] tasks/20260908-140024/TASK.md
[warn] Code style issues found in 2 files. Run Prettier with --write to fix.
```

Both files are the concurrent agent's uncommitted work under `tasks/`, outside
this task's scope. `git show HEAD:tasks/20260908-103403/TASK.md` is Prettier
clean, so the working-tree edit introduced it. Prettier over everything else
passes:

```
npx prettier --check . '!tasks/20260908-103403/**' '!tasks/20260908-140024/**'
Checking formatting...
All matched files use Prettier code style!
```

`tasks/20260908-135852/TASK.md` itself was Prettier-formatted; the only change
is leading whitespace inside two multi-line inline code spans.

## Not committed

`npm run check` does not pass end to end, for a reason outside this change, so
nothing was committed and the task stays OPEN.
