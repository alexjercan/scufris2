# Interactive Pi as a Scufris terminal surface

Date: 2026-09-11

## Decision

Keep `scufris-service` as the only owner of the foreground Pi process and its
session. Build the terminal as another protocol-v10 surface.

The terminal must connect to `surface.sock`, submit `surface.message`, and draw
the service's canonical replay. It must not open the Pi JSONL file, attach to
the RPC child's pipes, or become a second agent. If Pi's visual style is
important, build the client from the public `@earendil-works/pi-tui` package.
Do not make the `pi` executable the terminal client.

Do not restore the old debug/session lease as the product architecture. It can
serialize file ownership, but normal interactive Pi input bypasses Scufris
surface correlation, replay, attachments, and widget routing. Fixing that
requires a second input bridge and two presentation histories. It also creates
an avoidable failover boundary every time the terminal opens or closes.

The focused Pi 0.85 result is also no: one explicit extension can bridge the
common text path through both Unix sockets, but current public APIs cannot prove
that every model-bound user message took that path or make the service's
canonical replay the native TUI's presentation source. Also, opening the exact
managed session from a project does not automatically select that project's
resources: Pi 0.85 takes cwd from the resumed session header. A bearer token
can authenticate a handoff, but sockets and an in-memory token cannot fence a
second session writer after service or launcher failure.

Pi 0.85 has a promising hidden experimental server/client architecture. It is
a future replacement-hosting candidate, not a way to attach a TUI to the
current `pi --mode rpc` child. It is not ready for Scufris production use.

## Scope and terminology

This report compares these models:

- **A. Foreground interactive Pi**: stop the service-owned RPC child, start
  normal interactive Pi on the same session, and let that terminal process own
  the agent and TUI until it exits.
- **B. Attached terminal surface**: leave the service-owned RPC child running.
  A separate terminal application is only a Scufris protocol-v10 surface.
- **C. Pi 0.85 experimental server**: replace the classic coding-agent runtime
  with Pi's new server, Session worker, Agent Harness, and experimental client
  TUI. This is included because it changes the long-term answer, but it is not
  one of the two deployable models above.

"Attached terminal surface" does not mean attaching another process to the
RPC child's stdin/stdout. Pi RPC is one LF-delimited command/event stream owned
by the process that launched the child. Today that process is
`scufris-service`. Pi 0.85 exposes no supported attach operation for an
already-running classic RPC process.

## Current invariants

### Ownership and lifecycle

`host/service/src/main.rs` states and implements the current ownership model:

- the service owns one `pi --mode rpc` child;
- it owns the Pi session directory;
- it owns canonical conversation replay, briefing state, attachment storage,
  and the private sockets;
- terminal, desktop, iOS, and `scufris-ctl` processes are clients;
- shutdown stops and waits for the agent before removing sockets.

`host/service/src/agent.rs::Agent::start()` creates the child in its own process
group. `Agent::stop()` closes RPC stdin, waits, sends `SIGTERM`, then sends
`SIGKILL` to that exact process group if needed. This prevents a replacement
agent from racing an old session writer.

`host/service/src/config.rs::Config::agent_args()` starts:

```text
--session-dir <service directory> --continue --mode rpc
```

`Config::resolve()` sets the agent working directory to the user's home. A
surface connecting from a repository does not change the foreground agent's
cwd.

### Typed channels

`shared/control/src/service.rs` is the canonical protocol-v10 definition. It
uses distinct `surface.sock`, `agent.sock`, `control.sock`, and `content.sock`
channels. The service rejects the wrong version, direction, type, enum, and
bounds. Important limits include:

- 64 KiB encoded frame;
- 8 KiB user or response text;
- 32 KiB response details;
- 200 canonical conversation messages;
- 8 attachment references and 16 MiB per attachment;
- 32 widget definitions or calls;
- 8 job rows;
- 128 briefing rows in the canonical type.

A local terminal surface needs only `surface.sock` for messages and controls.
It may use the private bounded `content.sock` HTTP API for managed attachment
upload and download. It must not connect to `agent.sock` or `control.sock`.

`docs/src/dev/surfaces.md` describes the connect barrier:

```text
surface.hello
0..200 surface.message replay entries
surface.state
surface.jobs
surface.briefings
surface.ready
```

The client clears old display state before reconnect. It stores and draws the
replay, but enables live effects only after the matching `surface.ready`.
That file has stale prose references to protocol v9 and a 16-row briefing
bound. The canonical Rust types, examples, and implementation use protocol v10
and 128 briefing rows.

### User-turn correlation

`host/service/src/service.rs::surface_message()`:

1. verifies that the connection still owns the registered surface generation;
2. resolves managed attachment IDs;
3. sends `agent.message` with the selected surface's widget definitions;
4. records and broadcasts the canonical user message;
5. sets `associated_surface` to that surface;
6. acknowledges the submission with the same message ID.

`agent/extensions/scufris/service/index.ts` receives the agent message. If Pi is
idle it calls `pi.sendUserMessage(message)`. If Pi is busy it calls
`pi.sendUserMessage(message, { deliverAs: "steer" })`.

The response has turn-level surface association, not a request ID. A later
surface message delivered as steering changes the associated surface. Every
surface displays the final response, but only the associated ready surface may
perform live effects. `host/service/src/service.rs::agent_request()` closes the
association when it records the atomic response. If there is no live owner, it
records the response under reserved surface ID `unprompted` and removes widget
calls.

A surface keeps a submitted draft until `surface.message_ack`. If the
connection is lost first, the result is unknown and the client must not resend
automatically. This avoids duplicate user turns after an accepted request.

### Atomic responses and widgets

`agent/extensions/scufris/response.ts` registers `scufris_final_response` and
requires it to be the only final tool call. It emits one bounded value with
plain `text`, optional Markdown `details`, attachment IDs, widget calls, and
receipts. The tool returns `terminate: true`.

`host/service/src/service.rs::agent_request()` validates widget calls against
the associated surface's current registration. Invalid calls are dropped
without dropping the prose answer. Widget metadata may remain in canonical
replay, but a reconnecting surface must not execute it during replay. There is
no widget result or acknowledgment protocol.

A plain terminal surface can register no widgets. This preserves the complete
conversation and all other guarantees. Terminal widgets can be added later as
explicit local implementations.

### Replay, jobs, briefings, and proactive delivery

`host/service/src/conversation.rs` owns a bounded, atomically replaced canonical
snapshot. `register_surface()` queues replay, state, complete job state,
complete briefing state, and `ready` under the same service lock that excludes
live broadcasts.

Job execution records are durable under `$XDG_STATE_HOME/scufris/jobs`.
Protocol job rows in the service are in memory, but
`agent/extensions/scufris/service/index.ts` republishes its complete retained
row list when the agent socket reconnects. Session-start workflow recovery
reconstructs jobs from their durable records. A surface reconnect receives the
complete current list, not only changes.

Briefing delivery has a stronger service-owned transaction:

- `control.briefing` persists the row and optional wake before acknowledging;
- an idle gate reserves one proactive slot;
- the exact wake carries `proactive_id` into a Pi custom follow-up;
- the extension reports `agent.proactive_started` and
  `agent.proactive_settled` for that exact custom message;
- `record_delivery()` persists the atomic response in canonical replay before
  the briefing inbox is acknowledged;
- restart recovery deduplicates by delivery ID;
- missing or failed correlation retries with backoff and a bounded circuit.

The relevant code is in `host/service/src/briefings.rs`,
`host/service/src/service.rs::dispatch_briefing_now()`,
`Service::proactive_settled()`, and
`agent/extensions/scufris/service/index.ts::proactiveMessageDetails()`.
Ordinary `control.wake` is intentionally volatile and is refused if no agent
is connected. The caller keeps its own fallback.

## Capability matrix

Legend: **Yes** preserves the current guarantee. **Conditional** needs new
architecture or has a reduced guarantee. **No** conflicts with the model.
**Experimental** means that Pi exposes the mechanism but Scufris parity is not
present.

| Capability                                    | A. Foreground interactive Pi                                                                                                                                                                 | B. Protocol-v10 terminal surface                                        | C. Pi 0.85 experimental server                                                                               |
| --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| One foreground lifecycle owner                | Conditional. A lease can transfer ownership, but ownership changes at every open and close.                                                                                                  | Yes. The service remains the only owner.                                | Experimental. Server and Session worker become the new owners.                                               |
| One Pi session writer                         | Conditional. The service must fully stop and reap RPC Pi before terminal launch.                                                                                                             | Yes. The terminal never opens Pi JSONL.                                 | Yes within the new worker lock model, but it uses a different Session/Harness stack.                         |
| No agent downtime when terminal opens         | No. Handoff requires a stop/start boundary.                                                                                                                                                  | Yes.                                                                    | Usually, but adoption requires a full service cutover.                                                       |
| Native normal Pi TUI                          | Yes.                                                                                                                                                                                         | No. It is a custom Scufris TUI; public `pi-tui` can provide Pi styling. | Experimental client TUI, not the complete normal interactive TUI.                                            |
| Canonical user-message replay on all surfaces | No. Direct Pi input bypasses `surface.message`; a dual-role bridge covers common prose, but Pi 0.85 has unhooked ingress and no canonical transcript projection API.                         | Yes.                                                                    | Experimental transcript snapshots exist, but they do not implement Scufris canonical replay.                 |
| Surface response association                  | Conditional for common prose through the bridge. Unmediated native or extension-triggered turns still become `unprompted`.                                                                   | Yes. Existing last-user/steering rule is unchanged.                     | Attachment routing is exact, but Scufris surface association and effect policy are absent.                   |
| Busy-turn steering                            | Pi can steer locally, but Scufris cannot attribute direct terminal input without a bridge.                                                                                                   | Yes. Existing `deliverAs: "steer"` path.                                | `AgentController.steer()` exists.                                                                            |
| Proactive wake while no terminal is open      | No if interactive Pi is the only owner. A fallback RPC agent must restart.                                                                                                                   | Yes. Terminal presence is irrelevant.                                   | Harness can outlive presentations while active, but Scufris wake sources and durable gate are absent.        |
| Durable briefing response acknowledgment      | Conditional. It works only while the interactive process is correctly connected as the sole agent; transitions add failure cases.                                                            | Yes. Unchanged.                                                         | No Scufris briefing inbox or `proactive_id` transaction.                                                     |
| Durable jobs and row replay                   | Conditional. Extensions still work, but agent transition and direct-input state need reconciliation.                                                                                         | Yes. Unchanged.                                                         | No Scufris workflow/job facets.                                                                              |
| Managed inbound/outbound attachments          | No by default. Native Pi `@file` and images are not Scufris managed IDs.                                                                                                                     | Yes. Use existing bounded content API and descriptors.                  | Prompt images exist, but Scufris storage, descriptors, quotas, and surface APIs are absent.                  |
| Per-surface widgets                           | No by default. Native extension UI and Scufris surface widgets are different contracts.                                                                                                      | Yes. Register local definitions; suppress effects during replay.        | Plugin presentation facets exist, but they are a different API.                                              |
| Surface reconnect and replay barrier          | No. Normal Pi resumes Pi JSONL, not the canonical 200-message surface projection.                                                                                                            | Yes. Existing protocol.                                                 | Snapshot hydration exists. Low-level clients do not replay unsafe requests; transport reconnect is explicit. |
| Terminal crash isolation                      | Conditional. It drops the active agent and requires service recovery.                                                                                                                        | Yes. It drops only one surface.                                         | Presentation attachment drops; admitted calls may still finish remotely.                                     |
| Desktop/iOS remain live                       | Conditional. Service can remain live, but direct terminal input and effects are outside their protocol without an adapter.                                                                   | Yes.                                                                    | No without porting both surfaces or bridging two protocols.                                                  |
| Existing Scufris tools/extensions             | Yes in normal interactive Pi if launched with the Scufris resource arguments.                                                                                                                | Yes. They remain in the RPC agent.                                      | No. Classic extensions are not loaded by the experimental worker.                                            |
| Project-local `.pi` resources                 | Conditional. The exact managed session retains its header cwd, currently home, so launching from a repository does not inject that repository. A tested cwd override plus trust is required. | No, by design. A surface cannot change agent cwd or trust.              | Disabled in the current experimental TUI/worker; only explicit Chord plugin packages are supported.          |
| Normal Pi session commands                    | Dangerous. `/new`, `/resume`, `/fork`, `/reload`, and cwd changes can switch or rebuild the agent outside service lifecycle control.                                                         | Not exposed unless Scufris defines explicit controls.                   | Only a small experimental command set is present.                                                            |
| Remote terminal path                          | Requires inventing a secure remote handoff and terminal channel.                                                                                                                             | Yes through existing WSS gateway if wanted; local Unix is simpler.      | Radius relay exists, but it is experimental and a new trust boundary.                                        |
| Rollback                                      | Complex if a handoff or session mutation is active.                                                                                                                                          | Remove/disable one client; no data migration.                           | Requires rollback of a replacement session host and likely session conversion.                               |

## Model A: foreground interactive Pi

### What works

Normal interactive Pi gives the full supported terminal editor, transcript,
selectors, keybindings, extension UI, theme handling, image input, and normal
session commands. If launched through the Scufris launcher, the existing
workflow, briefing, response, calm, and service extensions load. While its
service extension is connected to `agent.sock`, external surface messages and
wakes can still enter that Pi process.

Simply starting interactive Pi beside the RPC child is invalid. It creates a
second prospective session owner, and the interactive process's service
extension is rejected because `Service::register_agent()` permits only one
agent-channel connection.

A connection-scoped handoff can prevent concurrent JSONL writers. Scufris had
such a mechanism before protocol v4:

- parent revision `9df10267dfa2e5e90a5dd690bb7bede0c3e594cb` exposed
  `scufris-ctl debug`;
- `Service::begin_debug()` checked the control role, reserved one connection,
  removed and stopped the agent, and returned an exact session command;
- connection loss released the lease and called `start_agent()`;
- tests included
  `a_debug_lease_detaches_and_is_held_by_the_connection_that_asked`,
  `a_second_debug_is_refused_while_a_lease_is_held`,
  `a_frontend_may_not_take_the_session_away_from_itself`, and
  `losing_the_connection_releases_the_lease`.

Revision `c122ba8431de26faae4adbb5f91e3f767ab53763` removed that interface during
the protocol-v4 redesign. Current `nix/home-manager.nix` now says, "Scufris
ships no terminal session handoff protocol." Current `host/service/src/bin/scufris-ctl.rs`
has only state, desktop-window, wake, and briefing operations.

The old lease is useful evidence, not a safe implementation to restore. Its
`scufris-ctl debug` process held the control connection while it used
`Command::spawn()` and `child.wait()` for Pi. If that parent was killed, the
kernel closed the lease socket, but an ordinary child could remain alive. The
service could then restart managed Pi while interactive Pi still had the
session. The lease was also only in service memory, had no activation token or
fresh `isCompacting`/`pendingMessageCount` gate, and had no recovery fence for
a service restart. Revision `c122ba8` removed this entire ownership mode rather
than carrying those gaps into the typed surface protocol.

### Why it is not a first-class surface

A normal Pi editor sends its input directly to its own AgentSession. It does
not send `surface.message`. Therefore the service does not:

- record or broadcast the terminal's user line;
- acknowledge its submission ID;
- associate the final response with a terminal surface;
- resolve Scufris attachment IDs or pass terminal widget definitions.

The response extension can still send `agent.response`, but the service sees no
open surface turn. It records that answer as `unprompted`. Other surfaces can
see the answer without the question, and no surface performs live effects.

Making direct Pi input first-class needs a dual-role adapter:

1. register the terminal separately on `surface.sock`;
2. intercept native Pi input before it reaches the agent;
3. send that input as `surface.message`;
4. receive the resulting `agent.message` on the same process's agent extension;
5. inject it into Pi exactly once;
6. hide the Scufris XML routing envelope in the native transcript;
7. reconcile Pi's full JSONL transcript with Scufris's bounded canonical replay;
8. map native file/image input into managed Scufris attachment IDs;
9. implement terminal widget effects separately;
10. block or redefine session-changing commands that break service ownership.

That loopback path is more complex than a surface client and gives no delivery
benefit. It also makes terminal cwd a security-sensitive agent setting.
Running interactive Pi in a project can load trusted project extensions,
skills, prompts, themes, and context files. The current service starts in
`$HOME` with explicit shipped Scufris resources. A handoff from a repository
would silently change tools, code execution, prompt context, and trust.

### Pi 0.85 extension path, traced exactly

Here, "extension-only" means no private Pi import and no Pi source patch. It
still requires changes to Scufris's extension, service, launcher, and wire
protocol. One explicit CLI extension could act as both the terminal surface and
the agent adapter:

```text
Pi editor
  -> input event returns handled
  -> surface.sock: surface.message {id, text, attachment IDs}
  -> service record, association, broadcast, and ack
  -> agent.sock: agent.message {id, text, widgets, attachments}
  -> guarded pi.sendUserMessage(...)
  -> normal Pi model and tool loop
  -> scufris_final_response -> agent.response
  -> canonical surface broadcast
```

The public Pi 0.85 pieces needed for that proof of concept exist:

- `pi.on("input")` sees text, images, source (`interactive`, `rpc`, or
  `extension`), and optional streaming behavior. It can return `handled` or
  `transform`.
- `pi.sendUserMessage()` creates a real user message and can deliver it as
  `steer` or `followUp` while streaming. The source of this reinjection is
  `extension`.
- `pi.sendMessage()` provides context-bearing custom messages with
  `triggerTurn` and `deliverAs`. This is enough for current job, offer, wake,
  and briefing reinjection.
- `session_start`, `session_shutdown`, `agent_start`, `agent_end`,
  `agent_settled`, and message events provide extension lifecycle edges.
- `session_before_switch`, `session_before_fork`, and `session_before_tree` can
  cancel `/new`, `/resume`, `/import`, `/fork`, `/clone`, and tree navigation.
- `ctx.abort()`, `ctx.shutdown()`, public editor/status/widget APIs, Markdown
  transformers, and custom entry renderers all work in TUI mode.
- `/reload` tears down and recreates extensions in the same process. A lease
  held by the process, rather than by one agent socket, could permit a bounded
  reconnect.

The bridge must be the first explicit CLI extension. It would keep a bounded
set of exact service reinjections so its input hook returns `continue` only for
those and `handled` for ordinary interactive or project-extension user
messages. Otherwise its own `pi.sendUserMessage()` loops back into
`surface.message`. This guard is possible for a canary, but it is not a Pi
security boundary; every trusted project extension executes in the same
process.

### Public API gaps that prevent a first-class replacement

#### Input interception is not total

`dist/core/agent-session.js::prompt()` executes extension commands before the
`input` event, then emits `input`, then expands skill commands and prompt
templates. Normal idle and streaming prose reaches this path. Other native
paths do not:

- `dist/modes/interactive/interactive-mode.js` handles built-in commands and
  leading `!` shell commands before `AgentSession.prompt()`. Bash has its own
  public `user_bash` event and can be replaced or disabled, but it is direct Pi
  session context, not a `surface.message`.
- A recognized project extension command is dispatched before `input`. Its
  command text is not a canonical user message. A later
  `pi.sendUserMessage()` can be intercepted by source, but a custom
  `pi.sendMessage({triggerTurn: true})` cannot.
- During compaction, the TUI stores multiple messages itself.
  `flushCompactionQueue()` sends later items with `AgentSession.steer()` or
  `followUp()`. Those methods expand and queue directly and do not emit the
  `input` event.
- A public custom editor could intercept more keystrokes, but it would recreate
  Pi's submit and command dispatcher, would lose composition with another
  project editor, and still would not gate programmatic agent input. That is a
  second terminal implementation hidden inside an extension.

Therefore an extension cannot assert the required invariant: every user-role
message in Pi has exactly one accepted `surface.message` record.

#### Loopback loses normal command expansion or routing metadata

The hook sees raw input before skill and prompt-template expansion. Returning
`handled` prevents Pi from doing that expansion. Pi orders CLI extension paths
before discovered project extensions, so a first bridge that returns `handled`
also prevents later project `input` transformers from seeing the original
text. On reinjection they see the service form instead. There is no post-chain
input hook that can route the final transformed value.

Current `agent/extensions/scufris/service/client.ts` constructs a self-contained
`<scufris_surface_message>` envelope, and `service/index.ts` reinjects it with
`expandPromptTemplates` left false. That correctly prevents a remote surface
from invoking local extension commands, but a terminal line such as
`/skill:name` no longer has normal interactive meaning after the round trip.

Using `expandPromptTemplates: true` does not solve this because the string is
now the routing XML, not the original slash command. Sending the raw string
would lose the per-message widget and attachment context carried by the XML.
`ExtensionAPI.sendUserMessage()` accepts no external message ID, metadata,
separate display text, or separate model text, and its normal extension API
returns `void`. There is no supported call that says: inject this already
accepted user submission, preserve this Scufris ID and metadata, expand only
its skill/template form once, and show this canonical display form.

#### Pi JSONL and canonical replay cannot be one native transcript

Normal interactive Pi rebuilds chat from the selected branch in Pi JSONL.
Scufris replays a separate, bounded 200-message projection. The stores differ
by design:

- Scufris user text is literal, while Pi currently persists the routing XML.
- Pi stores tool calls, tool results, compaction, bash messages, and the full
  branch. Scufris stores only canonical user-facing messages.
- canonical entries retain surface ownership, managed attachment descriptors,
  validated widget calls, receipts, and current `offer.taken` state;
- surface replay messages have no Pi session-entry ID with which to make an
  exact join, and duplicate text is valid;
- canonical state can change after the Pi response entry was persisted, for
  example when an offer is taken.

A Markdown transformer can hide routing markup, and a custom response entry
renderer can improve live display. Neither can replace the built-in chat's
data source. Pi 0.85 exposes no public non-persistent chat append, transcript
projection provider, external replay hydrator, or API to suppress and replace
the normal session transcript. `appendEntry()` is durable and would create a
third persisted projection. A lower editor widget cannot become the scrollable
canonical chat.

The terminal-side surface client could consume replay only to drive status and
side effects, but the visible transcript would still be Pi JSONL. That does
not meet Scufris's canonical replay contract.

#### Interactive lifecycle is missing on the agent channel

Today `scufris-service` learns `agent_start` and `agent_settled` from the stdout
of the child it launched in RPC mode. During a handoff it has no access to
interactive Pi stdout. The current agent-socket request enum has no generic
working/idle lifecycle message. The existing service extension still handles
messages, abort, jobs, responses, offers, and proactive correlation, but
surface state and the briefing idle gate would be stale unless protocol v11
adds authenticated interactive lifecycle snapshots and edges. This gap is in
Scufris, not upstream Pi; the extension hooks provide the source events.

#### Attachments and widgets have two owners

An input hook can upload bounded image bytes to `content.sock` and send managed
IDs. On loopback, it must still preserve the original image content or fetch
those bytes before `pi.sendUserMessage()` if the model is to see pixels; the
current `agent.message` routing envelope contains descriptors, not attachment
bytes. Native `@path` and `!` behavior also exposes project paths rather than
managed Scufris objects.

The terminal can register explicit widget definitions and render live
canonical calls in a public Pi TUI widget. The normal response entry is
persisted before service-side validation, however, and public render APIs
cannot retroactively make that entry identical to canonical replay. Jobs and
briefing rows can be shown in a status widget, and their current durable
recovery paths can continue to work, but that does not close transcript
parity.

### Project-local `.pi` is not selected by launch cwd

This point changes the premise of the proposed handoff. Current
`Config::resolve()` starts managed Pi in the user's home, so its session header
records that cwd. In Pi 0.85:

- `dist/core/session-manager.js::SessionManager.open()` chooses
  `cwdOverride ?? session-header cwd ?? process.cwd()`;
- `dist/main.js` opens the requested session first, takes
  `sessionManager.getCwd()` as the final runtime cwd, and creates project
  settings, trust, resources, providers, and models against that value.

Therefore this does not inject `scufris2/.pi`:

```text
cd /path/to/scufris2
scufris --session /path/to/the/managed-session.jsonl
```

The exact session header wins and the runtime remains rooted at home. CLI
`--extension` and `--skill` paths resolved from startup cwd can be passed
explicitly, but that is not automatic project-local extension, skill, prompt,
theme, settings, package, and context discovery.

If a future cwd override selects the project, Pi trust gates protected project
settings, `.pi` resources, packages, extensions, and `.agents/skills`. It does
not sandbox them. `AGENTS.override.md`, `AGENTS.md`, and `CLAUDE.md` context
files load regardless of the project trust answer unless context loading is
disabled. A handoff must disclose that distinction; "decline trust" does not
mean "use no project instructions."

The internal `AgentSessionRuntime.switchSession()` path accepts a
`cwdOverride`, but Pi 0.85 omits that option from the public
`ExtensionCommandContext.switchSession()` type. An untyped same-file rebind
would depend on private behavior, cause a session shutdown/start cycle, and
leave persisted header provenance at home. It is not an extension-only
workaround under the public-API constraint. Production use needs an upstream
startup cwd-override option for an existing session, or an explicit one-time
Scufris session-cwd migration with independent rollback. Simply changing
process cwd is insufficient.

### Historical lease versus the required handoff

The `9df1026` lease got one important edge right: it removed the `Agent` from
service state and synchronously called `Agent::stop()`, which closed RPC stdin,
signalled the exact managed process group if needed, and waited before handing
out the session command. Current `Agent::stop()` still provides this safe first
half.

It did not provide the second half needed now. The old lease did not mediate
Pi TUI input, authenticate the replacement agent, report interactive
lifecycle, or survive the launcher-parent and service-restart cases described
above. Current `Service::agent_ended()` also restarts a stopped managed child
after a delay unless a new explicit lease state suppresses that path. The
removed `Detached` state no longer exists in protocol v10.

A safe tokenized state machine would require at least:

1. `control.handoff` first installs a drain gate under the service lock. New
   surface prompts and volatile wakes are refused before quiescence is tested.
2. The service sends a fresh correlated Pi `get_state`, not the cached boot
   state, and requires an absolute exact session file, `isStreaming == false`,
   `isCompacting == false`, and `pendingMessageCount == 0`. It also requires no
   associated user turn and no active proactive delivery.
3. The service stops and reaps the exact managed RPC process group. Only then
   does it persist a pending lease generation and create a random, expiring
   lease secret with separate agent and surface authorization.
4. The immutable response names the packaged `scufris` program, exact
   `--session` file, lease generation, activation deadline, and approved
   project cwd. The launcher must not accept program or resource paths from the
   project.
5. The terminal process, not a disposable launcher parent, holds the lease.
   An `exec` handoff can preserve the caller's TTY and control connection, but
   it also needs a kernel-held ownership fence and durable pending/active lease
   record so a restarted service cannot launch RPC Pi during the handoff.
6. A first explicit CLI bridge presents the role-bound authorization on one
   agent registration and one designated surface registration. Initial use
   binds both roles to the lease process identity; reconnect is allowed only for
   that identity and lease generation. Both must become ready before input is
   enabled. Protocol v10 rejects unknown fields, so this is a coordinated
   protocol-v11 change, not an optional v10 field.
7. The authenticated bridge reports idle/working lifecycle and complete job
   rows, handles proactive correlation unchanged, blocks session replacement,
   and reconnects both sockets across `/reload` while the process lease remains
   held.
8. On quit, crash, terminal hangup, activation timeout, or service restart, the
   service waits until the kernel ownership fence is free before starting and
   accepting input for managed RPC Pi. It invalidates the token before restart.

A bearer token alone is not the ownership fence. If the service stores only an
in-memory token, a service crash forgets the handoff and startup launches a
second writer. If only the old control connection holds the lease, killing its
parent can release the lease before its child exits. A robust design needs a
separate flock/pidfd-style fence and durable generation record, or a
service-owned PTY and child. That requirement means the literal proposal of
"only an extension and Unix sockets" is not safe.

The token authenticates accidental peers, not trusted project code. A trusted
`.pi` extension is unrestricted code in the same process and user account. It
can inspect environment, descriptors, session data, and user-owned sockets.
The bridge must use a 256-bit token, constant-time comparison, one active peer
per role, binding to the first authenticated process identity, short activation
expiry, no logs, and immediate release invalidation. Reconnect must require the
same identity and generation. No such token can sandbox another loaded
extension.

### Extension, Scufris, and upstream boundary

| Requirement                                                                                        | Current public Pi 0.85 extension plus Scufris work                                                                                                                                                       | Required upstream Pi support                                                                                                              |
| -------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Dual agent/surface socket client                                                                   | Yes. Add a bounded `SurfaceClient` and activate it only for authenticated TUI handoff.                                                                                                                   | None.                                                                                                                                     |
| Common idle/streaming prose loopback                                                               | Yes. `input` can return `handled`; `sendUserMessage` can reinject and steer.                                                                                                                             | None for a canary.                                                                                                                        |
| Every model-bound user message crosses the service once                                            | No. Extension commands, custom trigger turns, and compaction queue `steer`/`followUp` paths are not one total hook.                                                                                      | One cancellable submission middleware for every user-role ingress path, after command policy is known and before model queueing.          |
| Preserve raw display, one-time skill/template expansion, images, routing metadata, and external ID | No single injection API carries these separately, and normal `sendUserMessage` is fire-and-forget.                                                                                                       | An awaitable accepted-user-message API with external ID, display content, model content, images, metadata, and explicit expansion policy. |
| Canonical replay as native chat                                                                    | No. Public renderers transform Pi entries; they do not replace the transcript source or append ephemeral canonical entries.                                                                              | A supported presentation/transcript projection adapter with reset/hydrate/live-boundary semantics.                                        |
| Project resource discovery on an existing managed session                                          | Only explicit resource paths. The internal runtime has a cwd override, but the public extension command type does not expose it.                                                                         | A documented startup cwd override for an existing session, with normal trust and resource loading.                                        |
| Interactive service lifecycle                                                                      | Yes after adding authenticated lifecycle messages to the Scufris agent protocol.                                                                                                                         | None. Existing Pi hooks are enough.                                                                                                       |
| Proactive wakes, offers, jobs, and briefing correlation                                            | Yes. Existing custom-message and agent-socket paths remain usable if the handoff is quiescent and authenticated.                                                                                         | None for current semantics.                                                                                                               |
| Managed attachment references                                                                      | Yes for upload/download and descriptors. The bridge must add bounded image rehydration if model vision is required.                                                                                      | No core change if the bridge performs content fetch and uses image-capable `sendUserMessage`.                                             |
| Terminal widgets and row status                                                                    | Yes for explicit best-effort TUI widgets after `surface.ready`. Exact replay placement remains blocked by transcript projection.                                                                         | The projection API above for full parity.                                                                                                 |
| Session-switch guards                                                                              | Mostly. Before-events can cancel replacement and tree operations; `user_bash` can replace native shell execution; `/reload` can reconnect. Trusted extension side effects remain outside service policy. | A host policy hook is needed only if all native actions must be mediated.                                                                 |
| Crash-safe single writer                                                                           | Not an extension concern. Scufris needs a durable lease generation and kernel-held process/file ownership fence.                                                                                         | None, provided Scufris owns the fence.                                                                                                    |

### Recommendation and smallest viable plan

Do not implement this handoff against unmodified Pi 0.85. The smallest current
implementation is Model B, the protocol-v10 terminal surface. It is already the
only route that preserves all service guarantees without a process transition.

If native Pi TUI plus project `.pi` injection remains a requirement, use this
order:

1. Specify upstream conformance tests for total user-submission middleware,
   accepted external user-message metadata/IDs, canonical transcript
   projection, and existing-session cwd override. Get supported public APIs,
   not imports from `dist/modes/interactive/*` and not a permanent local patch.
2. Pin the resulting Pi release and update the repository's 0.84.2 test family
   in lockstep. Run the bridge contract tests against that exact executable.
3. Add protocol v11 with drain/handoff, tokenized agent and designated-surface
   hello, interactive lifecycle, and explicit release. Add a durable lease
   generation plus kernel ownership fence before any production session is
   used.
4. Build one combined explicit CLI bridge. Keep project discovery behind Pi's
   visible trust decision. Block session replacement and keep service
   attachments, association, canonical replay, jobs, briefings, offers, and
   widget policy authoritative.
5. Canary only in an isolated runtime and copied session. Promote only after
   kill-at-every-edge, service-restart, duplicate-input, replay-equivalence, and
   rollback tests pass.

A reduced debug prototype can be smaller, but it must be named non-first-class:
it may use the current `input` hook, hide the XML with a Markdown transformer,
and consume canonical replay only for status. It can pass selected project
resources explicitly, but it cannot claim automatic project `.pi` injection
for the exact managed session. It must not be connected to production sockets
or sessions, because the known input and presentation gaps are the properties
under test.

Even after upstream support, Model B remains smaller and safer. The handoff is
justified only if full native Pi interaction and temporary project agent
capabilities are more valuable than continuous service ownership.

## Model B: service-owned RPC agent plus terminal surface

### Data path

```text
terminal editor
  -> surface.message {id, text, attachment IDs}
  -> scufris-service canonical user record and ack
  -> agent.message
  -> service extension -> pi.sendUserMessage / steer
  -> scufris_final_response
  -> agent.response
  -> service validation, association, persistence, broadcast
  -> terminal and every other surface
```

The terminal owns only local input state and presentation. It can terminate,
restart, resize, or lose its socket without changing agent ownership. Service
and agent restarts use the same recovery paths as desktop and iOS.

### Pi-like terminal UI

Use the public `@earendil-works/pi-tui` package for layout primitives, input,
focus, key decoding, Markdown display, and theme-compatible colors where its
public API is sufficient. Keep Scufris protocol and state management in the
terminal application.

Do not import installed private modules such as
`dist/modes/interactive/components/*` or `dist/modes/interactive/theme/*`.
They are not package exports and can change in a patch release. Pi's normal
interactive application is not a reusable view that can be pointed at an
external classic RPC process.

The first version should register `widgets: []`. It can still show text,
Markdown details, attachment descriptors, receipts, offers, jobs, and
briefings. Add terminal-native widgets only when each backend has an explicit
local implementation and validation test.

### Terminal state

Persist only presentation state:

- a stable private surface ID;
- unsent editor content;
- one pending submitted draft and its request ID;
- optional scroll position and local preferences.

Do not persist or parse Pi session entries. On reconnect, clear the rendered
conversation and rebuild it from protocol replay before `ready`. Mark an
unacknowledged submission as unknown. Never replay it automatically.

## Pi 0.85 experimental server/client

### Exact activation and APIs

This implementation is hidden, experimental, and not in the installed user
documentation. `/nix/store/aikz711i5wjvq0i8jc1x154jqa7abm9y-pi-src-with-lock/dist/core/experimental.js`
enables it only when `PI_EXPERIMENTAL === "1"`.
`dist/main.js::runExperimentalCommand()` dispatches when the first CLI argument
is `server` or `client`. The actual commands are therefore:

```text
PI_EXPERIMENTAL=1 pi server ...
PI_EXPERIMENTAL=1 pi client ...
```

They are not `pi experimental server` and are absent from normal help. A local
smoke run of installed Pi 0.85.0 with an isolated home and
`PI_OFFLINE=1` reported:

```text
Server: 01234567-89ab-4def-8123-456789abcdef
Socket: <isolated-home>/.pi/server/01234567-89ab-4def-8123-456789abcdef.sock
Radius: not connected; local only
```

The public experimental coding-agent declarations include:

- `openClientRuntime()` and `activateBuiltinClientServices()` in
  `dist/experimental/client-runtime.d.ts`;
- `runClient()` in `dist/experimental/client.d.ts`;
- `ExperimentalClientTui` and `ExperimentalChatView` in
  `dist/experimental/client-tui*.d.ts`;
- `startForegroundServer()` and activation helpers in
  `dist/experimental/server.d.ts`;
- plugin services exported through the package's
  `./experimental/plugin` entry point.

The underlying packages expose stronger general primitives:

- `@earendil-works/pi-client::Client.connect()`, `request()`, service
  subscriptions, `reconnect()`, and Unix discovery/transport;
- `@earendil-works/pi-server` routed server and Session handles;
- durable addresses `{serverId, sessionId, attachmentId}`;
- request-ID correlation and framed CBOR;
- complete subscription snapshots followed by ordered updates;
- multiple presentation attachments to one Session;
- connection-scoped attachment release after admitted calls settle;
- server and Session-scoped Chord facet services.

This is close to the long-term shape Scufris wants: one hosted harness with
multiple presentations.

### Current blockers

It is a replacement runtime, not an attach API for classic RPC. The current
experimental Session worker in `dist/experimental/session-worker.js` creates a
new `AgentHarness` with only `read`, `write`, and `bash`. It passes
`resources: {}`. It does not start the classic coding-agent AgentSession or load
Scufris extensions.

`dist/experimental/client-tui.js::runClientTui()` creates a
`DefaultResourceLoader` with:

```text
noExtensions: true
noSkills: true
noPromptTemplates: true
noContextFiles: true
```

It loads themes and explicit experimental Chord plugin facets. `-e` means a
plugin package with conventional `src/session.ts` and `src/tui.ts` facets, not
a classic Pi extension path. The example package imports
`defineFacet`, `AgentController`, `PresentationUI`, and `SlashCommands`.
Porting Scufris would be a rewrite of its extensions as server/Session/TUI
facets plus a host integration for canonical surface policy.

The built-in experimental TUI currently exposes a small command set:
`/model`, `/thinking`, `/compact`, and `/reload`. It has prompt, steer,
follow-up, abort, transcript snapshots, and queue display, but not full normal
Pi interactive behavior.

The low-level client deliberately does not reconnect or replay requests
automatically. After transport loss, callers reconnect, reattach, and repeat
only operations known to be safe. Accepted work can complete after a client
loses its local response. This is correct, but Scufris would still need its
pending-submission and canonical-delivery policy above it.

The `@earendil-works/pi-server` README also states that peer authentication is
application policy and is not implemented by the experimental Unix transport.
The coding-agent wrapper creates a user-owned mode-0700 server directory and
mode-0600 sockets, which is suitable for a local experiment but is not a remote
security model. Radius adds a relay and authentication boundary that Scufris
has not reviewed.

Adoption also requires proving or converting session formats. The experimental
server uses pi-agent-core v4 `JsonlSessionRepo` Session/Harness records. Current
Scufris owns a classic coding-agent session selected with `--session-dir` and
`--continue`. Similar JSONL storage does not establish direct compatibility.
Never point both implementations at the same production session directory.

### Research promotion gate

Reconsider Model C only after all of these are true:

1. server/client commands and APIs are documented and supported rather than
   hidden behind `PI_EXPERIMENTAL`;
2. one pinned release passes restart, replacement, worker-retirement, and
   multi-presentation tests;
3. classic extension/tool or facet parity exists for every Scufris extension;
4. Scufris canonical response association, replay barrier, jobs, briefing
   transaction, attachments, widgets, and offers have one defined owner;
5. local peer authorization and remote relay policy are reviewed;
6. session export/import and rollback are tested on copies;
7. desktop, iOS, terminal, and proactive producers can cut over together or use
   one bounded compatibility gateway.

## Version boundary

The repository and active installation are intentionally not the same thing
today:

- `package.json` development dependencies pin
  `@earendil-works/pi-coding-agent` 0.84.2 and use `^0.84.2` for `pi-ai` and
  `pi-tui`;
- `package-lock.json` resolves the Pi libraries to 0.84.2;
- peer dependencies are `*`, so the extensions are hosted by the runtime's Pi
  package;
- the inspected installed executable is
  `/nix/store/g252cvfmlmrfm9jysmfcxr6v0jyqybgy-pi-0.85.0/bin/pi` and reports
  0.85.0;
- `nix/launcher.nix` prefers `type -P pi` over its Nix fallback, so a system Pi
  update can change production runtime behavior without changing Scufris's
  TypeScript test dependency.

The current APIs used by Scufris exist in installed 0.85, but this is a
compatibility claim that needs tests, not an implied lockstep pin.

For a terminal built with `pi-tui`:

- pin and lock the terminal's direct `pi-tui` version;
- do not rely on the installed executable's private files;
- test the Scufris extensions against both the repository baseline and the
  selected deployed Pi version;
- upgrade `pi-ai`, `pi-coding-agent`, and `pi-tui` together when their shared
  types or singleton UI state require it;
- add a release check that records the Pi version used by the service package.

Experimental plugins require the 0.85 Chord/Pi package family and must not be
compiled against the 0.84.2 classic extension dependency graph.

## Staged implementation plan

No protocol change is required for the initial terminal.

### Stage 0: contract spike

- Create an isolated terminal client fixture, not a session handoff.
- Choose TypeScript plus public `pi-tui` if Pi visual parity is required.
- Mirror the strict protocol-v10 surface types and bounds.
- Add cross-language fixtures generated from `shared/control/src/service.rs` so
  Rust and terminal codecs reject and accept the same frames.
- Prove connect, replay, `ready`, disconnect, and terminal cleanup against
  `nix run .#staging -- backend`.

Exit: no Pi session file is opened, and killing the terminal does not change the
service agent PID or lifecycle.

### Stage 1: read-only terminal

- Register one stable terminal surface ID with `widgets: []`.
- Render canonical user/assistant messages, literal text, Markdown details,
  state, jobs, briefings, receipts, and attachment metadata.
- Clear and hydrate on every connection. Suppress all live effects until
  `ready`.
- Reconnect with bounded backoff and visible offline state.

Exit: the terminal can start before or after the service, survive service and
agent restarts, and show the same canonical 200-message suffix as desktop.

### Stage 2: text input and control

- Add an editor and fresh bounded request IDs.
- Keep submitted text pending until the matching ack.
- Preserve unsent and unknown-outcome text across disconnects without automatic
  resend.
- Implement abort, job cancel/archive, briefing dismissal, and offer take as
  their typed ID-only operations.
- Follow the existing busy-steering and proactive-slot refusal responses rather
  than inventing a local queue.

Exit: concurrent terminal and desktop submissions pass the association and
steering tests below.

### Stage 3: managed attachments and presentation parity

- Upload user-selected files through the bounded content API and send only
  returned IDs in `surface.message`.
- Download response attachments by opaque ID. Treat IDs as references, not
  authorization.
- Apply safe Markdown/link rules: no response HTML, no remote media, and only
  credential-free HTTP/HTTPS links with hosts.
- Add keyboard, resize, narrow-width, selection, scroll-follow, and
  accessibility behavior suitable for terminals.

Exit: attachment limits, interrupted transfers, reconnect, and link safety
match desktop/iOS behavior.

### Stage 4: optional terminal widgets

- Define an explicit allowlist of terminal-owned widget backends.
- Register only implementations available in this terminal process.
- Validate calls before dispatch.
- Run calls only for a new live assistant message associated with this ready
  terminal. Never run calls while hydrating replay.
- Keep failures best effort and never discard response prose.

Exit: widgets are harmless under replay, disconnect, invalid names/arguments,
and terminal replacement.

### Stage 5: package and enable

- Add a separate `scufris-terminal` package. Keep it out of the headless
  service closure unless enabled.
- Use the existing Home Manager `terminalCommand` integration to open it from
  the tray.
- Keep the old desktop/iOS clients and protocols unchanged.
- Make enablement optional for one release before considering a default.

Exit: disabling or uninstalling the terminal leaves the service, agent,
sessions, desktop, iOS, jobs, briefings, and attachments unchanged.

### Separate future stage: experimental Pi server canary

Use a private temporary `PI_SERVER_DIR`, a non-production Session directory,
`PI_OFFLINE=1`, and no Radius. Port one inert test facet first. Do not connect
it to Scufris production sockets or copy a production JSONL file into it.

## Required tests

### Protocol and client unit tests

- Accept every valid protocol-v10 surface response and reject wrong versions,
  unknown fields/types, invalid enums, duplicate IDs, binary frames, and every
  over-bound value.
- Bound partial lines and connection buffers before allocating or parsing.
- Verify stable surface ID storage is mode 0600 and regenerated safely after a
  malformed file.
- Verify pending drafts settle only on matching `surface.message_ack`,
  `surface.aborted`, or a matching rejection.
- Verify disconnect before ack becomes unknown and never auto-sends.
- Verify replay messages cannot trigger widget, speech, notification, link-open,
  or attachment-open side effects.

### Service integration tests

1. **Replay barrier**: connect while broadcasts occur. Observe exactly one
   ordered replay/state/jobs/briefings/ready prefix and no gap or duplicate at
   the live boundary.
2. **Replacement generation**: connect twice with the same stable surface ID.
   Only the new connection can submit or control.
3. **Slow reader**: fill one terminal outbox. The service drops it without
   blocking the agent or other surfaces.
4. **Idle submission**: terminal message is recorded once, acked by ID, sent to
   Pi once, and the final response is associated with terminal.
5. **Busy steering**: desktop opens a turn, terminal submits while Pi is busy,
   and final association follows the documented steering rule.
6. **Owner disconnect**: terminal disconnects before the response. The response
   is recorded as `unprompted`, reaches remaining surfaces, and has no widgets.
7. **Abort**: a terminal abort closes the association only after the service
   accepts and relays it; wrong/stale IDs do not settle another draft.
8. **Agent loss**: terminal remains connected while RPC Pi crashes and restarts.
   State changes are visible, rows republish, and input receives a bounded
   rejection while unavailable.
9. **Service loss**: terminal reconnects after service restart, clears stale UI,
   hydrates canonical replay, and does not duplicate a pending turn.
10. **Proactive wake**: an unprompted ordinary wake produces no terminal-owned
    effects.
11. **Briefing gate**: a reserved proactive slot refuses user input with
    `no_free_slot`; the terminal keeps the draft for manual retry.
12. **Briefing transaction**: crash before and after canonical
    `record_delivery`, then prove one response and one delivered acknowledgment
    after recovery.
13. **Jobs**: reconnect receives the complete reconstructed row list; whole-list
    replacements remove archived rows.
14. **Briefings**: quiet progress never creates a conversation line or live
    effect; dismissal is accepted only for eligible delivered rows and updates
    every surface.
15. **Offers**: taking an offer spends it once across all surfaces and replay;
    no prompt text crosses the surface channel.
16. **Attachments**: upload/download, 16 MiB limit, duplicate references,
    missing/expired IDs, interrupted writes, ranges, quota, and response
    descriptor replay.
17. **Widgets**: definitions are selected from the currently associated
    surface, invalid calls drop without losing prose, and replay executes none.
18. **Three surfaces**: desktop, iOS fixture, and terminal all display the same
    canonical messages while only the associated ready surface runs effects.

### PTY and packaging tests

- clean exit, Ctrl-C, Ctrl-D, terminal hangup, resize, very narrow width, Unicode
  input, paste, long wrapped Markdown, and non-interactive invocation;
- terminal process cleanup by recorded PID, with no broad process matching;
- package closure does not pull terminal dependencies into a disabled headless
  service;
- tray `terminalCommand` starts the packaged executable;
- protocol and Pi dependency versions are printed in diagnostic output;
- staging works with an isolated runtime directory and cannot touch deployed
  sessions.

### Additional tests before any foreground handoff canary

- Linearize the drain gate against a simultaneous desktop submission, wake,
  briefing reservation, agent settle, and response. No accepted work may be
  stranded on the stopped RPC child.
- Require fresh `get_state` values for `isStreaming`, `isCompacting`,
  `pendingMessageCount`, and the exact absolute session file. Reject malformed,
  stale, timed-out, or mismatched responses.
- Kill the control client before launch, after spawn but before exec, before and
  after lease-lock acquisition, before each hello, after one hello, during
  `/reload`, during a turn, and during release. At no point may two Pi processes
  own the file.
- Kill and restart `scufris-service` in every pending and active lease state.
  Prove that its durable generation and kernel fence fail closed, then prove
  exact managed-RPC recovery after the terminal process exits.
- Reject wrong, expired-before-activation, cross-process replay,
  cross-generation, second-agent, and second-surface authorization. Accept only
  same-process reconnect for an active lease. Redact token values from protocol
  errors and logs.
- Exercise ordinary text, busy steer, follow-up, multiple messages typed during
  compaction, startup prompts, skills, prompt templates, extension commands,
  programmatic user messages, custom trigger turns, native bash, images, and
  duplicate literal text. Every model user message must map to exactly one
  canonical accepted submission or the test must document the unsupported
  path.
- Compare Pi JSONL and canonical replay after duplicate text, compaction,
  service restart, response validation failure, owner disconnect, attachment
  expiry, and offer take. The native visible projection must equal canonical
  replay where the product claims parity.
- Prove `/new`, `/resume`, `/import`, `/fork`, `/clone`, and tree navigation are
  cancelled without changing the session file. Prove `/reload` reconnects both
  roles under the same process lease.
- Start with running jobs and each briefing delivery state. Verify row
  republish, proactive ID continuity, exactly-once canonical delivery, and no
  user turn captured by a briefing across both handoff edges.
- Deny project trust, approve it, and load a failing or hostile fixture
  extension. Verify the stated trust boundary and deterministic rollback.

### Experimental server tests before any promotion

- two presentations attach to one Session and steer concurrently;
- stale `{serverId, sessionId, attachmentId}` routes are rejected;
- accepted operations complete safely after presentation disconnect;
- server replacement, coordinator loss, worker crash, orphan-demand timeout,
  and explicit client reconnect preserve a coherent snapshot;
- every Scufris extension event/tool has a tested facet equivalent;
- session import/export is round-tripped on copies and rollback is proven;
- Unix directory/socket ownership and Radius authentication are adversarially
  tested.

## Security boundaries

### Terminal surface

- Trust the local Unix socket only through its private user runtime directory.
  A surface ID is stable routing identity, not authentication.
- Connect only to `surface.sock` and, for attachment bytes, the bounded
  `content.sock` API. Never connect to `agent.sock` or `control.sock`.
- Never read, write, tail, lock, or infer state from the Pi JSONL session.
- Enforce protocol version and byte/count bounds before rendering.
- Keep drafts and the stable ID in private files. Do not log bearer tokens,
  attachment bytes, prompts, or response details by default.
- Treat Markdown and widget arguments as untrusted presentation data. Render no
  HTML or remote media. Open only validated HTTP/HTTPS links after user action.
- Do not execute arbitrary widget names, shell commands, file paths, URLs, or
  code received from the service.
- Upload only files explicitly selected by the user. Send managed IDs, never a
  local path, across `surface.sock`.
- Running the terminal from a project directory must not load that project's
  Pi extensions, skills, prompt templates, context files, or trust state. The
  terminal is not the agent.

### Foreground handoff, if researched again

- Treat the approved project as executable code with the user's full local
  authority. Pi has no built-in sandbox, and a token does not isolate
  extensions in one process. Project context files can affect the prompt even
  when protected `.pi` resources are not trusted.
- Keep the lease token random, short-lived for activation, one-generation,
  constant-time compared, absent from logs, and invalid after release. Bind its
  active roles to the first process identity, but allow that same process to
  reconnect after `/reload` or service restart. Prefer
  an inherited private descriptor over command-line text, but do not claim it
  is secret from same-process code.
- Persist only a token hash and lease generation if crash recovery needs a
  record. Keep the record and lock path mode 0600 under a mode-0700 directory.
- Do not rely on PID alone. Use a kernel-held lock or pidfd-like identity and
  fail closed after service restart. Never restart RPC Pi merely because one
  socket closed.
- Stop and reap the managed process group before granting the file. On return,
  prove the terminal process/fence is gone before starting managed Pi.
- Resolve and display the exact project root before trust. Do not accept an
  executable, extension path, socket path, session path, or resource list from
  project configuration as part of the grant.
- Keep desktop and iOS connected during the transition, but refuse new prompts
  while no authenticated agent is ready. Do not record a user message that
  cannot be delivered.
- Never expose handoff, agent, control, content, or lease operations through the
  remote surface gateway.

### Remote use

If remote terminal access is added, use the existing loopback-only surface
gateway, WSS, complete bearer-token comparison, and secret storage. Opaque
attachment IDs are not authorization. Do not expose Pi RPC, `agent.sock`,
`control.sock`, `content.sock`, the experimental Unix server, or a shell over
the remote route.

### Experimental Pi server

Treat its Unix filesystem permissions as the present local authorization
boundary. Its package README explicitly leaves peer authentication to the
application. Keep Radius disabled until relay trust, credential scope, prompt
confidentiality, server identity, revocation, and replay behavior have a
separate threat model.

## Migration and rollback

### Recommended terminal surface

Migration is additive:

1. back up no session data because the terminal does not transform it;
2. install the optional terminal package;
3. create its stable surface identity and local presentation state;
4. connect it to the existing service;
5. enable the tray terminal command after staging passes.

There is no conversation import. The service's current Pi session, canonical
replay, briefing inbox, attachments, job records, desktop, and iOS clients stay
in place.

Rollback is immediate: close and disable the terminal command, then uninstall
the terminal package. Preserve its unsent/unknown draft file if desired. Do not
restart or rewrite the service session. No protocol downgrade is needed because
v1 adds no messages.

If implementation discovers a required wire change, bump the protocol and
update service plus every surface together. Do not make protocol-v10 decoders
silently accept a new field or behavior.

### Foreground Pi handoff

A rollback while a handoff is active must first close the drain gate, invalidate
the exact lease generation, request graceful shutdown from the authenticated
bridge, stop the exact terminal process if needed, and wait for its kernel
ownership fence to clear. Only then may the service restart RPC Pi on the exact
recorded session and reopen ingress. Do not infer release from one socket EOF or
PID alone. Canonical user lines that bypassed `surface.message` cannot be
reconstructed safely from Pi JSONL without a designed importer. If any were
possible, quarantine the session copy rather than guessing. This is another
reason not to deploy the model against Pi 0.85.

### Experimental server

Migration would be a host replacement, not an in-place upgrade. It needs
separate storage, copy-based conversion, a quiesced cutover, checksums/counts,
and retained read-only backups. Never dual-write one JSONL directory. Rollback
must restore the classic service and its untouched session/replay/briefing
stores. Until a tested converter and parity gateway exist, there is no accepted
production migration path.

## Evidence index

### Repository

- `host/service/src/main.rs`: service ownership, sockets, startup, shutdown.
- `host/service/src/agent.rs`: exact child process-group ownership and bounded
  stop.
- `host/service/src/config.rs`: home cwd and classic RPC arguments. Its comment
  about a former debug lease is historical residue, not a current handoff API.
- `host/service/src/rpc.rs`: used Pi RPC events, request correlation, and
  cancellation of blocking extension UI dialogs.
- `host/service/src/service.rs`: surface registration/replay, association,
  steering ingress, proactive gate, jobs, briefings, response validation.
- `host/service/src/conversation.rs`: bounded canonical replay and delivery-ID
  persistence.
- `host/service/src/briefings.rs`: durable briefing rows and wake inbox.
- `host/service/src/attachment.rs`: durable bounded content store and private
  API.
- `shared/control/src/service.rs`: canonical protocol-v10 types, reserved
  `unprompted`, and bounds.
- `agent/extensions/scufris/service/index.ts`: agent socket integration,
  `pi.sendUserMessage`, `steer`, custom follow-up wakes, reconnect row publish,
  proactive correlation.
- `agent/extensions/scufris/service/client.ts`: agent-channel reconnect and
  dispatch.
- `agent/extensions/scufris/service/protocol.ts`: matching TypeScript agent
  protocol and `surfacePrompt()` envelope.
- `agent/extensions/scufris/response.ts`: atomic response tool and terminal
  entry renderer.
- `agent/extensions/scufris/service/attachments.ts`: managed outbound
  attachment import.
- `agent/extensions/scufris/workflow/orchestration.ts`: durable job recovery and
  follow-up wakes.
- `scripts/scufris-agent`: explicit production-like extension and skill set.
- `scripts/scufris-dev`: isolated interactive development sessions, not the
  deployed conversation.
- `host/service/src/bin/scufris-ctl.rs`: current control verbs; no handoff.
- `nix/home-manager.nix`: terminal command hook and explicit absence of session
  handoff.
- `nix/launcher.nix`: extension arguments and system-Pi preference.
- `package.json`, `package-lock.json`: 0.84.2 development library baseline.
- `docs/src/dev/architecture.md`, `service.md`, `surfaces.md`, `messaging.md`,
  `jobs.md`, `briefings.md`, `widgets.md`: documented ownership and delivery
  contracts.
- Git revisions `9df1026` and `c122ba8`: former debug lease and its removal.

### Installed Pi 0.85 documentation

Root:
`/nix/store/g252cvfmlmrfm9jysmfcxr6v0jyqybgy-pi-0.85.0/libexec/pi/`

- `README.md`: modes, resources, and environment.
- `docs/rpc.md`: classic RPC commands/events, queue behavior, extension UI, and
  the complete `get_state` fields needed for quiescence.
- `docs/sdk.md`: AgentSession construction and session management.
- `docs/extensions.md`: `input` source/handling, `user_bash`,
  `sendUserMessage`, `sendMessage`, lifecycle and session-before hooks,
  shutdown, renderers, and public TUI integration.
- `docs/tui.md`: public low-level TUI API.
- `docs/session-format.md`, `docs/sessions.md`: classic coding-agent JSONL and
  session selection.
- `docs/settings.md`, `docs/packages.md`, `docs/skills.md`,
  `docs/prompt-templates.md`, `docs/themes.md`: resource discovery, project
  scope, and trust.
- `docs/security.md`: project-trust inputs, context files that load regardless
  of trust, same-user extension authority, and the absence of a sandbox.
- `docs/terminal-setup.md`, `docs/keybindings.md`, `docs/tmux.md`: terminal
  behavior.
- `examples/extensions/README.md`, `examples/sdk/README.md`: supported extension
  and SDK composition.
- `package.json`: public exports and exact 0.85.0 package dependency family.

### Pi 0.85 classic implementation

Materialized coding-agent source:
`/nix/store/aikz711i5wjvq0i8jc1x154jqa7abm9y-pi-src-with-lock/`

- `dist/cli/args.d.ts` and `dist/cli/args.js`: no normal startup cwd override
  for a selected existing session.
- `dist/main.js`: session selection, final session cwd, project trust, and
  cwd-bound resource construction.
- `dist/core/session-manager.js::SessionManager.open()`: resumed-session header
  cwd precedence and the internal optional `cwdOverride`.
- `dist/core/agent-session.js::prompt()`: command-before-input ordering,
  source-aware input emission, expansion, queueing, and
  `sendUserMessage()` behavior.
- `dist/core/resource-loader.js`: CLI-first extension ordering, pre-trust
  loading, final project extension merge, and cwd-bound resources.
- `dist/core/extensions/runner.js`: ordered input handling, user-bash handling,
  and first cancel for session-before events.
- `dist/core/extensions/types.d.ts`: exact public extension, input, user-bash,
  context, message, lifecycle, session-control, UI, and shutdown types.
- `dist/core/agent-session-runtime.js`: cancellable switch/fork/import lifecycle
  and its internal cwd override, which the public command-context type omits.
- `dist/modes/interactive/interactive-mode.js`: built-in and bash dispatch,
  editor clearing, native busy submission, compaction queues, reload, and chat
  rebuild from Pi session entries.

### Pi 0.85 experimental implementation

Materialized coding-agent source:
`/nix/store/aikz711i5wjvq0i8jc1x154jqa7abm9y-pi-src-with-lock/`

- `dist/core/experimental.js`: `PI_EXPERIMENTAL=1` gate.
- `dist/main.js`: hidden `server`/`client` dispatch.
- `dist/cli/experimental/commands/server.js` and `client.js`: accepted command
  options.
- `dist/experimental/server.js`: directory permissions, logical server profile,
  coordinator, Session repo, and activation lifecycle.
- `dist/experimental/client-runtime.js`, `client.js`: discovery, attach,
  service activation, and prompt flow.
- `dist/experimental/client-tui.js`, `client-tui-chat.js`: snapshot-driven
  experimental TUI, reconnect handling, resource disabling, and rendering.
- `dist/experimental/session-worker.js`: worker lock, Agent Harness, tools,
  operation lifecycle, and empty resource set.
- `dist/experimental/services/agent-controller.d.ts`: prompt, steer, follow-up,
  next-run, abort, compact, and navigate facade.
- `dist/experimental/services/transcript.d.ts`: coherent transcript snapshot and
  non-replayed source event.
- `dist/experimental/services/sessions.d.ts`: directory, create, attach,
  detach, and remove.
- `dist/experimental/plugins/package.d.ts` and
  `examples/plugins/pi-example-plugin/`: Session/TUI Chord plugin model.
- `dist/experimental/process.js`: detached coordinator/server/worker process
  model.

Dependency package evidence was inspected at:

- `/tmp/tmp.4NojNkNHLy/node_modules/@earendil-works/pi-client/README.md`;
- `/tmp/tmp.4NojNkNHLy/node_modules/@earendil-works/pi-client/dist/*.d.ts`;
- `/tmp/tmp.4NojNkNHLy/node_modules/@earendil-works/pi-server/README.md`;
- `/tmp/tmp.4NojNkNHLy/node_modules/@earendil-works/pi-server/dist/*.d.ts`.

Those READMEs define the transport-neutral client, explicit reconnect rule,
Unix discovery, routed Session host, multi-presentation attachment, durable
routing tuple, and application-owned authentication policy.
