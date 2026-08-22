# Fix prose-only final response speech

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, response, speech

## Goal

Make structured spoken-only final responses available to speech playback after Pi persists the terminating tool exchange. Preserve fail-closed extraction for unsafe assistant output.

## Cause

The response extension appends the validated custom response during `message_end`, before Pi persists the assistant tool-call message. Pi then persists the tool call after the custom entry. Speech scans backward at settlement, reaches the final tool-call assistant message first, and correctly refuses to fall back across it. This hides the safe custom response and produces `No safe response to speak.`

## Accepted correction

- Prepare and scrub the final response during `message_end`.
- Append the custom response during final-tool execution, after Pi has persisted the assistant tool call.
- Keep one custom response for malformed batches with more than one final call.
- Keep direct-text fallback and fail-closed speech validation unchanged.

## Implementation

- `message_end` still validates the spoken field, stores optional detail, and scrubs private arguments before Pi schema validation.
- Final-tool execution now appends the prepared custom response after Pi persists the assistant tool call.
- Shared prepared entries are appended once when a malformed batch contains more than one final call.
- Direct-text fallback and speech extraction are unchanged. Unsafe final assistant output still prevents fallback to an older response.

## Regression evidence

- Before the fix, the focused response test failed with 8 passes and 1 failure. Settled extraction returned no paragraph after the modeled custom-entry, assistant-tool-call, and tool-result order.
- After the fix, focused response and speech tests passed: 14 tests.

## Initial verification

- `npm run check` - passed. TypeScript, 49 tests, and Prettier passed.
- `nix flake check -L` - passed, including 25 checks on x86_64-linux.
- `git diff --check` - passed.

## Post-synchronization verification

- `sprout sync fix-prose-response-speech` - passed; already up to date.
- `node --experimental-strip-types --test --test-concurrency=1 tests/response.test.ts tests/speech.test.ts` - passed, 14 tests.
- `npm run check` - passed. TypeScript, 49 tests, and Prettier passed.
- `nix flake check -L` - passed from the synchronized tree.
- `git diff --check` - passed.
- Existing Nix unknown-output, deprecated platform predicate, and incompatible-system omission warnings remain.

## Limitations

- Automated tests model the installed Pi event and persistence order. No live TUI audio playback was run in the delegated worker.

## Revisions

- Starting revision: `0636ae3a45a1b5df4c85f96f948bb570b4059ac1`.
- Implementation revision: `d3a4a0f699e1b1d9288488c5dfbef35a948f5706`.
- Final evidence: this task record's closing commit.
