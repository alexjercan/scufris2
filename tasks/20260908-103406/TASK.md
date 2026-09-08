# Draw receipts, offers, and live job rows in the conversation HUD

- STATUS: OPEN
- PRIORITY: 70
- TAGS: desktop,ux


## Goal

The conversation HUD shows what the helper measured and what Alex can start,
without becoming an agent IDE. Three additions: a `receipts` field on the
final response drawn as a facts row under the prose; an `offers` field drawn
as one control that starts a task; live job rows in the conversation window.
The tray and the pill stay Scufris' own state.

Origin: research task 20260908-003207, candidates 4 and 5. Depends on task
20260908-103402 for receipts and task 20260908-103403 for offers. The jobs widget is optional and
comes after the rows, if at all.

## Facts

- The final response carries prose, optional Markdown details, attachments,
  and up to eight widget calls (`agent/extensions/scufris/response.ts:131-166`).
  Protocol validation caps widgets at 32
  (`agent/extensions/scufris/service/protocol.ts:11`,
  `shared/control/src/service.rs:33`).
- Surfaces receive one aggregate job state, `failed`, `blocked`, or
  `clear`, with one detail string
  (`agent/extensions/scufris/service/index.ts:50-75`,
  `host/service/src/service.rs:84-95,452-456`). The desktop paints the tray
  from it (`surfaces/desktop/src/tray.rs:55,104`); iOS maps `blocked` to a
  notice (`surfaces/ios/Sources/ContentView.swift:1134-1135`).
- `scripts/scufris-jobs all --json` lists id, state, live, project,
  workspace, worker, and summary.
- The desktop runtime is slot based: a shelf of three exhibits and four edge
  slots for pinned instruments (`surfaces/desktop/src/widgets/runtime.rs:28,36-41`).
  A fifth pin fails with "every instrument slot is taken" (`:572-573`, `:944`).
  By design; not in scope here.
- Widgets feel clunky to Alex: opened after the turn, mouse-first, and
  presentation only. Widgets stay citations that support a claim.

## Direction

- Add optional typed `receipts` and `offers` fields to the final response
  and the protocol (next protocol version). `receipts` holds helper facts
  only, bounded like `facts` in a briefing. `offers` holds an agent name
  and a prompt. A surface that does not know a field ignores it.
- Desktop and iOS draw receipts as a facts row under the prose, in the
  briefing page's style, and draw an offer as one control that submits the
  stored offer.
- Replace the aggregate job notice with a job list frame read from durable
  state: id, project, state, age, last verified fact, and the worker's
  summary labeled as what it says. Replay on connect. Draw it as a row or
  side panel in the conversation window, hidden when empty.
- Keep the HUD minimal. No diff viewer, no transcript, no buttons beyond the
  offer.

## Verification

- Test: a final response with two receipts and one offer validates, and a
  surface on the previous protocol version ignores the fields.
- Test: two live jobs produce two rows on connect and zero rows after both
  finish.
- `cargo test` for the desktop and service, `npm run check`, one staging
  run with a job in flight.
