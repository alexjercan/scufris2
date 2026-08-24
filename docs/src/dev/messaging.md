# Messaging

Foreground Scufris coordinates three message flows: worker events into the
conversation, workflow acknowledgments out of it, and the shaped final
response to the user.

## Worker event delivery

The orchestration extension watches each owned status file with filesystem
notifications; it never polls jobs on a timer. A change triggers an `events`
read. Delivery depends on whether the event wakes:

- A waking event is persisted before it is acknowledged. It is sent as a
  `scufris-job-event` custom message with `deliverAs: "followUp"` and
  `triggerTurn: true`, carrying the job, project, event line, event
  identity, and generation. Only then does the extension record the event
  identity and acknowledge the exact event, advancing the durable cursor.
- A quiet `working` event surfaces only as a transient UI notification and
  is acknowledged directly. It leaves no session entry and carries no replay
  guarantee.

Because a persisted waking message precedes its acknowledgment, a crash
between the two steps replays the event; recovery collects already-delivered
event IDs from the session entries and acknowledges them without a second
wake. Terminal and blocked events that were never delivered wake exactly
once after restart.

`/wake` selects the wake mode. In `minimal` (default), `working` events show
only a notification; `blocked`, `done`, and `failed` trigger turns. In `all`,
`working` events also trigger turns. Runtime failures always notify and wake.

Standalone Quick Review completion, Plannotator results, and `/detail`
feedback use the same follow-up message channel, bounded and JSON-encoded, each ending
with the instruction to answer with one short final response.

## The acknowledgment gate

Meaningful workflow actions are serialized against everything else:

- `scufris_job_spawn`, `scufris_job_send`, `scufris_job_stop`,
  `scufris_job_land`, and `scufris_job_plannotator_review` must each be the only
  tool in their tool batch, as must `scufris_final_response`.
- After a successful action, the only permitted follow-up is one
  `scufris_final_response` call. Every other tool is blocked with an
  explanatory reason until the final response completes or the run settles.
- A separate `tool_call` guard blocks foreground bash commands that execute
  `sleep` or `wait` at any position in a pipeline or list. Foreground Scufris
  never waits for workers; filesystem notifications start later turns.

The gate emits its state on the shared `scufris:acknowledgment-state` event so
the response module can suppress plain assistant text while an
acknowledgment is pending.

## Final response shaping

The response module makes `scufris_final_response` the only user-visible
output path:

- Streamed assistant text and text beside tool calls are removed at
  `message_end`.
- A valid final call carries a safe spoken paragraph and optional detail. The
  detail is stored as a private sidecar artifact and the transcript renders
  the paragraph plus a `/detail <id>` command.
- An invalid spoken value is replaced with a fixed safe sentence and the
  rejected content is preserved in the detail artifact.
- A plain text-only turn is split: first paragraph spoken when safe, the
  remainder stored as detail. While a workflow acknowledgment is pending,
  plain text is discarded entirely; only a successful final-response call
  speaks.

Artifacts are created only when the tool executes, never during message
validation, so blocked or rejected batches leave no artifact. Speech plays
only the validated paragraph of the current settled run.

## Standalone Quick Review agent

`scufris_job_quick_review` starts a separate Pi process in RPC mode. That
process disables discovered extensions, skills, templates, themes, and context
files; it loads only `npm:@alexjercan/quick-review@0.1.1` and read-only built-in
tools. The foreground tool returns after RPC accepts `/quick-review`, so Scufris
never waits for walkthrough generation or page questions.

The private adapter relays only `ready` and the validated
`quick-review-outcome` custom message. Approval wakes foreground Scufris with a
bounded summary. Requested changes are first sent to the implementation job in
a new generation, then wake Scufris. Workflow stop and session shutdown signal
only the exact recorded adapter process, which in turn stops its exact Pi child.
