# Make Quick Review work like a GitHub PR review

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: quick-review

## Design decisions

- The existing section is the bounded file/change review unit. Its persisted `viewed` boolean remains authoritative, but the UI now changes it through a checkbox in the section header.
- Section comments are always non-blocking. Their composer has one submission action: Add comment. Exact-revision explanation, context, reviewer-question, and full-diff tools remain separate walkthrough utilities.
- The final overall comment is submitted only with a terminal action. It is included in approval payloads and is the required explanation for Request changes.
- One approval protocol action is used. Its UI label is derived from section comments plus the current overall comment. This avoids two semantically duplicate approval controls.
- Legacy `changeRequests` state remains validated for compatibility, but the new UI cannot create section-level blocking requests. Existing requests are rendered in their section and final review, continue to block approval, and are included with the required overall explanation in the routed change-request payload.
- Viewed checkbox mutations are transactional around persistence. A failed write restores both `viewed` and section status before returning the authoritative error state; the checkbox scope reports that error visibly.
- Approval still persists before routing. If exact-revision terminal finalization fails, it persists `approved: false` as a durable compensating rollback before allowing a retry.

## Independent-review corrections

Addressed all three findings:

1. Added durable approval rollback and retry coverage for terminal finalization failure.
2. Added explicit visible and payload-preserving handling of legacy section change requests.
3. Added transactional checkbox state rollback and browser error-rendering coverage for persistence failure.

## Verification

- `node --experimental-strip-types --test --test-concurrency=1 tests/walkthrough.test.ts`: passed 18 focused bridge, state, payload, rollback, and lifecycle tests.
- `python3 -m unittest tests/test_quick_review.py`: passed 7 focused renderer and transport tests.
- `ruff check tools/quick-review/quick_review.py tests/test_quick_review.py`: passed.
- `ruff format --check tools/quick-review/quick_review.py tests/test_quick_review.py`: passed.
- `python3 -m py_compile tools/quick-review/quick_review.py tests/test_quick_review.py`: passed.
- `npm run check`: passed type checking, all 49 TypeScript tests, and Prettier validation.
- `git diff --check`: passed.
- Rendered a representative page in headless Chromium and inspected the screenshot. It showed the header Viewed checkbox, threaded section comment, single Add comment composer action, retained exact-revision utilities, overall review area, and GitHub-like action hierarchy.

The task remains `IN_PROGRESS` for foreground review and landing.
