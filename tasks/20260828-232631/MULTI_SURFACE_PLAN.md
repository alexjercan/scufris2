# Protocol v4 multi-surface plan

Status: proposed, paused before implementation.

## Goal

Prepare Scufris for a desktop, laptop, and phone to share one host conversation
without duplicate speech, widget commands, window requests, or frontend
reconnect fights.

Every connected surface watches. Exactly one surface is active. The service does
not know whether a surface is local, remote, desktop, or phone.

## Current failures

Protocol v3 and `native/scufris-service/src/service.rs` currently:

- remove the existing frontend when another frontend registers;
- broadcast speech, widget commands, and conversation-window requests;
- keep one last-writer-wins widget catalog;
- accept widget reports from any frontend; and
- treat `scufris-ctl watch` as a full frontend.

Two desktop companions therefore evict each other while both reconnect. Two
frontends that survived would speak the same paragraph, duplicate widget work,
and raise windows on the wrong surface.

## Protocol boundary

### Stable handshake

Protocol v4 is the point where the version handshake becomes permanent.
`hello`, `welcome`, and the mismatch response retain the same shape in every
future version. A server parses `hello` before parsing an operational message.

A mismatch response echoes the version the client spoke, so that client can
always decode the response:

```json
{"v":4,"type":"version","supported":5,"detail":"This surface must be updated."}
```

The operational protocol remains tightly coupled. Only the handshake keeps
cross-version compatibility.

### Roles

Add a read-only `watcher` role:

```json
{"v":4,"type":"hello","role":"watcher"}
```

A watcher receives state, transcript, and notices. It cannot claim presence,
submit as a surface, receive side effects, or announce capabilities.
`scufris-ctl watch` uses this role.

`frontend` remains a surface. `control` remains a request client. `agent`
remains the one Pi process.

### Presence requests

Add correlated frontend requests:

```json
{"v":4,"type":"claim","id":"phone-1"}
{"v":4,"type":"release","id":"phone-2"}
```

Add a per-client service push:

```json
{"v":4,"type":"presence","active":true}
```

The service answers claim and release with `ok` or `refused`.

### Surface capabilities

A frontend announces what it can present. Capabilities describe a surface, not
its location or device class.

```json
{
  "v":4,
  "type":"capabilities",
  "widgets":[],
  "conversationWindow":false
}
```

The desktop announces its widget catalog and a conversation window. The phone
announces an empty widget catalog and no separate conversation window because
the app itself is the conversation HUD.

## Presence policy

Internally this is a revocable lease. In the UI the states are `active` and
`watching`; a watching surface is not asleep because it still receives current
state and transcript.

Rules:

1. The first full frontend becomes active.
2. Claim moves a connected frontend to the top of a recency list.
3. A frontend submit also claims as a final invariant.
4. A control submit does not move presence.
5. Release removes that frontend from the claim list.
6. Disconnect and slow-client removal also remove it.
7. The most recent remaining connected claimant becomes active.
8. If none remains, there is no side-effect destination until a frontend acts.
9. No timer expires presence. Connection loss bounds a stale lease without
   moving output during a long visible answer.
10. A claim never blocks another claim.

Proposed client policy:

- phone foreground sends `claim`;
- phone background sends `release`, with disconnect as the authoritative
  fallback;
- desktop activation, workspace reveal, HUD interaction, and stop claim;
- either surface can reclaim while the other remains connected; and
- opening the phone moves output there, but the next deliberate desktop action
  moves it back.

The foreground-versus-first-touch phone trigger remains a product decision.
Foreground claim is recommended because the whole app is a Scufris surface and
later desktop intent can revoke it immediately.

## Message routing

| Message | Destination |
| --- | --- |
| `state` | all frontends and watchers |
| `transcript` | all frontends and watchers |
| `notice` | all frontends and watchers |
| `speak` | active frontend only |
| `widget` | active capable frontend only |
| `conversation` | active capable frontend only |
| `submit`, `abort` | accepted from allowed callers |

When presence changes, the old surface is told it is inactive. It must stop any
local speech immediately. Already queued side effects must not continue on two
surfaces.

## Widgets and conversation tools

Keep one catalog per frontend rather than one catalog for the service.

The agent needs two views:

- the union of registered capabilities; and
- the capabilities of the active frontend.

The widget extension registers or refreshes the available widget tool schema
from the union. It activates widget tools only when the active frontend can draw
them. The conversation extension activates its tool only when the active
frontend has a separate conversation window.

A phone holding presence therefore makes Scufris answer in ordinary transcript
text and local speech instead of selecting a desktop widget.

Presence changes that affect active tools are deferred to a safe turn boundary
when a tool is executing. A widget command already sent stays pinned to the
frontend that received it.

The service records the recipient of every pending widget command. It accepts
the corresponding report only from that frontend. If the recipient disconnects,
the command fails; it is never replayed on another surface because it may have
already produced a side effect.

Existing desktop widgets may remain visible while the phone is active. They
receive no new agent commands until a capable surface becomes active again.

## Desktop behavior

`native/scufris-desktop/src/link.rs` gains claim and release operations and a
presence event.

Desktop intent claims presence before or with:

- microphone activation;
- workspace reveal;
- conversation HUD activation;
- typed submission; and
- stop or abort.

Submission remains an implicit service-side claim in case a client misses an
explicit one.

Losing presence:

- stops local speech immediately;
- prevents new widget and window side effects because the service no longer
  sends them;
- leaves state, transcript, and notices current; and
- may dim the pill or mark the tray as `watching` without adding a new assistant
  state.

Presence is a surface concern. It must not be folded into `ScufrisState`, which
continues to describe the one agent.

## Implementation batches

### 1. Protocol v4

Change the shared Rust protocol and TypeScript agent protocol:

- permanent handshake and mismatch response;
- `watcher` role;
- claim and release requests;
- presence push;
- capability announcement; and
- focused codec, bound, role, and mismatch tests.

### 2. Service ownership

Change `native/scufris-service/src/service.rs`:

- retain every frontend;
- add claim recency and active frontend state;
- centralize client removal so disconnect, eviction, and slow-client removal
  update presence and pending commands identically;
- fan passive messages out;
- route side effects to presence; and
- retain capabilities and catalogs by connection.

### 3. Agent tool surface

Change `extensions/scufris/widgets/`, `extensions/scufris/conversation.ts`, and
the service client event path:

- receive union and active capabilities;
- refresh widget schemas when the union grows;
- activate or deactivate optional surface tools without clobbering unrelated
  active tools; and
- defer unsafe changes to the turn boundary.

A focused proof must confirm how repeated `pi.registerTool` calls update the
schema of an existing generic widget tool before relying on that behavior.

### 4. Desktop presence

Change `native/scufris-desktop/`:

- expose claim and release on `ServiceLink`;
- deliver presence changes to the runtime;
- claim on deliberate desktop interaction;
- cut speech on loss; and
- present watching without changing assistant state.

### 5. Verification

Add service and integration coverage before a phone app depends on v4.

Required cases:

- two frontends remain connected;
- both receive the same replay and live transcript;
- only presence receives speech, widgets, and window requests;
- claim order, release, disconnect, and slow-client removal select the correct
  fallback;
- frontend submit claims while control submit does not;
- watcher never claims or receives side effects;
- concurrent submit remains a defined prompt or steer;
- a wrong frontend cannot answer another frontend's widget command;
- disconnect fails rather than replays an in-flight widget command;
- capability changes activate the correct tools at a safe boundary;
- desktop speech stops when presence moves; and
- old and new protocol peers receive a readable mismatch response.

Then run a real two-surface proof through the SSH Unix-socket bridge before
starting the iOS product.

## Out of scope

- iOS UI or signing implementation;
- a Scufris TCP listener;
- public relay or push notifications;
- widgets rendered on the phone;
- multiple users, conversations, agents, or hosts;
- the four-widget limit, Claude usage HTTP 429 issue, Dashboardd replacement
  coverage, and general desktop polish.
