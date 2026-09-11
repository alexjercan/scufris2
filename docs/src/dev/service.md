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
three protocol-v11 sockets and one private HTTP socket:

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
bounded LF-terminated JSON object with `"v":11`. A wrong version is logged and
the connection closes without a response. Clients show a local message that
asks the user to update the host and surface together.

A surface starts with `surface.hello`. The hello carries a stable ID, diagnostic
name, and complete widget definitions. Registration binds identity to that
connection. A later `surface.message` or `surface.abort` does not repeat the
surface ID. Registering the same ID replaces only the previous generation.

An agent starts with `agent.hello`. A second agent receives `agent.rejected` and
is disconnected. Control supports `control.hello`, `control.state`,
`control.wake`, durable `control.briefing` upserts, one bounded
`control.conversation` page, and the three lease verbs below. There is no
control watch, abort, debug, event stream, or prompt command.

## The terminal lease

The service starts and owns a Pi child. An interactive Pi in a terminal is a
second Pi, and two agents on one channel is the thing the channel refuses. The
lease is how the terminal becomes the one agent instead of a rival to it.

`control.lease_acquire` stops and reaps the managed child, then answers
`control.lease` with a generation. It blocks for as long as the stop takes,
because the terminal must not touch the agent channel while the child still
holds the session file. The reply is the permission to connect. A busy agent is
refused with `agent_busy` unless the caller asked to abort the work first, a
held lease is refused with `lease_held`, and a service started without the
lease enabled refuses every acquire with `lease_disabled`.

The lease is the control connection. `agent.hello` must carry the live
generation to be admitted while a lease is held, which fences out the child's
own connection as it closes and a terminal whose lease already ended. With no
lease held, a hello that names a generation is refused too, because nothing
granted it. `control.lease_ping` every five seconds says the holder is still
there; three missed pings end the lease, because a terminal that is stopped
rather than killed keeps an open socket that nobody reads. `control.lease_release`,
or the connection closing, gives the agent back.

`agent.handoff` tells the agent the lease ended and who is next, so a terminal
leaves the channel before the managed child returns to it.

## Session lineage

Pi fixes a session's working directory when the file is created, so the service
cannot hand one file to processes in different directories. It hands over the
lineage instead. An agent declares the file it writes with `agent.session`, and
the next holder is started with `--fork` on that file. Pi copies the branch into
a new file and names the old one as its parent.

A joining agent that continues the lineage, by writing that file or by forking
it, already has the words. Any other session is told what it missed:
`agent.catch_up` carries one bounded page of canonical entries, which the agent
injects as a single undisplayed message. One block of text, not a replayed turn
for every entry.

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

Each open turn has an identifier, and an answer that names it closes exactly
that turn. An answer naming a turn that already closed is recorded rather than
dropped, and leaves the open turn alone. This is what lets a terminal, where the
person also types into the same session, keep a phone's question and a local
question apart.

The terminal is a surface as well as the agent. `agent.turn` submits the words
a person typed there, which are recorded as a user message from the `terminal`
surface and answered by `agent.turn_ack` with the sequence they were recorded
at. Widget calls in an answer to a terminal turn are refused with
`invalid_widgets`: a terminal draws text.

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
response. It then sends `agent.proactive_settled` for the same turn. If Pi
settles before the queued custom message reaches `message_end`, that final
boundary sends settlement for the exact queued ID so the host can release and
retry it. The extension retains queued IDs across socket reconnects and ignores
a redelivery of an ID Pi already holds, so reconnect cannot duplicate a model
turn. These markers and the response use one ordered agent socket. A different
queued follow-up cannot consume or overwrite the correlation. The service
accepts only the active event ID.

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
through that backoff or a surface opens a user turn. After three starts, the
service-owned circuit opens before another turn for a date/profile pair already
seen in that sequence. This stops a producer that keeps minting generations for
one logical run while allowing a queued backlog of distinct days and profiles
to drain. Every retained queued row becomes `failed`, keeps its measured
summary, remains visible with recovery instructions, and cannot be dismissed.
A surface user turn returns stopped rows to `pending` and resets the circuit;
restarting the service performs the same recovery.

Collection and delivery never overwrite each other. The filesystem helper owns
`collecting`, `collected`, and `failed`; the service owns `pending`,
`in_progress`, `failed`, and `delivered`.

After delivery, a successful row leaves surface presentation. A delivered row
with `collection == failed`, or a measured partial row with
`collection == collected && failed > 0`, remains until `briefing.dismiss`. A
circuit-stopped delivery is separate and nondismissible. Only a registered
surface can send a dismissal request. The
opaque ID must name a retained terminal, delivered generation. The service
stores dismissal atomically and then broadcasts the new whole list. A repeated
dismissal succeeds as a no-op; unknown and nondismissible IDs receive bounded
rejections.

Dismissal does not acknowledge delivery and does not remove the canonical
answer, run artifacts, or audit row. Briefing state format 2 retains up to 64
audit rows plus their bounded dismissed-ID set and reads format 1 as no
dismissals. Stores written under format 2's original 128-row bound compact on
upgrade instead of being rejected. Active and undismissed delivered attention
rows are protected from
audit eviction. The oldest delivered success, dismissed attention row, or
circuit-stopped row yields first. Evicting a stopped row also removes its wake,
so a runaway producer cannot fill the store and refuse later legitimate
ingress.

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
holds `failed` until a user turn or service restart returns it to `pending`.
Thus the tray stays red while explicit recovery is still required, not only
while a process happens to run. A failed job is named before the generic
briefing stop when both need attention.

Surfaces layer local listening, transcription, and speaking presentation over
that state.

`surface.state` also says which process is the agent. The field is absent for
the managed child, so a host that never hands the agent over sends what it
always did, and `terminal` while a lease is held.

## Process ownership

The service starts Pi in RPC mode, reads lifecycle events, cancels extension
dialogs that have no interactive RPC client, and restarts quick failures with a
bound. It does not use Pi RPC to inject prompts or abort work. Those operations
travel only over the typed agent channel.

While a terminal holds the lease there is no managed child. The service starts
one again when the lease ends, forking the file the terminal wrote so the
conversation continues rather than starting over.

---

Next: [Desktop companion](desktop.md)
