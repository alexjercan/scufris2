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

Quick Review completion, Plannotator review results, and `/detail` feedback
use the same follow-up message channel, bounded and JSON-encoded, each ending
with the instruction to answer with one short final response.

## The acknowledgment gate

Meaningful workflow actions are serialized against everything else:

- `scufris_job_spawn`, `scufris_job_send`, `scufris_job_stop`,
  `scufris_job_land`, and both review-opening tools must each be the only
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

## Quick Review bridge

Quick Review runs as a local Python page server
(`tools/quick-review/quick_review.py`) connected to the extension over a
JSON-lines stdio bridge:

- The extension sends one `init` line with the parsed walkthrough document
  and state. The page replies `ready` with a loopback URL guarded by a random
  path token, then the extension activates the browser.
- Each page action becomes one bridged request. The extension verifies the
  exact base and implementation revisions before and around every action,
  applies the state machine in `walkthrough.ts`, persists state, and answers
  with the updated state.
- Section actions: mark viewed, reopen, explain (the section's review
  prompt), ask (a free question), exact-revision context, and non-blocking
  comments. Terminal actions: approve (requires every section viewed and no
  blocking requests) and request changes (requires an explanation).
- Approval and change requests re-verify revisions, then finish through the
  normal event channel: approval publishes a `quick-review-approved` result;
  a change request steers the job, invalidates the walkthrough, and restores
  the status watcher.

All bridge lines are strictly validated and bounded on both sides. The page
serializes actions, refuses concurrent terminal actions, and shuts down when
the extension closes the review.
