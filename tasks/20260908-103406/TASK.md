# Draw receipts, offers, and live job rows in the conversation HUD

- STATUS: CLOSED
- PRIORITY: 70
- TAGS: desktop, ux

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

Settled on 2026-09-09. `DESIGN.md` beside this file holds the decisions, the
protocol shape, and what was rejected. In short:

- Add an optional `receipts` field to the final response, grouped by job. Each
  group draws as one wrapping rank of badges at the foot of the message, led
  by the job id. Offers live inside the group and draw as the one control.
- Every badge comes from the helper's receipt. Four states: `measured`,
  `refuted`, `claimed`, `unknown`. The model writes only the offer.
- Taking an offer sends an id. The host submits the prompt the extension
  composed. No user line appears; the badge marks itself spent, recorded with
  the conversation entry.
- Replace the aggregate `agent.state` with `agent.jobs`, a list of rows read
  from durable state and replayed on connect. A row outlives its job and holds
  until it is filed. Live rows cancel behind an arming `x`; terminal rows
  clear. The tray word is folded from the rows on the host.
- The list is the last item in the conversation flow, hidden when empty. Keep
  the HUD minimal: no diff viewer, no transcript, no control but the offer and
  the row controls.

## Verification

- Test: a final response with two citations, four badges, and one offer
  validates; a citation with seven badges or three offers does not.
- Test: `offer.take` submits the stored prompt and adds no user entry, and a
  second take of the same offer id is refused.
- Test: two live jobs produce two rows on connect; both finishing leaves two
  terminal rows, and archiving both leaves zero.
- Test: the tray word stays `failed` while a failed row is unfiled.
- `cargo test` for the desktop and service, `npm run check`, one staging run
  with a job in flight.

Protocol 6 surfaces cannot ignore the new fields: `read_exact`
(`shared/control/src/service.rs:422`) rejects any version but its own. Desktop,
iOS, and `scufris-ctl` ship together at version 7.

### Verified, 2026-09-09

Built at protocol version 7 across the Rust service, the Pi extension, the
desktop HUD, the iPhone app, and the jobs helper. `VERIFY.md` beside this file
holds the commands, their results, and the test that stands behind each line
of the verification above.

What is left needs a machine this cannot reach: the iOS sources are not
compiled, because no Swift toolchain is installed here, and there is no staging
run with a job in flight. The Swift is held to the same wire shape by
`surfaces/ios/Tests/ProtocolTests.swift` and the Rust validator; a run with a
real worker is the one thing only a rebuilt desktop can produce.
