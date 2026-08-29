# Give an unattended job an ambient signal again

- STATUS: CLOSED
- PRIORITY: 40
- TAGS: desktop, service, protocol

## Source

Split out of `20260827-205340` (finding M5), which removed the signal
rather than routing it. This task is the routing, done as its own
increment. The removal is in the Unreleased "Removed" block.

## What was there

Control protocol v2: the workflow extension emitted
`scufris:attention-state` with `attention`, `error`, or `clear`, the
`desktop` extension subscribed, and the tray painted wisteria with
"Scufris needs you" (`tray.rs:55,104`). The subscriber went with the
inversion.

## What is there now

The job still reaches the person. `blocked` and `failed` wake the
conversation in every wake mode (`workerEventWakes`), so Scufris says
it in words. What is missing is the ambient copy: a signal that stays
up until it is dealt with, which is what a tray is for.

## Why it is not a `ScufrisState`

The service state vocabulary is `starting`, `idle`, `working`,
`detached`, `error`, and it answers one question: what is Scufris doing.
Attention answers a different one: what is waiting for you. The two
have separate lifetimes, so a sixth variant would fight the first five -
the agent is `working` while a job is blocked, and one of them would
have to win.

The pill already models it as separate and gets it right:
`Companion::tray_state` (`state.rs:941`) overrides the assistant state
with `attention` from its own local phase, and `resting_state` is only
consulted when nothing local is waiting.

## Shape to build

A notice channel of its own, not a state variant:

- A service message carrying a raised or cleared notice, with an
  identifier so a job can clear its own without clearing another's.
- The service holds the open set and replays it to a frontend that
  connects, the way it replays the transcript.
- `Companion` merges it into `tray_state` beside `Phase::Retained`,
  which is the existing precedent for "something is waiting".
- The workflow extension raises and clears it. The signal it needs is
  the deleted `workerAttentionSignal`: `blocked` raises, `failed` is an
  error, anything else clears. Recover it from git history rather than
  designing it again.

Decide whether a notice belongs in the conversation window too. The
tray is the ambient surface; the window is the one with room for the
detail.

## Proof

- A service test that a notice raised before a frontend connects is
  replayed to it.
- A `Companion` test that a raised notice shows `attention` and that
  clearing it returns the tray to the assistant state.
- An end-to-end check that a blocked job reaches the tray.

## Decision

Notices are tray-only. Their detail goes to the tray tooltip and status item,
not to the pill or conversation window. The wake response already puts the
same event in the conversation.

Local recording, transcription, and companion failures keep precedence. Among
open job notices, `error` wins over `attention`. A service `welcome` starts a
complete notice replay, so the companion first clears its old local set.

## Delivered

- Protocol v3 carries identified `notice` updates with `attention`, `error`, or
  `clear`.
- The service keeps the open set by job identifier and replays it to a frontend
  that connects.
- Workflow events restore the prior mapping: `blocked` raises attention,
  `failed` raises error, and ordinary progress or completion clears that job.
- The desktop link and companion compose notices into an independent tray state
  and detail.

## Verification

- `npm run typecheck`: passed.
- Focused TypeScript agent and service tests: 26 passed.
- `cargo test --workspace`: 389 passed.
- Focused Prettier check and `git diff --check`: passed.
- Full `npm run check`: project tests reached 90 passed and one unrelated
  environment failure because the installed Pi store path lacks
  `dist/modes/interactive/theme/dark.json`.
- Full `npm run format:check`: blocked only by the unrelated untracked
  `tasks/20260828-232631/TASK.md`; every file changed for this task passes the
  focused Prettier check.
