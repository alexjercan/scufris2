# Spoken responses and private detail

Foreground Scufris is a conversational orchestrator. It answers conversation, product decisions, and project work that should take seconds directly. It delegates work expected to take minutes. Scope and latency control routing, not the use of project tools.

Every final answer has one short plain-prose paragraph. Optional Markdown detail is stored in a private sidecar beside the active Pi session. The transcript then shows one compact command:

```text
/detail 4f8c7a21d3e64b829e93ab10
```

Run that exact command to open the session-owned artifact in Plannotator. Scufris accepts only an opaque artifact ID from the active session. Approval and closure produce one compact transcript row. Actionable feedback returns to Scufris without rendering the feedback in the transcript.

Scufris hides direct response streaming. If a model does not use the structured final-response tool, Scufris extracts a safe first paragraph and stores the remainder. The finalized assistant record is the only visible copy. Unsafe output is stored in full and replaced with a safe sentence. Piper receives only the validated spoken paragraph.

Run `/scufris-prompt` to create a private prompt inspection artifact without contacting a model provider. The artifact contains the exact assembled prompt and ordered provenance for Pi prompt inputs, context files, active tools and guidelines, loaded skills, the embedded Scufris orchestration policy, and the final-response policy.

Artifact sidecars use private directory and file modes. They remain beside their owning session. `scufris-artifacts-prune` removes bounded sidecars only after their session file is gone.
