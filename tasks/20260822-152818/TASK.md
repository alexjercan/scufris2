# Fix final response termination and delegation threshold

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: bug, orchestration

## Goal

- Stop the structured final-response tool after one provider turn.
- Route by expected latency and scope. Handle work that takes seconds in the foreground. Delegate work that takes minutes.

## Cause

- The message-end scrub replaced valid tool arguments with an undeclared artifact ID before Pi validated and executed the tool. Validation failed, so the terminating result never ran and Pi requested another model response.
- The canonical prompt and delegation skill used an absolute project-work rule instead of a latency and scope judgment.

## Implementation

- Keep the persisted final tool call schema-valid after removing private detail. Store only `spoken` in its arguments. The prior `artifact_id` replacement failed Pi validation before `execute`, so `terminate: true` never reached the agent loop.
- Replace the absolute delegation rule with a scope and latency rule. Foreground Scufris handles work expected to take seconds. It delegates work expected to take minutes.
- Name direct and delegated examples in the canonical prompt, skill, architecture, protocol, and manual.

## Tradeoffs

- The assistant tool-call record no longer carries the artifact ID. The session-owned custom response entry remains the durable artifact reference and visible response representation.
- Routing remains model judgment. A deterministic timer or request classifier would not know actual project scope and would add a second policy source.

## Verification

- Focused identity and response tests - passed, 10 tests.
- `npm run check` - passed, including 48 TypeScript integration tests.
- `nix flake check -L` - passed, 26 checks on x86_64-linux. Existing unknown custom-output, deprecation, and incompatible-system omission warnings remain.
- `git diff --check` - passed.
