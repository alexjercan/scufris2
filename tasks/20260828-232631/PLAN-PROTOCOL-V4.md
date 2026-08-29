# Protocol v4 registered-surface plan

Status: approved for protocol v4 core implementation.

This revision supersedes the presence-lease design and the earlier mixed-role
v4 sketches. It uses registered surfaces, separate typed channels, bounded
conversation replay, and one atomic agent response.

## Goal

Allow one Scufris host conversation to have several simultaneous surfaces:

- the desktop companion on the host;
- the same desktop companion on a compatible Linux laptop; and
- a personal iOS surface.

One host owns Pi, the session, and `scufris-service`. Surfaces are replicated
clients of that authoritative host. Every surface displays the same bounded
conversation. The surface associated with the latest user message performs
local presentation such as speech and widgets.

## Server model

The architecture resembles a small game server:

- the service owns canonical state and the recent conversation;
- surfaces register stable identities and widget schemas;
- `broadcast` sends canonical updates to every registered surface;
- `send` sends one protocol message to one connection; and
- reconnection replaces local state with an authoritative replay.

There is no presence lease, claim/release protocol, recency stack, watcher role,
or speech route in the service.

## Typed channels

Protocol v4 has three separate typed channels rather than one role-selected
message enum:

```text
surface:
    many connections
    remote-capable
    conversation and presentation

agent:
    exactly one connection
    local-only
    Pi integration and final responses

control:
    minimal local diagnostic requests
```

Each channel has its own inbound and outbound decoder. A surface message cannot
be decoded as an agent request, and a service-to-surface message cannot be sent
back as a surface request.

The channels use three physical Unix sockets:

```text
surface.sock  - many local surfaces; the TCP gateway proxies only here
agent.sock    - the one local agent
control.sock  - local scufris-ctl
```

Physical separation ensures that a byte-only TCP gateway cannot let a remote
client claim the agent or control channel.

## Delivery helpers

The service needs only `send` and `broadcast`:

```rust
fn send<C: Channel>(connection: &Connection<C>, message: C::Outbound);

fn broadcast(message: SurfaceMessage) {
    for surface in registered_surfaces() {
        send(&surface, message.clone());
    }
}
```

Both use the same bounded LF-delimited JSON v4 framing. `broadcast` is ordinary
fan-out implemented with `send`. Routing policy remains at the call site.

## Surface identity

A surface installation generates one bounded opaque ID and sends it in
`surface.hello`:

```json
{
  "v":4,
  "type":"surface.hello",
  "surface":{
    "id":"surface-550e8400-e29b-41d4-a716-446655440000",
    "name":"Alex's iPhone",
    "widgets":[]
  }
}
```

Rules:

1. The desktop persists its ID in its state directory.
2. iOS persists its ID in Keychain.
3. The name is diagnostic and need not be unique.
4. Registration binds the connection to the surface ID.
5. Later inbound requests do not repeat the sender's surface ID.
6. Registering the same ID replaces its old connection.
7. A connection generation prevents a late close from removing its replacement.
8. The client sends the complete hello again after reconnecting.
9. Surface identity is routing identity, not authentication.

## Surface protocol

### Surface to service

| Type | Body | Meaning |
| --- | --- | --- |
| `surface.hello` | `surface` | Register this connection |
| `surface.message` | `id`, `text` | Send a user message or steer |
| `surface.abort` | `id` | Abort active work |

A surface cannot send speech, state, widgets, agent responses, or control
requests on this channel.

### Service to surface

| Type | Body | Meaning |
| --- | --- | --- |
| `surface.message` | `role`, `surface`, `text`, `details?`, `widgets?` | Canonical conversation entry |
| `surface.message_ack` | `id` | The submitted message was accepted |
| `surface.aborted` | `id` | The abort was accepted |
| `surface.state` | `state`, `detail` | Current user-facing agent state |
| `surface.ready` | `surface` | Registration and replay are complete |
| `surface.rejected` | `id?`, `operation`, `code`, `detail` | A request was refused |

There is no `surface.speak`, `surface.stop_speech`, standalone
`surface.widget`, widget result, or conversation-window effect.

## Canonical conversation

Every accepted user message and every final agent response becomes one bounded
conversation entry. The service stores the latest N entries and broadcasts each
live entry to every registered surface.

User entry:

```json
{
  "v":4,
  "type":"surface.message",
  "role":"user",
  "surface":"phone-1",
  "text":"Run the tests."
}
```

Assistant entry:

```json
{
  "v":4,
  "type":"surface.message",
  "role":"assistant",
  "surface":"phone-1",
  "text":"All tests passed.",
  "details":"## Verification\n\n84 tests passed.",
  "widgets":[
    {
      "id":"widget-call-1",
      "name":"test-summary",
      "arguments":{"passed":84,"failed":0}
    }
  ]
}
```

`role` follows LLM conversation naming. `surface` associates both sides of an
interaction with one registered surface. All clients store the same entry.

The service keeps a bounded human-facing projection, not Pi's complete internal
context. Thinking, tool results, compaction entries, and other private session
data do not belong in the surface conversation.

## Latest-sender presentation

The latest accepted `surface.message` selects the associated surface for the
current response. A steer from another surface updates that selection.

When the final response arrives, the service adds the selected surface ID and
broadcasts one assistant `surface.message`. There is no separate direct speech
message.

For a live assistant entry, a surface may perform local presentation only when:

```text
message.surface == its registered surface ID
```

Local presentation includes:

- speaking `text` when local TTS is enabled;
- stopping prior local speech when another live message arrives;
- animating its response state; and
- rendering the attached widget calls it supports.

Other surfaces update their conversation silently. Speech configuration, mute,
TTS endpoint selection, playback, and interruption are entirely surface-local.
Speech is not a registered capability and the service does not know whether a
surface speaks.

## Replay and readiness

A reconnecting surface does not send a cursor or merge local history. It clears
its local conversation and waits on the loading screen.

After `surface.hello`, the service queues under one ordering boundary:

1. the latest N `surface.message` entries;
2. the current `surface.state`; and
3. `surface.ready`.

Example final message:

```json
{"v":4,"type":"surface.ready","surface":"phone-1"}
```

Messages received before `surface.ready` are replay. The surface stores them but
does not speak, animate a new response, or execute widgets. Messages after
`surface.ready` are live.

The service makes the connection eligible for broadcasts only after replay and
ready have been queued under the same lock. A live message cannot overtake ready
or fall into a registration gap.

Both the service and every surface retain at most N conversation messages. The
initial value remains the current 200-entry limit unless a focused UI test
justifies another value.

## Agent protocol

The service permits exactly one agent connection.

### Agent to service

| Type | Body | Meaning |
| --- | --- | --- |
| `agent.hello` | none | Open the one agent channel |
| `agent.response` | `text`, `details?`, `widgets?` | Emit one atomic final response |
| `agent.state` | `state`, `detail` | Report attention, blocked, failed, or clear |

### Service to agent

| Type | Body | Meaning |
| --- | --- | --- |
| `agent.ready` | none | Accept the agent connection |
| `agent.message` | `id`, `text`, `widgets` | Deliver a user message or steer |
| `agent.abort` | `id` | Abort active work |
| `agent.rejected` | `code`, `detail` | Refuse a second or invalid agent |

The agent channel is the only prompt ingress. On `agent.message`, the Pi
extension constructs one self-contained user message and calls
`pi.sendUserMessage()`, with `deliverAs: "steer"` while Pi is busy. The old RPC
prompt road, side-channel context queue, and dynamic widget activation are not
preserved.

## Per-message widget definitions

Every `agent.message` carries a fresh server-owned snapshot of the selected
surface's widget schemas:

```json
{
  "v":4,
  "type":"agent.message",
  "id":"message-14",
  "text":"Run the tests.",
  "widgets":[
    {
      "name":"test-summary",
      "description":"Display passed and failed test counts.",
      "input_schema":{
        "type":"object",
        "properties":{
          "passed":{"type":"integer"},
          "failed":{"type":"integer"}
        },
        "required":["passed","failed"],
        "additionalProperties":false
      }
    }
  ]
}
```

The extension serializes that message deterministically for Pi:

```text
<scufris_surface_message>
<widgets>
[
  {
    "name": "test-summary",
    "description": "Display passed and failed test counts.",
    "input_schema": {
      "type": "object",
      "properties": {
        "passed": { "type": "integer" },
        "failed": { "type": "integer" }
      },
      "required": ["passed", "failed"],
      "additionalProperties": false
    }
  }
]
</widgets>
<user_message>
"Run the tests."
</user_message>
</scufris_surface_message>
```

`widgets` is used rather than `tools` so the model does not confuse
best-effort presentation calls with native Pi tools. Widget definitions retain
the familiar `name`, `description`, and `input_schema` shape. The user message
is a JSON string, and the deterministic encoder escapes XML delimiters.

The surface sends widget schemas only in `surface.hello`. The service obtains
them from the connection-bound registration for every new message and steer.
There is no separate context acknowledgement or `before_agent_start` context
queue.

## Atomic final response

The agent always responds in one format:

```json
{
  "v":4,
  "type":"agent.response",
  "text":"All tests passed.",
  "details":"## Verification\n\n84 tests passed.",
  "widgets":[
    {
      "id":"widget-call-1",
      "name":"test-summary",
      "arguments":{"passed":84,"failed":0}
    }
  ]
}
```

- `text` is mandatory bounded plain prose and is safe for local speech.
- `details` is optional bounded Markdown. Surfaces display it but do not speak
  it.
- `widgets` is an optional bounded list of calls shaped like LLM tool calls.

The service validates widget names and arguments against the selected surface's
registered schemas, adds `role` and `surface`, records the resulting canonical
entry, and broadcasts it.

Widgets are synchronous with the final response: they exist only in that atomic
response. There are no later widget open, update, close, result, or
acknowledgement messages.

Consequences:

- widget rendering is best-effort presentation;
- the agent never waits for rendering;
- widget calls should be idempotent;
- rendering failures remain local in logs or surface UI; and
- replay stores widget metadata but never executes historical calls.

This response shape can replace ordinary `/detail <id>` retrieval. Details are
part of the bounded shared conversation rather than private artifacts hidden
behind a command.

## State

Pi lifecycle and agent attention have different internal sources, but surfaces
receive one computed message:

```json
{
  "v":4,
  "type":"surface.state",
  "state":"blocked",
  "detail":"The workflow needs review."
}
```

User-facing server states are:

```text
failed
blocked
working
starting
idle
```

The service retains lifecycle plus internal attention causes and applies this
severity-first precedence:

```text
failed > blocked > working > starting > idle
```

Blocked work therefore remains visible while Pi performs unrelated work.
Surfaces layer local recording, transcription, and speaking visuals on top
without changing or reporting server state.

## Minimal control protocol

There is no watcher. `journalctl` provides event observation and `systemctl`
provides process management.

Initial control protocol:

```text
control -> hello, state
service -> ready, state, rejected
```

| Direction | Type | Body |
| --- | --- | --- |
| control to service | `control.hello` | none |
| control to service | `control.state` | `id` |
| service to control | `control.ready` | none |
| service to control | `control.state` | `id`, `state`, `detail` |
| service to control | `control.rejected` | `id`, `code`, `detail` |

Abort, watch, event streaming, debug, and session hijack are not part of the
initial v4 control channel. A safe terminal hijack can be reconsidered later as
a lease that stops only managed Pi, runs `pi --resume`, and lets the service
resume Pi when the helper connection closes.

## Compatibility policy

Protocol v4 replaces v3 outright. There is no legacy `service.sock` listener,
v3 parser, conversion, fallback, dual protocol, or compatibility shim. Host,
agent, control, and surfaces are updated as one staging deployment with:

```text
nix run .#staging -- up
```

A mixed v3/v4 deployment is unsupported. The missing or wrong endpoint fails
rather than activating old behavior.

Every endpoint accepts only its exact protocol version. A wrong `v` is logged
and the connection is closed without a response. Clients turn handshake EOF or
closure into a local message telling the user to update the host and surface
together. There is no cross-version diagnostic envelope.

## Remote transport

Local channels use private mode-0600 Unix sockets. The optional gateway:

- binds only a configured Tailscale address and port;
- proxies each TCP stream only to the surface channel;
- copies bytes without parsing Scufris messages;
- has bounded connections and correct half-close behavior;
- is disabled by default; and
- is exposed only on `tailscale0` under a tailnet grant.

The service does not expose public TCP. Router forwarding and Tailscale Funnel
remain disabled. Tailnet membership is the first personal deployment's
transport authentication boundary.

## Desktop reuse and iOS

A compatible Linux/X11 laptop runs the same desktop package with a configured
surface endpoint rather than a remote mode:

```text
host:   unix:///run/user/1000/scufris/surface.sock
laptop: tcp://nixos.tailnet:PORT
```

The first iOS product is foreground-only and text-first:

- `NWConnection` over Tailscale;
- persistent surface channel;
- stable Keychain surface ID;
- loading until replay ends with `surface.ready`;
- bounded conversation, typed message, abort, state, and reconnect; and
- no native widget implementation until one has tested behavior.

Hold-to-talk and local speech follow the text surface. STT and TTS endpoints
belong to a separate inference repository and do not block protocol v4.

## Implementation batches

### 1. Implement channel and envelope boundaries

- add `surface.sock`, `agent.sock`, and `control.sock`;
- reject every non-exact version by logging and closing without a response;
- define separate Rust and TypeScript channel enums;
- bound surface IDs, names, text, details, widget lists, schemas, and arguments;
- test direction and channel rejection; and
- prove self-contained `pi.sendUserMessage()` prompt and steer delivery.

### 2. Implement registered surface replay

- retain multiple surface connections;
- bind stable IDs and replacement generations;
- keep a canonical N-entry conversation ring;
- queue replay, state, and ready atomically;
- broadcast live user and assistant entries; and
- add two-surface, replacement, slow-reader, and reconnect tests.

### 3. Implement latest-sender responses

- associate accepted user messages with their bound surface;
- update the association on a cross-surface steer;
- attach the selected ID to assistant entries;
- deliver fresh widget context for every agent message; and
- prove that every surface stores the same conversation while only the matching
  live surface performs local presentation.

### 4. Replace response and widget protocols

- emit one `agent.response` with text, optional details, and widget calls;
- remove separate said and speak reports;
- remove widget command results and asynchronous updates;
- validate widget calls against registered schemas;
- include details and widget metadata in replay; and
- remove ordinary `/detail` retrieval only with replacement coverage.

### 5. Add the optional TCP gateway

- proxy only the surface channel;
- test byte fidelity, connection bounds, shutdown, and half-close;
- deploy disabled by default with a Tailscale-only bind and firewall; and
- prove a real remote surface.

### 6. Add products

1. Compatible remote laptop desktop.
2. iOS text surface.
3. External STT/TTS inference service.
4. iOS hold-to-talk and local speech.

## Required verification

Before remote products depend on v4, prove:

- several different surface IDs remain connected;
- registering the same ID replaces only its old generation;
- a surface cannot send agent, control, or outbound surface messages;
- every surface receives identical live conversation entries;
- registration replays exactly the latest N entries and state before ready;
- no replay message triggers speech, animation, or widget execution;
- a live assistant message names the latest submitting surface;
- a cross-surface steer updates that association;
- optional details are identical on every surface and never spoken;
- widget calls are emitted only in the atomic final response;
- only the associated live surface executes widget calls;
- widget rendering produces no protocol result;
- a slow surface is removed without affecting other surfaces;
- a late old connection cannot remove its replacement;
- only one agent connection is accepted;
- the minimal control channel cannot act as a surface or agent; and
- every non-exact version is logged and disconnected without response;
- each client renders a local update-together message after handshake failure.

## Out of scope

- presence leases and active-surface controls;
- server-owned speech and audio transport;
- asynchronous widget updates or widget results;
- agent-controlled conversation windows;
- control watch, abort, or debug in the first v4 protocol;
- multiple users, agents, conversations, or authoritative hosts;
- public endpoints, Funnel, relays, accounts, or push notifications;
- widgets implemented on iOS before the text surface works; and
- model hosting inside this repository.

## Deferred product input

Before implementing laptop deployment, record its operating system, display
stack, and architecture. This does not block protocol v4, local desktop, or the
synthetic multi-surface proof.
