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
latest 200 conversation messages, and managed attachment content. It exposes
three protocol-v5 sockets and one private HTTP socket:

- `$XDG_RUNTIME_DIR/scufris/surface.sock`: registered desktop and synthetic
  surfaces;
- `$XDG_RUNTIME_DIR/scufris/agent.sock`: exactly one local Pi extension; and
- `$XDG_RUNTIME_DIR/scufris/control.sock`: local state diagnostics; and
- `$XDG_RUNTIME_DIR/scufris/content.sock`: private attachment upload, import,
  lookup, download, and HEAD operations.

The runtime directory is mode 0700. Each socket is mode 0600. Set
`SCUFRIS_RUNTIME_DIR` to place all four sockets in another directory for a
coordinated staging stack.

## Typed channels

Each socket has its own inbound and outbound message enum. Every line is one
bounded LF-terminated JSON object with `"v":5`. A wrong version is logged and
the connection closes without a response. Clients show a local message that
asks the user to update the host and surface together.

A surface starts with `surface.hello`. The hello carries a stable ID, diagnostic
name, and complete widget definitions. Registration binds identity to that
connection. A later `surface.message` or `surface.abort` does not repeat the
surface ID. Registering the same ID replaces only the previous generation.

An agent starts with `agent.hello`. A second agent receives `agent.rejected` and
is disconnected. Control supports only `control.hello` and `control.state`.
There is no control watch, abort, debug, event stream, or prompt command.

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
2. current `surface.state`; and
3. `surface.ready`.

The connection becomes eligible for live broadcasts only after all three are
queued. A surface clears its local copy when replay starts. It stores replayed
messages but performs no speech, response animation, or widget calls before
`surface.ready`.

## Prompt ingress and association

The agent channel is the only prompt ingress. The service sends every accepted
surface message as `agent.message`, with the original text and a fresh snapshot
of that surface's registered widget definitions. The Pi extension builds one
self-contained `<scufris_surface_message>` user message and uses
`pi.sendUserMessage()`. It uses `deliverAs: "steer"` while Pi is busy.

The latest accepted surface message selects the response association. A steer
from another surface changes it. The service records an assistant response with
that surface ID and broadcasts it to all surfaces.

## Atomic responses

The agent emits one `agent.response` with mandatory bounded plain `text`,
optional bounded Markdown `details`, and optional bounded `widgets` calls. The
service validates widget names and arguments against the selected surface's
registration before it records and broadcasts the response.

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

Pi lifecycle and agent attention are retained separately. The service computes
one state with this precedence:

```text
failed > blocked > working > starting > idle
```

Surfaces layer local listening, transcription, and speaking presentation over
that state.

## Process ownership

The service starts Pi in RPC mode, reads lifecycle events, cancels extension
dialogs that have no interactive RPC client, and restarts quick failures with a
bound. It does not use Pi RPC to inject prompts or abort work. Those operations
travel only over the typed agent channel.

---

Next: [Desktop companion](desktop.md)
