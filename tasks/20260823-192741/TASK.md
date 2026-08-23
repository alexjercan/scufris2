# Fix foreground workflow acknowledgments

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: extension

## Request

Fix foreground Scufris turns that ended after workflow tools without producing a
safe spoken response. Preserve strict nonblocking delegation and cover action
errors, tool batches, wake delivery, response safety, tests, and documentation.

## Investigation

- Pi 0.84.2 preflights sibling tool calls before execution. Tool results with
  `terminate: true` skip the normal model follow-up only when the batch
  terminates. This made the old spawn and send result incompatible with a
  model-authored final acknowledgment.
- `before_agent_start` does not run for idle extension-triggered wake delivery.
  Wake messages therefore need explicit final-response guidance in addition to
  the normal assembled system prompt and tool guidance.
- Settled speech already uses only a new validated response after the run entry
  boundary. A tool-only wake safely refuses old output, but repeated failures
  emitted the same warning on every turn.
- Commit `5680a38` introduced spawn/send termination to enforce no waiting. The
  no-wait rule remains valid; termination was stronger than required.

## Decisions

- Remove terminating results from successful spawn and send actions. Mark
  successful spawn, send, stop, land, Quick Review, and Plannotator review
  actions as awaiting one model-authored acknowledgment.
- Require each meaningful action and each final-response call to be alone in
  its tool batch. While an acknowledgment is pending, block every follow-up
  except `scufris_final_response`. This preserves the spawn/send no-sleep,
  no-wait, no-poll, no-inspect rule without suppressing the response turn.
- Mark the gate only after action success. Tool errors remain eligible for one
  concise final explanation and must not claim success. A failed final response
  leaves the gate closed so only a safe retry is allowed.
- Add explicit final-response guidance to worker, review, and detail-feedback
  wake messages because those turns bypass `before_agent_start`.
- Keep acknowledgments model-authored and contextual. Do not add deterministic
  speech. Coalesce consecutive missing-response warnings until a valid response
  is played.

## Implementation

- `extensions/scufris/workflow/orchestration.ts`: shared action policy, action
  batch validation, pending-acknowledgment gate, successful action markers,
  wake instructions, and nonterminating spawn/send results.
- `extensions/scufris/voice/response.ts`: final-response policy now covers
  workflow actions, wakes, and tool errors.
- `extensions/scufris/voice/speech.ts`: repeated missing-response failures warn
  once until a safe response is available.
- `skills/workflow/SKILL.md`, `docs/src/workflow.md`, and
  `docs/src/responses.md`: matching durable behavior and safety rules.
- Focused tests cover action batches, errors, blocked polling/inspection,
  spawn/send followed by a terminating safe final response, wake guidance, and
  non-repetitive warning failure behavior.

## Verification

- Focused: `node --experimental-strip-types --test --test-concurrency=1
tests/agents.test.ts tests/response.test.ts tests/speech.test.ts` - passed, 27
  tests.
- Full extension check: `npm run check` - passed. TypeScript, 57 tests, and
  Prettier all passed.
- `git diff --check` - passed.

## Independent review corrections

The independent review found three lifecycle gaps. The task was reopened and
all findings were resolved:

- The workflow lifecycle now publishes pending acknowledgment state to response
  finalization. Ordinary assistant text during that boundary is removed before
  persistence and speech extraction. Only one successful
  `scufris_final_response` execution clears the gate. If the run settles
  without success, the gate resets so later turns are not stuck.
- Response validation now keeps only ephemeral unsaved arguments until tool
  execution. It creates the response entry and private detail artifact during a
  successful final tool execution. Rejected batches create nothing, and an
  error `tool_result` clears any prepared call after preflight blocking.
- Missing-response warning suppression now resets only after playback resolves
  successfully. A synthesis or playback failure leaves suppression active.

Added real lifecycle coverage for pending action event delivery, a text-only
assistant stop, response hiding, speech extraction, and settled gate cleanup.
Added `message_end -> tool_call` preflight rejection `-> tool_result` coverage
for a final call with private detail and a sibling tool. The test verifies no
response entry, speech, or artifact. Spawn/send coverage now uses the registered
lifecycle handlers and verifies duplicate execution rejection. Speech coverage
now exercises missing response, playback failure, then another missing
response.

Post-review verification:

- Focused: `node --experimental-strip-types --test --test-concurrency=1
tests/agents.test.ts tests/response.test.ts tests/speech.test.ts` - passed, 29
  tests.
- Full extension check: `npm run check` - passed. TypeScript, 59 tests, and
  Prettier all passed.

## Follow-up

The Sprout remains unlanded for user Quick Review. No broad `nix flake check`
was needed because package and Nix surfaces did not change.
