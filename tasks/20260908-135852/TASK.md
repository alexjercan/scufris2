# Add an unprompted wake ingress so a process outside the agent can reach the foreground

- STATUS: OPEN
- PRIORITY: 100
- TAGS: workflow,service

## Goal

A process outside the agent can wake Scufris proactively, with words, without
looking like the owner typed them. This is what a systemd timer, a finished
collection, or any later out-of-process event needs to reach the foreground.

Origin: task 20260908-103403, split. This is the gate for the briefing
scheduler (task B) and is useful on its own.

## Facts

- Nothing outside the agent process can carry words to it today. The only
  ingress is `surface.message` on the surface socket
  (`shared/control/src/service.rs:156-168`), which becomes a user turn: it is
  recorded in the conversation and echoed to every surface.
- `scufris-ctl` carries no words. Four window verbs and `state`
  (`host/service/src/bin/scufris-ctl.rs:22-33`), over the control socket.
- A proactive wake today is `pi.sendMessage(..., { deliverAs: "followUp",
triggerTurn: true })` inside the agent process
  (`agent/extensions/scufris/briefing/briefing.ts:135-152`,
  `workflow/orchestration.ts:222-250`).
- Answer attribution already works for both cases. The service associates an
  answer with the surface that sent the last message and falls back to the
  reserved `unprompted` name when there is no association
  (`host/service/src/service.rs:475`, `UNPROMPTED_SURFACE` at
  `shared/control/src/service.rs:28`). A wake must not disturb that
  association.
- `SERVICE_VERSION` is 5 in both implementations
  (`shared/control/src/service.rs:15`,
  `agent/extensions/scufris/service/protocol.ts:3`).

## Direction

- Add one control verb, `control.wake`, carrying text and optional details.
  The control socket, not the surface socket: this is not a second way to
  drive the conversation, and it must stay unreachable from the remote surface
  gateway.
- The service forwards it to the agent connection as its own kind, distinct
  from `agent.message`. It is not appended to the conversation as a user turn,
  it is not echoed to surfaces, and it does not set or change the surface
  association. What the agent answers is attributed by the existing rule.
- The extension turns it into the same `pi.sendMessage(..., followUp,
triggerTurn)` a wake uses today, with a `customType` the caller names, so an
  existing wake handler keeps working unchanged.
- Bound it like every other text: `MAX_TEXT_BYTES` and `MAX_DETAILS_BYTES`.
- Refuse with a code, never silently, when no agent is connected. A caller
  that cannot wake the foreground must be able to tell, so its own durable
  state stays the fallback.
- `scufris-ctl wake` sends one. Protocol version goes to 6 in both
  implementations together.

## Verification

- Test: a wake reaches the agent, is not recorded as a user message, and is
  not echoed to any surface.
- Test: a wake does not change the surface association. An answer before the
  owner has spoken is still recorded against `unprompted`; an answer after is
  still recorded against the surface the owner last used.
- Test: a wake with no agent connected is refused with a code and a detail.
- Test: text and details over the bounds are refused.
- Test: the remote surface gateway cannot send a wake.
- `cargo test`, `npm run check`, and `nix flake check`.
