# Never lose an answer to a surface that left

- STATUS: CLOSED
- PRIORITY: 85
- TAGS: service,bug

## Goal

An answer nobody asked for reaches every connected surface, or it reaches the
one that asked. It never reaches nobody.

Alex lost a morning briefing to this. He had last spoken to Scufris from the
phone, the phone was disconnected by morning, and the briefing was refused
rather than shown on the PC that was connected the whole time.

## Facts

- `associated_surface` is set every time any surface sends a message
  (`host/service/src/service.rs:365`) and is never cleared.
- On a final response (`service.rs:471-486`): with no association the answer
  goes out as `unprompted`, broadcast to every surface with widgets stripped.
  With an association whose surface is not connected, the whole response is
  rejected with `surface_unavailable` and reaches nobody.
- The comment above that block already states the wanted behaviour: "An answer
  nobody asked for - a morning briefing, a finished job - can reach a service
  no surface has spoken to yet. It is displayed rather than refused... so every
  screen shows it and none speaks it." The condition tests whether any surface
  has ever spoken, not whether this answer was asked for.
- `control_wake` leaves the association alone on purpose, documented at
  `service.rs:527-533` and asserted by `a_wake_leaves_the_response_association_alone`
  (`:1186`). That decision is the root cause.
- A surface speaks only what is attributed to it:
  `message.surface == local_surface` (`surfaces/desktop/src/main.rs:283-284`).
  So an unprompted answer is shown everywhere and spoken nowhere, which is what
  a briefing wants.
- `UNPROMPTED_SURFACE` is already in the protocol
  (`shared/control/src/service.rs:28`). No `SERVICE_VERSION` change, so no
  surface has to move with this.

## Direction

Two changes. The second stands on its own and is worth landing even if the
first turns out to be delicate.

### A wake starts an unprompted turn

The association belongs to the turn that produced the answer, not to the
session. A wake is words from outside the agent process, so the answer it
produces was asked for by nobody and is `unprompted` however recently a surface
spoke.

The care is a turn where the owner speaks and a wake lands before the answer.
Decide which one owns that answer and write the reason down; do not leave it to
whichever happened to set the field last.

`a_wake_leaves_the_response_association_alone` asserts today's behaviour. Its
intent inverts rather than disappearing: the case worth keeping is that a wake
does not steal a _later_ owner turn's association.

### A missing surface falls back rather than refusing

An association whose surface is gone becomes `unprompted` instead of
`surface_unavailable`. Losing an answer outright is the worst of the available
outcomes, and this covers finished jobs and anything else proactive rather than
briefings alone.

Widgets are stripped on that path, as they are for anything unprompted: a
widget belongs to the surface that asked and there is no such surface here.

## Verification

- Test: a surface speaks, disconnects, and the next answer reaches the
  remaining surfaces as `unprompted` rather than being rejected.
- Test: an answer produced by a wake is `unprompted` even though a surface
  spoke earlier in the session.
- Test: an ordinary owner turn still routes to the surface that asked, with its
  widgets intact.
- Test: an answer to an owner turn whose surface left carries no widgets.
- `cargo test`, and one staging run with a briefing wake and no phone
  connected.

## Landed, 2026-09-08

The association is now a property of one turn instead of the session. It opens
on an accepted surface message and closes when that turn ends: the answer that
is recorded, or an accepted abort. Nothing else touches it.

That is what change 1 asked for, reached from the other end. `control_wake` is
untouched and its documented promise - a wake neither sets nor changes the
association - is now literally true rather than the root cause, because there
is nothing left for a wake to inherit. The session-long association was the
bug; the wake only exposed it.

The tie the Direction asked to decide: **the owner wins**. If Alex speaks and a
briefing wake lands before his answer, that answer is still his, spoken by his
surface. The wake's own answer follows it and is `unprompted`, because the
first answer closed the turn. He asked first and is waiting; a briefing is not.
Under the alternative - clearing on the wake - his question would come back
silent on every screen to spare a briefing an attribution nobody hears.

An abort clears the association for the same reason a response does: the turn
ended, and nobody is owed an answer. Without that a cancelled turn would leave
the old association standing and reintroduce the bug on that path.

A refusal does not clear it. `invalid_widgets` and `attachments_unavailable`
are not answers, and the agent may correct the response and still reach the
owner it was refused against.

Change 2 landed as written: `surface_unavailable` is gone from the codebase. An
answer whose owner disconnected before it arrived is recorded as `unprompted`
with widgets stripped, so it reaches the screens that are present.

### Evidence

- `host/service/src/service.rs`: `surface_message` opens the turn,
  `surface_abort` closes it, the `Response` arm closes it and falls back, and
  the `control_wake` docstring states the turn rule.
- New tests: `an_answer_to_a_surface_that_left_reaches_the_others_unprompted`,
  `an_answer_to_a_surface_that_left_carries_no_widgets`,
  `a_refused_answer_leaves_the_turn_open_for_a_corrected_one`.
- `a_wake_leaves_the_response_association_alone` became
  `a_wake_owns_its_answer_only_when_no_owner_turn_is_open`. Both of its old
  legs still assert what they asserted; a third leg holds the fix, where the
  owner has spoken, been answered, and a later wake is still `unprompted`.
- `latest_sender_and_atomic_widgets_are_associated` and
  `a_cross_surface_steer_moves_the_response_association` are unchanged and
  still pass: an ordinary turn routes to the surface that asked, widgets
  intact.
- `cargo test --workspace` 321 + 42 + 19 pass. Clippy with `-D warnings`,
  `cargo fmt --check`, and `npm run check` are clean.
- Docs: `docs/src/dev/service.md`, `surfaces.md`, `messaging.md`, and the
  briefings section that promised the old behaviour by name.

### Left open

- The staging run with a real briefing wake and no phone connected. It needs
  the deployed binaries, so it belongs with the switch to the next release. No
  `SERVICE_VERSION` change, so no surface has to move.
- An agent process that dies mid-turn leaves the association standing. That is
  deliberate: the session is resumed on restart and the answer to that turn may
  still arrive. If it never does, the next answer is misattributed once.
