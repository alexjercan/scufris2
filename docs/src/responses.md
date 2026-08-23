# Spoken responses and private detail

The voice extension owns response shaping in every Scufris package. Its speech
module and Piper tool are present only in voice-capable resources.

Foreground Scufris is a pair-programming companion. It keeps the conversation in the foreground, synthesizes gathered evidence in its own voice, and stops at meaningful decisions. It answers conversation and narrow project questions directly, but delegates work expected to take minutes. Scope and latency control routing, not the use of project tools.

Every final answer has one short plain-prose paragraph. This includes automatic
wake turns and successful workflow actions. After spawning, steering, stopping,
landing, or opening a review, Scufris synthesizes one concise contextual
acknowledgment in its own voice before ending. It does not substitute canned
speech. A failed action instead gets one concise explanation that does not
claim success. Optional Markdown detail is stored in a private sidecar beside
the active Pi session. The transcript then shows one compact command:

```text
/detail 4f8c7a21d3e64b829e93ab10
```

Run that exact command to open the session-owned artifact in Plannotator. Scufris accepts only an opaque artifact ID from the active session. Approval and closure produce one compact transcript row. Actionable feedback returns to Scufris without rendering the feedback in the transcript.

Scufris hides direct response streaming. During a pending workflow
acknowledgment, ordinary assistant text is discarded: only one successfully
executed final-response tool call can produce the acknowledgment. If that turn
settles without success, the workflow gate resets without speaking the rejected
text. Outside that boundary, if a model does not use the structured
final-response tool, Scufris extracts a safe first paragraph and stores the
remainder. The finalized assistant record is the only visible copy. Unsafe
output is stored in full and replaced with a safe sentence. Piper receives only
the validated spoken paragraph. Response entries and private detail artifacts
are created only when the final-response tool executes, not while its assistant
message is being validated. Rejected or preflight-blocked batches therefore
leave no response artifact. Consecutive turns with no new safe response produce
at most one warning until playback succeeds, so a playback failure cannot
re-arm repetitive missing-response warnings.

Run `/scufris-prompt` to create a private prompt inspection artifact without contacting a model provider. The artifact contains the exact assembled prompt and ordered provenance for Pi prompt inputs, context files, active tools and guidelines, loaded skills, the embedded Scufris identity policy, and the final-response policy.

Artifact sidecars use private directory and file modes. They remain beside their owning session. `scufris-artifacts-prune` removes bounded sidecars only after their session file is gone.
