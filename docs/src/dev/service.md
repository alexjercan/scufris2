# Background service

[Previous: Test a change](testing.md)

```text
surfaces -> surface.sock -> SERVICE -> agent.sock -> Pi
                              |
terminal -> control.sock ------+
                              |
local HTTP -> content.sock ----+
```

`scufris-service` owns the Pi RPC process, canonical user-facing state, the
latest 200 conversation messages, a durable scheduled-briefing inbox, and
managed attachment content. It exposes
three protocol-v10 sockets and one private HTTP socket:

- `$XDG_RUNTIME_DIR/scufris/surface.sock`: registered desktop and synthetic
  surfaces;
- `$XDG_RUNTIME_DIR/scufris/agent.sock`: exactly one local Pi extension; and
- `$XDG_RUNTIME_DIR/scufris/control.sock`: local state diagnostics and the
  unprompted wake; and
- `$XDG_RUNTIME_DIR/scufris/content.sock`: private attachment upload, import,
  lookup, download, and HEAD operations.

The runtime directory is mode 0700. Each socket is mode 0600. Set
`SCUFRIS_RUNTIME_DIR` to place all four sockets in another directory for a
coordinated staging stack.

## Typed channels

Each socket has its own inbound and outbound message enum. Every line is one
bounded LF-terminated JSON object with `"v":10`. A wrong version is logged and
the connection closes without a response. Clients show a local message that
asks the user to update the host and surface together.

A surface starts with `surface.hello`. The hello carries a stable ID, diagnostic
name, and complete widget definitions. Registration binds identity to that
connection. A later `surface.message` or `surface.abort` does not repeat the
surface ID. Registering the same ID replaces only the previous generation.

An agent starts with `agent.hello`. A second agent receives `agent.rejected` and
is disconnected. Control supports `control.hello`, `control.state`,
`control.wake`, and durable `control.briefing` upserts. There is no control
watch, abort, debug, event stream, or prompt command.

## Replay and broadcast

The service keeps the latest 200 canonical `surface.message` entries. It
atomically snapshots them at `$XDG_DATA_HOME/scufris/conversation.json` before
each live broadcast. The file is mode 0600 in a mode-0700 directory and has an
explicit format version. A completed snapshot therefore survives a service
restart or Home Manager switch independently of Pi's session JSONL. An I/O
error never exposes a partial snapshot: the service logs it, keeps the current
in-memory replay, and retries the complete snapshot on the next message.

A malformed snapshot is moved to `conversation.json.corrupt`; a snapshot from
an unsupported format version is moved to `conversation.json.incompatible`.
Neither prevents startup. Exact repeated internal sequence records are
collapsed during recovery, but equal messages with distinct sequence records
remain distinct conversation turns. Only the latest 200 restored entries are
retained and written back in canonical form.

Every accepted user message and every final assistant response is broadcast to
every registered surface.

Registration queues these under one lock:

1. retained messages;
2. current `surface.state`;
3. current `surface.jobs`;
4. durable `surface.briefings`; and
5. `surface.ready`.

The connection becomes eligible for live broadcasts only after all five are
queued. A surface clears its local copy when replay starts. It stores replayed
messages but performs no speech, response animation, or widget calls before
`surface.ready`.

## Prompt ingress and association

The agent channel is the only prompt ingress, and a surface message is the only
prompt. The service sends every accepted surface message as `agent.message`,
with the original text and a fresh snapshot of that surface's registered widget
definitions. The Pi extension builds one
self-contained `<scufris_surface_message>` user message and uses
`pi.sendUserMessage()`. It uses `deliverAs: "steer"` while Pi is busy.

The latest accepted surface message selects the response association. A steer
from another surface changes it. The service records an assistant response with
that surface ID and broadcasts it to all surfaces.

The association belongs to one turn, not to the session. It opens on an
accepted surface message and closes when that turn ends: the answer that is
recorded, or an accepted abort. A refusal is not an answer and leaves it open,
so the agent may correct the response and still reach the owner. With no turn
open the next answer is `unprompted`, and so is an answer whose owner
disconnected before it arrived, widgets stripped. An answer is never refused
for want of a surface to attribute it to.

## Durable briefing ingress

`control.briefing` upserts one generation-fenced `BriefingRow` and may include
one terminal `BriefingWake`. The service writes the complete row and wake queue
to `$XDG_DATA_HOME/scufris/briefings.json` before returning
`control.briefing_ack`. The update is accepted with no agent or surface
connected. The service broadcasts the complete presentation-relevant
`surface.briefings` list, so collection progress is quiet state and never a
model turn.

A terminal wake has a stable event ID. The service queues it once, waits for Pi
to be idle and for no user turn to own the response association, writes
`in_progress`, then sends `agent.wake` with that event ID as `proactive_id`.
While this one proactive slot is reserved, surface submissions receive
`no_free_slot` and remain in their composer. They are never steered into the
briefing.

The agent extension stores `proactive_id` on the exact custom message that Pi
queues. It captures the ID only when Pi delivers that message, sends an
`agent.proactive_started` marker, and adds the ID only to that turn's atomic
response. It then sends `agent.proactive_settled` for the same turn. These
markers and the response use one ordered agent socket. A different queued
follow-up cannot consume or overwrite the correlation, and its generic
lifecycle event cannot retry a proactive message that is still queued in Pi.
The service accepts only the active event ID.

The service atomically records the assistant message and delivery ID in
canonical conversation replay, then marks the briefing `delivered` and removes
its queued wake. It broadcasts only after both writes succeed. A failed write
keeps the correlated response retryable. A restart resets an unrecorded
`in_progress` item to pending. If replay was written but the second snapshot was
not, startup finds the delivery ID in replay and closes the inbox before an
agent can connect. Duplicate terminal upserts and response retries therefore do
not produce a second visible answer.

Consecutive proactive turns use exponential backoff from 2 seconds, bounded at
30 seconds, between dispatches. The count resets when the queue stays empty
through that backoff or a surface opens a user turn. Three consecutive
proactive turn starts open a service-owned circuit before a fourth can start.
Every retained queued row becomes `failed`, remains
visible with restart instructions, and cannot be dismissed. Restarting the
service is the explicit recovery: startup returns those durable wakes to
`pending`.

Collection and delivery never overwrite each other. The filesystem helper owns
`collecting`, `collected`, and `failed`; the service owns `pending`,
`in_progress`, `failed`, and `delivered`.

After delivery, a successful row leaves surface presentation. A failed row, or
a measured partial row with `collection == collected && failed > 0`, remains
until `briefing.dismiss`. Only a registered surface can send that request. The
opaque ID must name a retained terminal, delivered generation. The service
stores dismissal atomically and then broadcasts the new whole list. A repeated
dismissal succeeds as a no-op; unknown and nondismissible IDs receive bounded
rejections.

Dismissal does not acknowledge delivery and does not remove the canonical
answer, run artifacts, or audit row. Briefing state format 2 retains up to 128
audit rows plus their bounded dismissed-ID set and reads format 1 as no
dismissals. Active and undismissed attention rows are protected from audit
eviction. The oldest delivered success or dismissed attention row yields
first.

## Unprompted wake ingress

`control.wake` is the volatile way a process outside the agent reaches the
foreground with words. Durable scheduled briefings use `control.briefing`
instead. Other out-of-process events send a wake over `control.sock`; the
service forwards it to
the agent connection as `agent.wake`, and the Pi extension delivers it with
`pi.sendMessage()` using `deliverAs: "followUp"` and `triggerTurn: true` under
the `custom_type` the caller named, so an existing wake handler keeps working.

A wake is not a user turn:

- it is never recorded in the canonical conversation;
- it is never echoed to a surface; and
- it never sets or changes the response association. It neither opens a turn
  nor closes one, so an answer that closes an owner's open turn still belongs
  to that surface, and an answer with no turn open is recorded against
  `unprompted` however recently a surface spoke.

Wake text is bounded like every other text, and optional wake details are one
bounded JSON object. With no agent connected the wake is refused with
`agent_unavailable` and a detail, never dropped, so the caller knows its own
durable state is the fallback. The verb exists on the control socket only: the
remote surface gateway speaks the surface channel, which cannot express one.

`scufris-ctl wake "<text>" [--custom-type <type>] [--details <json>]` sends one
and exits non-zero when it did not land. The default custom type is
`scufris-wake`.

## Atomic responses

The agent emits one `agent.response` with mandatory bounded plain `text`, an
optional proactive correlation ID, optional bounded Markdown `details`, and
optional bounded `widgets` calls. The
service validates widget names and arguments against the selected surface's
registration before it records and broadcasts the response. An answer with no
selected surface carries no widget calls: a widget belongs to the surface that
asked, so they are dropped rather than validated.

Widget calls are synchronous response metadata. Only the associated live
surface executes them. Replay stores but never executes them. Rendering is
best-effort and produces no protocol acknowledgement or result.

Speech is also surface-local. A ready surface may speak only a live assistant
message whose `surface` equals its own ID. Details are displayed but never
spoken. The service has no speech message, capability, route, audio state, or
mute state.

## Attachment ownership

Attachment bytes and metadata live under
`$XDG_DATA_HOME/scufris/attachments`. Objects and metadata files are mode 0600;
their directories are mode 0700. IDs contain 192 random bits and do not encode
a path.

The private HTTP API accepts bounded raw uploads at `POST /attachments`, local
regular-file imports at `POST /attachments/import`, and reads at
`GET|HEAD /attachments/{id}`. Reads advertise byte ranges and GET accepts one
bounded byte range, including open-ended and suffix forms. It is available only
through `content.sock`. Import rejects relative paths, final-component
symlinks, directories, devices, FIFOs, empty files, and files over 16 MiB. The
remote gateway never forwards the import operation. The orchestrator's
`store_attachment` tool is the only model-facing importer; it submits an
absolute path and returns the service-owned opaque ID.

The store holds at most 512 objects and 256 MiB. Unreferenced uploads expire
after 24 hours. Referenced objects survive for 30 days and canonical replay.
Startup removes expired data, incomplete temporary files, and orphaned objects.

A surface or agent sends IDs only. Before accepting a message, the service
resolves each ID to its immutable descriptor and marks it referenced. Missing,
expired, or invented IDs receive `attachments_unavailable`; they never enter
the canonical conversation.

## State

Pi lifecycle and the job rows are retained separately. The service computes one
state with this precedence:

```text
failed > blocked > working > starting > idle
```

`failed` and `blocked` are folded from durable rows rather than sent as their
own field. A failed job holds until it is filed. A circuit-stopped briefing
holds `failed` until the service restart that returns it to `pending`. Thus the
tray stays red while explicit recovery is still required, not only while a
process happens to run.

Surfaces layer local listening, transcription, and speaking presentation over
that state.

## Process ownership

The service starts Pi in RPC mode, reads lifecycle events, cancels extension
dialogs that have no interactive RPC client, and restarts quick failures with a
bound. It does not use Pi RPC to inject prompts or abort work. Those operations
travel only over the typed agent channel.

---

Next: [Desktop companion](desktop.md)
