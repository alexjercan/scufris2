# Spike registered remote laptop and iOS surfaces

- STATUS: CLOSED
- PRIORITY: 65
- TAGS: architecture, spike, remote

## Ask

Prepare a reviewed implementation plan for:

- several simultaneous Scufris surfaces sharing one host conversation;
- a compatible laptop running the desktop companion remotely; and
- a personal iOS app with bounded conversation, typed input, and later voice.

The design is approved. The next phase implements protocol v4 through the local
desktop and synthetic multi-surface proof in this task. The gateway, remote
laptop, iOS product, and inference repository remain later batches.

## Artifacts

- Current protocol and implementation plan: `PLAN-PROTOCOL-V4.md`
- New-session protocol implementation prompt: `HANDOFF.md`
- Standalone AI tools API repository prompt: `HANDOFF-AI-TOOLS-API.md`
- Accepted interactive iOS design: `ios-app-design.html`
- Earlier settled design: `tasks/20260828-170154/`

The current plan supersedes the presence-lease, SSH transport, mixed-role v4,
and separate speech-message sketches retained in Git history.

## Current project state

- `v0.5.0` is tagged and deployed.
- The repository uses `agent/`, `host/`, `surfaces/`, and `shared/` boundaries.
- Protocol v3 still permits only one frontend and uses one mixed-role socket.
- The service keeps a bounded 200-entry human-facing transcript ring.
- The private service socket remains mode 0600.
- Tailscale is deployed on the NixOS host and iPhone.
- The iPhone has reached the host through Tailscale and public-key SSH.
- No compatible laptop is currently visible in the tailnet.

## Settled v4 direction

### One authoritative host

The NixOS host owns Pi, its session, the service, canonical state, and the
bounded conversation. A desktop, laptop, or phone is a replicated surface, not
another agent or conversation.

### Registered surfaces

Every installation generates a stable opaque ID and registers it once in
`surface.hello` with a diagnostic name and widget schemas. Registration binds
the connection. Later requests do not repeat sender identity.

Registering the same ID replaces its old connection. A connection generation
prevents the old connection's late close from removing its replacement.

### Broadcast and replay

The service has ordinary `send` and `broadcast` helpers. `broadcast` calls
`send` for every registered surface.

Every accepted user message and final agent response becomes one canonical
`surface.message` with:

- LLM-style `role`;
- associated surface ID;
- mandatory plain `text`;
- optional Markdown `details`; and
- optional LLM-shaped `widgets` calls for assistant responses.

Every live message is broadcast to every surface. Each service and UI retains
only the latest N entries, initially the existing limit of 200.

After `surface.hello`, the service sends the latest N messages and current state,
then sends `surface.ready`. Until ready, the client stays on its loading screen,
replaces its local conversation, and performs no presentation effects. It does
not send a cursor or merge local history.

### Latest-sender local presentation

The latest accepted surface message associates the response with that surface.
A steer from another surface updates the association.

Every surface stores the same assistant message. Only the matching live surface
may speak the plain text, animate the response, or render attached widgets:

```text
message.surface == local registered surface ID
```

Speech is a frontend feature. The service has no speak message, speech
capability, audio path, mute state, or TTS routing.

### Atomic agent response

The agent emits exactly one final shape:

```text
agent.response { text, details?, widgets? }
```

`text` is bounded plain prose. `details` is bounded Markdown. `widgets` is a
bounded list of calls shaped like LLM tool calls.

The service adds the assistant role and latest surface ID, records the entry,
and broadcasts it. Details become part of shared replay and can replace ordinary
`/detail <id>` retrieval.

Widgets are synchronous with the final response. There are no standalone open,
update, close, result, or acknowledgement messages. Rendering is best-effort,
only the associated live surface executes it, and replay never executes it.

### Self-contained Pi messages

A surface sends widget schemas only in `surface.hello`. The service puts a fresh
snapshot of the selected surface's schemas in every `agent.message`, including a
cross-surface steer.

The agent channel is the only prompt ingress. The Pi extension encodes `text`
and widget definitions together inside one `<scufris_surface_message>` block,
using `<widgets>` with LLM-shaped `name`, `description`, and `input_schema`
fields. It then calls `pi.sendUserMessage()`, or uses `deliverAs: "steer"` while
Pi is busy. There is no RPC prompt road, context queue, context acknowledgement,
or dynamic widget activation.

### Typed channels

Protocol v4 separates surface, agent, and control message enums:

```text
surface:
    -> hello, message, abort
    <- message, message_ack, aborted, state, ready, rejected

agent:
    -> hello, response, state
    <- ready, message, abort, rejected

control:
    -> hello, state
    <- ready, state, rejected
```

There is no watcher. Control watch, abort, events, debug, and session hijack are
outside the first v4 protocol. Use `journalctl` and `systemctl` for routine
observation and process management.

Exactly one agent connection is accepted. Multiple surface connections are
retained. The control channel remains local and minimal.

### Server state precedence

The service broadcasts one user-facing state with severity-first precedence:

```text
failed > blocked > working > starting > idle
```

Blocked work remains visible during unrelated Pi work. Listening, transcribing,
and speaking remain surface-local visual states and do not change server state.

### No legacy compatibility

Protocol v4 replaces v3 outright. There is no legacy `service.sock` listener,
v3 parser, conversion, fallback, dual protocol, or compatibility shim. The host
and clients are tested and deployed together with `nix run .#staging -- up`.
Mixed v3/v4 installations are unsupported and fail rather than activating old
behavior.

### Private remote transport

Local channels use three mode-0600 Unix sockets:

```text
surface.sock
agent.sock
control.sock
```

An optional byte-only TCP gateway proxies only `surface.sock`, binds a configured
Tailscale address, and is disabled by default. NixOS exposes its port only on
`tailscale0` under a tailnet grant. Router forwarding, Funnel, and public
endpoints remain disabled.

### Products

A compatible Linux/X11 laptop can run the same desktop package with a configured
surface endpoint rather than a remote flag.

The first iOS product is foreground-only and text-first. It uses `NWConnection`
over Tailscale, a Keychain surface ID, bounded replay, loading until ready,
state, typed messages, abort, and reconnect. Hold-to-talk and local speech
follow.

STT and TTS hosting belongs in a separate inference repository and does not
block protocol v4.

## Evidence

- The service protocol is bounded LF-delimited JSON and can be proxied as an
  opaque byte stream.
- The existing service already separates retained transcript from non-retained
  speech and keeps a 200-entry ring.
- The current response extension already produces mandatory plain prose plus
  optional Markdown detail.
- The current widget protocol is asynchronous and result-bearing, so replacing
  it requires explicit integration coverage rather than a wire-only rename.
- Citadel 0.12.1 did not provide the required high-level binary-clean streaming
  stdin for an iOS SSH transport.
- GitHub-hosted macOS runners provide Xcode, simulators, signing, and TestFlight
  tooling unavailable on NixOS.
- The real vendored orb renders in `ios-app-design.html`.

## Protocol v4 core implementation

Implemented directly on `master` from the approved `PLAN-PROTOCOL-V4.md`.

Decisions applied:

- Replaced protocol v3 outright with typed `surface.sock`, `agent.sock`, and
  `control.sock`; no `service.sock`, parser, conversion, fallback, dual
  protocol, RPC prompt path, control watch/debug/abort, speech route, or widget
  result protocol remains.
- Kept `desktop.sock` only as the independent surface-local window-manager
  command protocol for `scufris-ctl open|hud|show|hide`.
- Bound stable desktop identity from a mode-0600 `surface-id` file beside the
  pending transcript state.
- Made replay, state, and ready one registration lock boundary. A connection is
  broadcast-eligible only after ready is queued.
- Made `agent.message` the sole prompt ingress and `agent.response` the sole
  atomic final response. Widget definitions are snapshotted per accepted
  message. Calls are validated against the selected surface schema.
- Removed the dynamic Pi widget extension and skill, agent-controlled
  conversation extension, spoken event road, detail artifact store and command,
  artifact pruning helper, and their replaced tests.
- Kept local desktop speech, mute, recording, transcription, HUD controls, and
  widget runtime. Replay has no presentation effects. Only the associated live
  desktop surface speaks and executes widget calls.

Verification evidence:

- `cargo test --workspace`: 14 shared protocol, 312 desktop, and 24 service
  tests passed. Coverage includes two-surface broadcast/replay, exact 200-entry
  retention, generation-safe replacement, slow-reader removal, one agent,
  cross-surface steer association, atomic details/widgets, and frontend-local
  replay/presentation behavior.
- `npm run check`: 67 TypeScript and UI tests passed, including deterministic
  XML-safe self-contained prompts, busy steer delivery, atomic response
  emission, wrong channel/version decode, and local handshake failure text.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 89 helper tests
  passed.
- Focused Nix resource, launcher, service, desktop, documentation, and native
  format checks passed.
- `nix flake check`: all 35 x86_64-linux checks passed.
- `nix run .#staging -- up` launched the built v4 service and local desktop.
  All three service sockets were mode 0600. The desktop persisted its mode-0600
  surface ID and reached `idle` through control v4.
- `evidence/proof-v4.json` records the live local proof with the desktop plus
  two synthetic registered surfaces: identical user and assistant broadcast,
  response association to `synthetic-one`, ordered state/ready replay, strict
  wrong-version EOF, strict cross-channel EOF, and rejection of a second agent.
  The real Pi response was `Protocol v4 confirmed.`
- `evidence/staging-v4.log` records the coordinated staging paths and processes.
  The owned staging service, desktop, and wrapper PIDs were stopped explicitly
  after the proof.

## Split staging follow-up

Added separate foreground staging commands for local multi-surface testing:

- `nix run .#staging -- backend` owns the isolated service and Pi agent.
- `nix run .#staging -- frontend NAME` owns one desktop and requires a running
  staging backend.
- Each validated name has an independent durable surface identity, state/data
  roots, command socket, and lock. Different names can run concurrently; the
  same name is refused.
- `up` remains the one-terminal service-plus-desktop convenience and also
  accepts additional named frontends.
- Each command stops only its recorded children. Frontends do not own or stop
  the backend or their peers.

Focused verification: all 8 staging helper integration tests pass, including
simultaneous `one` and `two` frontends, isolated identities and command sockets,
backend independence, same-name refusal, absent-backend refusal, exact teardown,
and command-socket cleanup. ShellCheck and the packaged `scufris-staging` Nix
build pass. A live packaged run launched one backend and two local desktop
frontends with distinct mode-0600 persisted surface IDs and separate
`desktop-one.sock` and `desktop-two.sock` command sockets. Both commands reached
the backend in `idle`; all recorded processes were then stopped.

## Structured logging follow-up

Added INFO/DEBUG observability to both protocol v4 sides:

- Service INFO records named surface and agent connect/disconnect events,
  replacements, listener lifecycle, and shutdown.
- Service DEBUG records accepted channel connections, connection and generation
  IDs, registration/replay/widget details, received surface and agent messages,
  control requests, full typed protocol payloads, broadcasts, recipients, wire
  writes, drops, and clean closes.
- Desktop INFO records its stable identity/name and service connect/close
  lifecycle. DEBUG records resolved paths, connection attempts and failures,
  typed requests and responses, live/replay status, and HTTP transcription
  request/response metadata.
- Named staging frontends export their profile as the protocol diagnostic name,
  so `frontend one` appears as `name=one` in both logs.
- DEBUG protocol records intentionally include conversation/widget payloads;
  audio and transcription response text remain excluded. The operation guide
  documents this privacy boundary and `RUST_LOG` filters.

Verification: 312 desktop and 24 service tests pass; all 8 split-staging tests
pass; ShellCheck passes. A packaged `RUST_LOG=debug` backend plus `frontend one`
produced structured journald entries for agent and named surface connection,
registration, payload send/receive, and control traffic. The registration entry
contained `F_NAME=one`, the stable `F_SURFACE`, connection/generation IDs,
widget count, and replay count. All recorded proof processes were stopped.

## Implementation sequence

Implement the v4 core in this task, then create separate product work for later
batches:

1. **Protocol v4 typed channels**
   - three physical Unix socket endpoints;
   - channel-specific enums and direction rejection;
   - strict exact-version close without a response;
   - bounds for identities, text, details, widget schemas, and calls; and
   - Pi RPC ordering proof.

2. **Registered replay and broadcast**
   - multiple retained surfaces;
   - bound identities and replacement generations;
   - canonical N-entry ring;
   - replay, state, and ready ordering; and
   - two-surface, reconnect, and slow-reader tests.

3. **Latest-sender atomic responses**
   - self-contained Pi messages with embedded widget definitions;
   - cross-surface steer association;
   - one agent response with text, details, and widget calls;
   - frontend-local speech and widget execution; and
   - removal of separate speech and widget-result roads.

4. **Optional Tailscale surface gateway**
   - bounded byte proxy;
   - disabled-by-default deployment;
   - Tailscale-only bind, firewall, and grants; and
   - real remote connection proof.

5. **Remote products**
   - compatible laptop desktop;
   - iOS text surface;
   - external STT/TTS service; and
   - iOS hold-to-talk and local speech.

## Required protocol verification

Before networking or iOS depends on v4, prove:

- different registered surfaces remain connected;
- a repeated ID replaces only its previous generation;
- channel and direction violations are rejected;
- every surface receives identical conversation entries;
- replay replaces local history and ends with ready;
- replay never triggers speech, animation, or widgets;
- only the associated live surface performs local presentation;
- a cross-surface steer changes the response association;
- details broadcast and replay but are never spoken;
- widget calls exist only in the atomic final response;
- widget calls produce no protocol acknowledgement or result;
- every agent message receives the current selected widget schema;
- only one agent is accepted;
- slow-client removal does not affect other surfaces or a replacement; and
- every non-exact version is logged and disconnected without response; and
- clients show a local update-together message after handshake failure.

## Deferred product input

The target laptop operating system, display stack, and architecture must be
recorded before laptop deployment. It does not block protocol v4 or the local
multi-surface implementation.

## Parked work

- terminal session hijack or handoff;
- asynchronous widgets and widget results;
- agent-controlled conversation windows;
- server-owned speech or audio transport;
- control watch, abort, and event streaming;
- public endpoints, relays, accounts, and push notifications;
- multiple users, agents, conversations, or authoritative hosts; and
- generic LLM APIs in the future inference repository.

## Completion criteria

This spike can close when:

- the protocol decisions are recorded;
- the plan is reviewed against the current tree;
- protocol, gateway, laptop, iOS, and inference work are sized separately; and
- review gives a clear go or no-go for remote desktop and personal iOS surfaces.
