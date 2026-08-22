# Render each foreground response once

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, response

## Cause

Pi emits `message_end` before it persists and renders the replacement assistant message. The direct-text fallback called `appendResponse`, which appended and immediately rendered a `scufris-response-v1` custom entry. The same handler then replaced the finalized assistant text with identical compact prose and `/detail` output. Pi rendered both channels. The built-in assistant renderer added its normal output padding, which explains the leading whitespace on the second copy.

The structured final-response tool, artifact splitter, transcript mutation, Plannotator rows, and speech playback did not create the second copy. The fault was the fallback path combining a custom entry with a visible assistant replacement.

## Fix

- Separate artifact preparation from custom-entry append.
- Keep structured tool and command responses on their existing custom-entry path.
- Keep fallback output only in the finalized assistant record. This preserves model context, restored rendering, artifact privacy, and speech extraction without a second visible row.
- Add a regression that composes both live render channels and requires one row. Keep restored structured-entry coverage.
- Document the distinct structured and fallback persistence paths.

## Embedded Pair prompt

- Move the exact canonical text from `prompts/pair.md` into `extensions/scufris/identity.ts` with the same constant-based ownership as the final-response prompt.
- Preserve the exact 683-byte LF-terminated value and every role, turn, and post-compaction behavior.
- Remove filesystem reads, path resolution, the external prompt file, resource copying, and packaged prompt checks.
- Label the prompt-inspection provenance and section as embedded.
- Test the embedded value against an independent exact literal and verify that packaged resources omit the obsolete prompt directory.

## Tradeoff

Two durable response representations remain because they have different origins. Structured tools and extension commands need custom entries. Direct model text already owns a durable assistant record. A new unified transcript type would require removing useful assistant context or adding hidden context machinery.

Embedding removes standalone prompt-file iteration and reuse. It makes the identity policy atomic with the extension and prevents packaging or relative-path failures. The exported prompt constant remains independently testable and available to private prompt inspection.

## Initial verification

- `node --experimental-strip-types --test --test-concurrency=1 tests/response.test.ts` - passed, 8 tests after `npm ci` installed locked dependencies. Before the fix, the new live-channel assertion observes two rows.
- `npm run check` - passed, 48 TypeScript integration tests plus type and format checks.
- `python3 -m unittest discover -s tests -p 'test_*.py'` - passed, 29 tests.
- `ruff check scripts tests` and `ruff format --check scripts tests` - passed.
- `shellcheck scripts/scufris-dev` - passed.
- `nix fmt -- --check .` - passed.
- `nix build .#docs --no-link` - passed.
- `git diff --check` - passed.
- `nix flake check -L` - passed, 25 checks on x86_64-linux. Existing unknown custom-output, deprecation, and incompatible-system omission warnings remain.

## Post-synchronization verification

- `sprout sync fix-duplicate-response-output` - passed after each implementation revision; already up to date.
- Exact old-file-to-embedded-value comparison - passed; the value remains 683 ASCII bytes including LF.
- `node --experimental-strip-types --test --test-concurrency=1 tests/identity.test.ts tests/response.test.ts` - passed, 10 focused tests.
- `npm run check` - passed, 48 TypeScript integration tests plus type and format checks.
- `python3 -m unittest discover -s tests -p 'test_*.py'` - passed, 29 tests.
- `ruff check scripts tests` and `ruff format --check scripts tests` - passed.
- `shellcheck scripts/scufris-dev` - passed.
- `nix fmt -- --check .` - passed.
- `nix build .#docs --no-link` - passed.
- `git diff --check` - passed.
- `nix flake check -L` - passed. The first expanded-scope run built 25 checks on x86_64-linux; the post-sync run reused them. Existing unknown custom-output, deprecation, and incompatible-system omission warnings remain.

## Live acceptance

Automated coverage verifies exact prompt bytes, per-turn and post-compaction injection, role exclusion, prompt-inspection provenance, live and restored compact rows, and speech extraction. A real foreground provider turn, TUI restore, `/scufris-prompt` artifact review, Plannotator feedback, and audio playback remain for the user's interactive acceptance.

## Revisions

- Starting and landing revision: `064b1ff`.
- Duplicate-response implementation: `dd9e770`.
- Embedded-prompt implementation: `ce77f66`.
- Final evidence: this record's commit.
