# Route or retire what the inversion left behind

- STATUS: OPEN
- PRIORITY: 80
- TAGS: orchestration,service,protocol

## Source

Review round 1 of `20260827-081702`, range `185034a..a13cb38`. Findings
M5 and M6. Full record: `tasks/20260827-081702/REVIEW.md`.

## M5. The `attention` state has no consumer

`orchestration.ts:211` still emits `ATTENTION_STATE_EVENT`. At `185034a`,
`desktop/index.ts:194` subscribed and reported it, and the tray painted
it wisteria with "Scufris needs you" (`tray.rs:55,104`). That file was
deleted in the inversion and nothing replaced the subscription. The
three surviving references are the constant, the import and the emit.

`ScufrisState` has no `attention` variant. `assistant-state.ts:6` defers
it to "the textbox increment", which has since landed without it.
`CHANGELOG.md:202` advertises the state in a released section, and the
Unreleased "Removed" block says nothing about its loss.

The tray's `attention` path is still reachable through `Phase::Retained`
(`state.rs:948`), so the state is not dead - only the blocked-job route
to it.

A blocked job therefore never reaches the person. Either route the event
through the service to the frontend and restore the tray signal, or
remove the emit and record the removal in the CHANGELOG. Routing it is
the behaviour the product had; choose deliberately and say which.

## M6. A sixth wire refusal code lives outside the module

`refusal` (`scufris-control/src/service.rs:294`) is documented "Stable
refusal codes. A caller branches on these" and holds five. `no_frontend`
is a private `const` in the service binary (`service.rs:76`), sent from
four places. Both TypeScript consumers branch on it
(`service/client.ts:34`, `conversation.ts:71`) and three tests pin it on
the wire. `docs/src/dev/service.md:63` enumerates the same five and
mentions the sixth only in prose further down.

An author of a new frontend or control client who enumerates the module
to cover every refusal will not handle it.

Move `no_frontend` into the `refusal` module beside the other five,
update the four send sites, and list it in `dev/service.md:63`.

## Proof

- `cd native && TMPDIR=/tmp nix develop --offline -c cargo test
--workspace`, and `TMPDIR=/tmp npm test`.
- For M6, the three existing wire tests should keep passing unchanged;
  that is the point of the move.
- For M5, if the event is routed: an end-to-end check that a blocked job
  reaches the tray. If it is removed: the CHANGELOG entry is the
  evidence.
