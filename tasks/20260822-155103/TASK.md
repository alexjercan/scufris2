# Speak wake-triggered Scufris responses

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, speech, orchestration

## Goal

Speak each safe settled Scufris response produced by an extension-triggered wake turn. Preserve one playback for ordinary prompts and all speech safety and mode controls.

## Cause

Installed Pi 0.84.2 calls `before_agent_start` for normal user prompts. An idle `pi.sendMessage(..., { triggerTurn: true })` wake calls `_runAgentPrompt` directly and bypasses that event. Both paths emit `agent_start` and end at `agent_settled`.

The speech extension captured the mode only at `before_agent_start`. Job lifecycle and detail feedback wake turns therefore reached settlement without an armed playback. The validated final response remained silent.

## Accepted correction

- Move mode capture from `before_agent_start` to the common `agent_start` lifecycle event.
- Keep the existing awaiting-settlement guard and one `agent_settled` playback path.
- Keep final-response extraction, safety validation, mode persistence, cancellation, error handling, and TUI isolation unchanged.
- Test ordinary prompts together with review-ready, done, and human review feedback wake turns.

## Regression evidence

After `npm ci`, the new focused lifecycle test failed before the correction with 5 passes and 1 failure. The ordinary response played once. The extension-triggered response did not play.

The corrected focused speech suite passes 6 tests. Its lifecycle regression models Pi's persisted raw custom wake message, common `agent_start`, validated final assistant response, and `agent_settled` ordering. It asserts only the final responses enter playback and the ordinary turn is not duplicated.

## Implementation

- Speech now arms on the common low-level run start used by both prompt and wake paths.
- One existing settlement guard spans retries and queued continuations.
- Settled extraction reads only branch entries added after the current run starts.
- Protocol and user voice documentation now include automatic wake response speech.

## Review correction

Independent preflight found that a wake run with no new assistant response could skip its raw custom message and replay an older safe response from the branch. The new failure regression reproduced this with 6 passes and 1 failure: the prior response played twice.

The correction records the branch length at the first `agent_start` in the settlement cycle and limits final extraction to entries after that boundary. A run with no new safe assistant response now emits the existing warning and never starts playback. The corrected focused speech suite passes 7 tests.

## Initial verification

- `node --experimental-strip-types --test --test-concurrency=1 tests/agents.test.ts tests/response.test.ts tests/speech.test.ts` - passed, 31 tests.
- `npm run check` - passed. TypeScript, 50 tests, and Prettier passed.
- `nix flake check` - passed, including 26 checks from the dirty source tree.
- `git diff --check` - passed.

## Post-synchronization verification

- `sprout sync speak-wake-responses` - passed; already up to date.
- `node --experimental-strip-types --test --test-concurrency=1 tests/agents.test.ts tests/response.test.ts tests/speech.test.ts` - passed, 31 tests.
- `npm run check` - passed. TypeScript, 50 tests, and Prettier passed.
- `nix flake check` - passed, including 25 checks on x86_64-linux.
- `git diff --check` - passed.
- Existing Nix unknown-output, deprecated platform predicate, and incompatible-system omission warnings remain.

## Review correction verification

- Before the correction, focused speech tests failed with 6 passes and 1 expected failure. A response from the prior run played twice.
- Corrected focused speech tests - passed, 7 tests.
- Corrected focused agents, response, and speech tests - passed, 32 tests.
- `npm run check` - passed. TypeScript, 51 tests, and Prettier passed.
- `nix flake check` - passed, including 25 checks on x86_64-linux.
- `git diff --check` - passed.
- `sprout sync speak-wake-responses` - passed after correction commit; already up to date.
- Post-sync focused agents, response, and speech tests - passed, 32 tests.
- Post-sync `npm run check` - passed. TypeScript, 51 tests, and Prettier passed.
- Post-sync `nix flake check` - passed from cache.
- Post-sync `git diff --check` - passed.

## Limitations

No live TUI audio playback was run in this delegated worktree.

## Revisions

- Starting revision: `6d11129aa1da48f1e389c13f61d30ca49618fad9`.
- Initial implementation revision: `d949bc6`.
- Initial evidence revision: `1564ac0`.
- Review correction revision: `f6074a8`.
- Final correction evidence: this task record's closing commit.
