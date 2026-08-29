# Service

`scufris-service` owns the Pi RPC process, canonical user-facing state, and the
latest 200 conversation messages. It exposes protocol v4 on three private Unix
sockets:

- `$XDG_RUNTIME_DIR/scufris/surface.sock`: registered desktop and synthetic
  surfaces;
- `$XDG_RUNTIME_DIR/scufris/agent.sock`: exactly one local Pi extension; and
- `$XDG_RUNTIME_DIR/scufris/control.sock`: local state diagnostics.

The runtime directory is mode 0700. Each socket is mode 0600. Set
`SCUFRIS_RUNTIME_DIR` to place all three sockets in another directory for a
coordinated staging stack.

## Typed channels

Each socket has its own inbound and outbound message enum. Every line is one
bounded LF-terminated JSON object with `"v":4`. A wrong version is logged and
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

The service keeps the latest 200 canonical `surface.message` entries. Every
accepted user message and every final assistant response is broadcast to every
registered surface.

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
