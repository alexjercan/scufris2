Use `/pair` and work directly on `master` in `/home/alex/personal/scufris2`.
Continue tracked task `20260828-232631` and implement the approved protocol v4
core end to end. Read these first:

- `/home/alex/AGENTS.md`
- `/home/alex/personal/scufris2/AGENTS.md`
- `tasks/20260828-232631/TASK.md`
- `tasks/20260828-232631/PLAN-PROTOCOL-V4.md`
- `.agents/skills/pi-extension/SKILL.md`
- `.agents/skills/helpers/SKILL.md` when deterministic helpers are involved
- the installed Pi extension documentation required by the Pi skill, completely

Treat `PLAN-PROTOCOL-V4.md` as the approved architecture. Do not redesign the
protocol and do not preserve v3 behavior through adapters.

Implement through the local synthetic multi-surface proof and the existing
local desktop. Do not add the TCP gateway, laptop deployment, iOS project, or
inference repository in this pass. Those are later product batches.

Non-negotiable decisions:

1. Replace protocol v3 outright. There is no legacy `service.sock`, v3 parser,
   fallback, conversion, dual protocol, compatibility listener, or stable
   cross-version response.
2. Use three mode-0600 Unix sockets: `surface.sock`, `agent.sock`, and
   `control.sock`.
3. Accept only the exact protocol version. Log every wrong `v` and close without
   writing a response. Clients render a local update-host-and-surface-together
   message after handshake failure.
4. Retain many registered surfaces, exactly one agent, and a minimal local
   control channel. There is no watcher.
5. Bind stable surface identity during `surface.hello`. Later requests derive
   identity from the connection. Re-registering the same ID replaces only its
   old generation.
6. Keep only the latest 200 canonical conversation messages in the service and
   every UI.
7. After `surface.hello`, send replay messages, current state, and finally
   `surface.ready` under one ordering boundary. Before ready, a client stores
   replay but performs no speech, animation, or widgets.
8. Broadcast every canonical user and assistant `surface.message` to every
   registered surface. It uses LLM-style `role`, associated `surface`, plain
   `text`, optional Markdown `details`, and optional widget calls.
9. The latest accepted surface message owns the next response association. A
   cross-surface steer moves that association.
10. Speech is frontend-only. There is no speech message, capability, route, or
    audio state in the service. A ready surface may speak a live assistant
    message only when `message.surface` equals its own registered ID.
11. The agent channel is the only prompt ingress. Remove the RPC prompt road,
    side-channel context queue, context acknowledgement, and dynamic widget
    activation.
12. Every `agent.message` contains the original text and the selected surface's
    registered widget definitions. The Pi extension constructs one
    self-contained user message with `<widgets>` and `<user_message>` inside
    `<scufris_surface_message>`, then calls `pi.sendUserMessage()`. Use
    `deliverAs: "steer"` while Pi is busy. Serialize deterministically and JSON
    escape user-controlled text and widget data, including XML delimiters.
13. Rebuild the response extension around one atomic `agent.response` with
    mandatory bounded plain `text`, optional bounded Markdown `details`, and an
    optional bounded `widgets` list shaped like LLM tool calls.
14. Widgets are synchronous with the final response and best-effort. Remove
    standalone open/update/close messages, widget results, acknowledgements,
    and asynchronous updates. Validate names and arguments against the selected
    surface registration. Only the associated live surface executes calls;
    replay never does.
15. Remove separate `said` and `speak` roads and replace ordinary `/detail`
    artifact retrieval with details carried in the shared response and replay.
16. Remove the agent-controlled conversation-window protocol. Surface-local HUD
    controls remain surface-local.
17. Broadcast one server state using severity-first precedence:
    `failed > blocked > working > starting > idle`. Listening, transcribing, and
    speaking remain local presentation states.
18. Keep control v4 minimal: `hello` and `state`, with `ready`, `state`, and
    `rejected` responses. Do not add watch, abort, debug, events, or hijack.
19. Preserve user-facing executable and package names, flake outputs, systemd
    units, and installed resource behavior except where the approved v4 socket
    and protocol replacement explicitly changes them.

Implementation method:

- Inspect the current tree, tests, and worktree before editing.
- Record implementation decisions and verification evidence in
  `tasks/20260828-232631/TASK.md`.
- Implement in narrow tested batches: shared protocol types and bounds; three
  listeners and channel rejection; registered replay/broadcast; agent ingress
  and response rewrite; desktop adaptation; Nix/resources/docs cleanup.
- Remove replaced v3 code and tests instead of leaving compatibility branches.
- Preserve bounded queues and generation-safe client removal.
- Add focused tests for every required case listed in the plan before broad
  checks.
- Run the cheapest checks per batch. Before completion run `cargo test
  --workspace`, `npm run check`, relevant focused Nix builds/checks, and then
  `nix flake check` if the focused checks pass.
- Use `nix run .#staging -- up` only when the implementation is ready for the
  coordinated staging deployment. Do not preserve the currently running v3
  stack.
- Continue through mechanical work and approved decisions without asking for
  confirmation. Stop only if a new choice can materially change the approved
  outcome.

Completion requires the local desktop and at least two synthetic registered
surfaces to prove identical replay and live conversation, latest-sender
association, no replay effects, generation-safe replacement, strict channel and
version rejection, atomic details/widgets, frontend-local speech behavior, and
one accepted agent.
