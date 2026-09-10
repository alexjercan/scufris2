# Review

## Round 1

- Reviewer: separate read-only Pi review agent, `openai-codex/gpt-5.6-sol`, xhigh
- Range: `ea2ca2d..2b99134`
- Verdict: changes requested

## Findings

### Medium: audit eviction could select an active row

`host/service/src/briefings.rs` selected any delivered non-attention row for eviction. A malformed but protocol-valid combination of `collection == collecting` and `delivery == delivered` is active by the shared predicate but met that eviction condition.

Resolution: fixed. Eviction now also requires `!row.active()`. The bounded-audit test includes such an active delivered row, proves a full protected audit refuses insertion, then proves a dismissed terminal row yields while the active row remains.

### Medium: desktop lost briefing dismissal refusals

The desktop link reduced every rejection with an ID to a submission refusal. The conversation state ignored a briefing ID because it did not match an outstanding chat submission. A not-dismissible or persistence failure therefore left the briefing visible without an error.

Resolution: fixed. `LinkEvent::Refused` now preserves ID, operation, code, and detail. Message refusals keep their existing submission behavior. Other request refusals become a visible HUD notice and do not settle an unrelated message. Conversation and app tests cover the two state transitions.

## Verified by reviewer

The reviewer found the partial predicate, success transience, terminal delivered dismissal gate, idempotent durable cross-surface dismissal, and protocol version changes consistent.

## Adjudication

Both findings were grounded in reachable protocol/store state or the live desktop response path and were accepted at medium severity. Neither was a blocker because neither corrupted canonical conversation or collection artifacts, but both violated explicit visibility and retention requirements. No second review pass was requested or run.
