# Interactive review walkthrough extension

- STATUS: OPEN
- PRIORITY: 100
- TAGS: review, walkthrough, pi, scufris

## Summary

Build a post-preflight review experience for Scufris and Pi. A separate walkthrough reviewer agent analyzes the exact implementation revision, the code diff, the implementation conversation, task evidence, and the approved preflight report. It writes a normal Markdown walkthrough that highlights the important literal diff blocks, explains why they exist, and identifies what Alex should verify. A Pi extension parses that Markdown and presents it as an interactive local review story.

This is not a replacement for preflight review. Preflight remains an independent correctness and safety gate. Plannotator remains available for exhaustive line-by-line review.

## User flow

1. The implementation worker finishes.
2. The mandatory independent preflight reviewer runs and must approve the exact revision.
3. A separate walkthrough reviewer receives the exact revision, diff, implementation conversation, task evidence, and preflight result.
4. The walkthrough reviewer produces a Markdown artifact and a machine-readable completion record.
5. The Pi extension validates the artifact, renders it as an interactive review story, and keeps landing blocked.
6. Alex reviews important changes section by section.
7. Alex can mark a section as Looks good, ask for an explanation, request changes, or open the complete diff in Plannotator.
8. Approve allows guarded landing only after all critical sections are resolved. Request changes sends feedback to the implementation worker without requiring a full diff review.
9. Any implementation change invalidates the artifact and review state and sends the feature through preflight and walkthrough again.

## Walkthrough Markdown contract

The generated file must remain useful as ordinary Markdown when opened directly. Custom directives add semantics only for the renderer. The initial format should support:

```markdown
# Assessment Available Actions

:::walkthrough
status: ready
files: 7
added: 183
removed: 42
preflight: passed
:::

## Runtime filtering

:::change
id: runtime-filter
importance: critical
file: src/actions.py
lines: 42-61
:::

This is the central behavioral change.

```diff
+return [
+    action
+    for action in catalog.actions_for(resource_type)
+    if all(
+        preconditions.evaluate(p.expression, facts)
+        for p in action.preconditions
+    )
+]
```

:::review
Verify that multiple preconditions intentionally use AND semantics.
:::
```

Required semantics:

- The top-level walkthrough metadata includes status, file count, additions, deletions, and preflight result.
- Each important change has a stable ID, importance, file, and exact line or hunk reference.
- A change contains a human explanation and a literal diff block.
- Review prompts identify a concrete thing to verify.
- The generator labels author-reported rationale, code-confirmed facts, reviewer inference, and unknown reasoning separately.
- Malformed or unsupported directives must not crash the renderer; preserve readable Markdown and surface a validation warning.
- All metadata and actions are bound to the exact reviewed revision.

## Interactive review UI

The Pi extension should serve a local, owned review surface or use the project's existing safe UI mechanism. It should render:

- Feature title and metadata summary.
- A plain-language What was built section.
- Ordered change cards with importance, explanation, and diff blocks.
- Explicit section states: Not reviewed, Looks good, Needs explanation, Change requested.
- Actions per section: Looks good, Explain, Request change, show context, open file, and ask reviewer.
- A final review area showing progress and actions: Approve, Request changes, and View full diff.

The renderer must not treat arbitrary Markdown or metadata as executable HTML or shell input. Use a known directive schema and sanitized Markdown rendering. The full diff action should hand the exact revision to Plannotator rather than trying to reproduce exhaustive diff review in the walkthrough UI.

Review state should be persisted separately from the generated Markdown. Store the artifact identity, exact revision, section states, questions, answers, and change requests in the owned task or job state. Do not mutate the source document for every click. Reject or invalidate state when the revision changes.

## Agent responsibilities

### Walkthrough reviewer

- Independently inspect the code and exact diff.
- Use the implementation conversation only as intent and decision context.
- Verify author explanations against code.
- Never invent missing rationale; mark it unknown.
- Rank behavior, safety, lifecycle, compatibility, data-flow, and maintenance changes above formatting, test boilerplate, synchronization, and evidence noise.
- Produce Markdown plus a bounded machine-readable completion result.
- Do not approve or land the feature.

### Pi extension

- Start the walkthrough job only after approved preflight.
- Enforce exact revision and ownership checks.
- Parse and validate the Markdown contract.
- Render the review story and maintain user review state.
- Route Explain and Ask reviewer requests to the walkthrough reviewer.
- Route Request changes to the implementation worker and invalidate the current review.
- Open Plannotator for exhaustive review when requested.
- Keep guarded landing blocked until explicit user approval.
- Stop, clean up, and invalidate resources idempotently during shutdown or cancellation.

Keep orchestration, lifecycle events, native tools, in-memory state, and polling in `extensions/scufris/`. Keep the walkthrough prompt and model-facing workflow in a skill or dedicated worker prompt. Keep deterministic parsing, validation, revision checks, and filesystem operations in small owning scripts or modules.

## Conversation and clarification model

The implementation conversation should be bounded or summarized into a structured handoff containing intent, decisions, alternatives, limitations, and relevant evidence. Do not pass an unbounded transcript by default.

The walkthrough reviewer should answer questions from its own report first. If intent remains genuinely unclear, Scufris may ask the implementation worker one bounded clarification. The answer is context and must not override independent code verification.

## State machine

Suggested states:

- `preflight-passed`
- `walkthrough-running`
- `walkthrough-ready`
- `section-reviewing`
- `needs-clarification`
- `changes-requested`
- `full-diff-open`
- `approved`
- `invalidated`
- `landed`

A section can move from `not-reviewed` to `looks-good`, `needs-explanation`, or `change-requested`. An explanation can resolve `needs-explanation`; a code change always invalidates every section state.

## Safety and correctness requirements

- No automatic landing from walkthrough generation or reviewer output.
- Explicit user approval is required.
- Never allow the walkthrough reviewer to mark user sections approved.
- Do not derive spoken or review content from rejected or unsafe response material.
- Bind all artifacts, state, review actions, and Plannotator launches to an exact revision.
- Prevent stale artifacts and stale approvals after synchronization, correction, or new commits.
- Keep full diff access available so summary omission cannot hide changes.
- Keep raw lifecycle payloads, unrestricted shell commands, arbitrary filesystem paths, URLs, and desktop operations out of model-facing interfaces.

## Implementation plan

1. Inspect current delegation, preflight, Plannotator launch, task evidence, Pi extension, and local UI patterns.
2. Define strict TypeScript schemas for walkthrough metadata, change blocks, review prompts, completion results, and persisted review state.
3. Add a walkthrough reviewer worker or skill that receives the exact revision and structured implementation handoff and writes the artifact.
4. Add deterministic Markdown parsing and validation with fixture coverage for valid, malformed, nested, and unsupported directives.
5. Add the Pi review surface and section state transitions. Start with a safe local HTML/TUI surface that is readable without JavaScript if practical.
6. Add action routing for explain, ask reviewer, request changes, full diff, and approve.
7. Integrate exact-revision invalidation with existing correction, cancellation, shutdown, and guarded landing paths.
8. Add integration tests for the complete lifecycle, stale artifact rejection, request-change routing, full-diff routing, and approval guards.
9. Add a runnable example artifact and open the rendered output during verification.
10. Run focused tests, `npm run check`, `nix flake check`, Python checks where touched, and `git diff --check`.

## Testing strategy

- Parser tests: top-level metadata, change metadata, review prompts, diff fences, plain Markdown fallback, malformed metadata, duplicate IDs, invalid paths, invalid line ranges, and oversized content.
- Renderer tests: correct section ordering, escaped content, states, progress, action availability, and no executable injection.
- Lifecycle tests: preflight approval starts walkthrough; preflight failure does not; exact revision mismatch invalidates; cancellation and shutdown clean up; correction reuses or regenerates only where safe.
- Interaction tests: Looks good persists state; Explain creates a bounded reviewer question; Request changes routes feedback and blocks landing; View full diff opens the exact reviewed revision; Approve is rejected while critical sections are unresolved.
- End-to-end fixture: generate a walkthrough from a small fake implementation, render it, exercise all actions, and verify the final guarded decision.

## Non-goals for the first version

- Replacing Plannotator.
- Making the walkthrough reviewer the preflight correctness gate.
- Supporting arbitrary HTML, arbitrary extension-provided widgets, or unrestricted user input.
- Automatically changing code based on a walkthrough comment without returning through the implementation and preflight lifecycle.
- Persisting generated walkthroughs as permanent manual documentation.

## Open decisions

- Whether the first renderer should be a local web surface, a Pi TUI view, or both. -> Let's make it a web surface.
- Whether Plannotator can accept the walkthrough artifact directly or should remain a separate full-diff action. -> it should remain a separate full-diff action.
- The exact Markdown directive grammar and whether metadata should use YAML front matter or fenced directives. -> fenced is fine.
- How much implementation conversation to include in the structured handoff. -> depends on task and the reviewer.
- Whether all sections must be marked Looks good or only critical sections before Approve is enabled. -> all.
