# Nightly review

- STATUS: CLOSED
- PRIORITY: 70
- TAGS: review

## Scope

Nightly review of what landed on `master` on 2026-09-09. Read-only: no edit,
no commit, no push, no release, no Sprout. The report is the whole of the
work.

29 commits landed since midnight (`db32817`..`bbaabff`). Three more landed on
2026-09-08 after the previous briefing asked at 23:00:15, so they were never
covered: `315b931`, `dd929ec`, `78e0145`. They are folded into G1.

Whole window `315b931~1..bbaabff`: 124 files, 9772 insertions, 929 deletions.
That is 10701 changed lines, above the `/scufris-review` cap of 10000, so the
day must be split.

## Baseline checks on master

Run before any review, at `bbaabff` with a clean tree.

- `npm run check`: typecheck and 121 node tests pass. `format:check` fails on
  exactly one file, `tasks/20260909-230022/TASK.md`, which is the file `tatr
new` created for this run. Every other `tasks/*/TASK.md` passes. Not a
  master failure.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 376 tests, 6
  failures, all in `tests/test_briefing.py`.
- The same suite with `SCUFRIS_BRIEFING_*` unset: 376 tests, OK.

So master is green. The 6 failures are the ambient environment leaking into
the tests. See F0.

## Groups

Chronological and contiguous, one component to a group where the day allows
it. Reviewed highest risk first, not in date order.

| ID  | Range                | Commits | Changed | Component                            |
| --- | -------------------- | ------- | ------- | ------------------------------------ |
| G1  | `315b931~1..997e057` | 10      | 2535    | extension, host service, jobs helper |
| G2  | `997e057..19ce35a`   | 3       | 898     | desktop surface, den helper          |
| G3  | `19ce35a..474a7fa`   | 4       | 894     | refusal codes, release, jobs         |
| G4  | `474a7fa..353198d`   | 7       | 1787    | briefing pipeline, widgets           |
| G5  | `353198d..31a877c`   | 2       | 5488    | HUD receipts, offers, job rows       |
| G6  | `31a877c..bbaabff`   | 6       | 352     | packaging, release, 2.4.0 and 2.4.1  |

Review order: G5, G1, G4, G2, G3, G6.

## Findings

### F0 MAJOR: the briefing tests inherit the ambient briefing bounds

`tests/test_briefing.py` does not clear `SCUFRIS_BRIEFING_*` from the
environment. `tools/briefing/briefing.py` reads them in
`environment_seconds` and `environment_int` (lines 972-986, 1216-1257), so a
test that passes its own `source_deadline`, `max_body` or `max_offers` is
silently overridden by whatever the shell already exports.

Measured this run: 6 failures with the vars set, 0 with them unset.

- `Run.test_a_run_records_the_bounds_it_was_actually_given` got
  `source_deadline 28800.0, run_deadline 28800.0` where it asked for
  `900.0 / 1800.0`.
- `Command.test_the_prompt_carries_the_project_guidance_and_the_shape` and
  `Run.test_a_source_is_told_what_it_may_offer` got the default `65536` and
  `8` in the prompt instead of the profile's `16384` and `3`.
- `Bounds`, `Envelope` and `Offers` each expected `briefing.Unusable` for an
  over-bound answer and got no refusal, because the ambient bound was higher
  than the one the test set.

Why it matters: the nightly timer unit exports exactly these variables into
every source process. A source told to run the project's checks, which is
what this night was told to do, reads its own test suite as red. It cost
this run a full diagnostic pass to prove master was green.

### G5 `353198d..31a877c` - HUD receipts, offers, job rows

Dispatched 5 lanes: craft, correctness, desktop, contracts, red team. No
`--live`, no Feel lane, no X display: the night is unattended and the desktop
lane was told to report its judgement as unharnessed.

### G6 `31a877c..bbaabff` - packaging, release, 2.4.0 and 2.4.1

Reviewed in this session rather than by a panel. 352 changed lines, of which
Cargo.lock and package-lock.json are 10, and the rest is packaging, RELEASE.md
prose, and the filed-row persistence in `orchestration.ts`.

Verified by reading:

- `nix/python.nix` is new and five paths take the interpreter from it:
  `nix/staging.nix:29`, `nix/launcher.nix:35`, `nix/briefing.nix:6`,
  `nix/dev-shell.nix:3`, `nix/checks/launcher.nix:15`.
- `.github/workflows/check.yml` now runs `nix develop -c npm run check`, and
  `nix flake check -L` still runs `nix/checks/helpers.nix`, which is where the
  376 Python tests are gated. CI therefore does run them, in a sandbox with no
  ambient `SCUFRIS_BRIEFING_*`. F0 does not reach CI.
- `session_environment()` in `tools/jobs/scufris-jobs` pins the six locating
  variables onto `tmux new-session -e`. `scripts/scufris-staging:234-241`
  exports `XDG_STATE_HOME` and `XDG_DATA_HOME`, so the staging case the fix
  was written for is actually covered. Not a finding.
- `filedRowsFromEntries` takes the last whole set rather than a union, and the
  restore at `orchestration.ts:1712` runs after recovery so a filed id for a
  landed job is dropped. Correct as documented.

#### F1 MINOR: `nix/checks/helpers.nix` still builds its own Python

`nix/checks/helpers.nix:11` keeps
`pkgs.python3.withPackages (p: [p.markdown-it-py])` instead of importing
`../python.nix`. It is the only path left that does.

`nix/python.nix:1-9` states the invariant it was written for: "Every path that
reaches that code has to name this one ... a fourth path that forgets it fails
at the first import and only when a briefing runs." The commit that wrote it
converted five paths and missed the sixth.

Failure scenario: a helper gains a second Python dependency, it is added to
`nix/python.nix`, and `helper-tests` is the one gate that does not have it.
The break is loud rather than silent - the check goes red in CI - which is
why this is MINOR and not MAJOR. It is still the invariant broken on the day
it was declared.

#### Not a finding, noted

`RELEASE.md:7` step 3 now says `nix develop -c npm run check` and step 4 says
`npm run check` again "in `nix develop`". Two spellings of the same thing in
adjacent steps. Prose only.

#### F2 MAJOR: an offer the extension cannot honour disables its button forever and says nothing

Derived in this session from the tree, before the panel reported.

The chain:

- `surfaces/desktop/ui/hud.ts:341-352`. The click handler sets
  `button.disabled = true` and re-enables only in `.catch`.
- `surfaces/desktop/src/hud.rs:216-221`. `Hud::offer_take` returns `Ok` as
  soon as the backend accepts the message. Nothing is awaited, so the
  `invoke` promise resolves and the `.catch` never runs.
- `surfaces/desktop/ui/hud.ts:741`. `listen("scufris://offer-taken")` calls
  `spend(id)`, which is the only path that marks the button spent and
  re-enables it. It fires only when the extension honours the offer.
- `agent/extensions/scufris/response.ts:183-185`.
  `const prompt = offerPrompts.get(signal.id); if (!prompt) return;` - a
  silent drop. No `surface.offer_taken`, no refusal, no log line.

Failure scenario: a session writes more than `maxLiveOffers = 64` offers
(`response.ts:34`), which evicts the oldest at `response.ts:124-128`. Alex
scrolls back and presses one of the evicted buttons. The button goes grey and
stays grey, no notice is drawn, no follow-up turn starts, and nothing anywhere
says the press was dropped. It reads as accepted. The only way back is a
restart, and `session_start` re-applies the same 64 bound at
`response.ts:194-199`.

`response.ts:29-33` names the hazard - "An offer whose words are gone is a
button that does nothing, so the store is what keeps a button honest" - and
the eviction path is where the store stops keeping it honest. Disabling the
button on press makes it worse than inert: an inert button at least still
looks pressable.

Not a BLOCKER because it needs 64 offers in one session and the ordinary
press works. The fix is on either side: refuse an unknown offer id back to
the surface, or do not disable until `offer-taken` arrives.

The iPhone app takes the same `surface.offer_taken`
(`surfaces/ios/Sources/ConversationStore.swift:690`), so whatever is decided
has to cover it.

#### F2 corrected: the button is not the problem, the two bounds are

The paragraph above is wrong about the disabled button and is superseded here.
`host/service/src/service.rs:619-647` settles the press before the agent sees
it: `conversation.take_offer` marks it, `broadcast(OfferTaken)` goes to every
surface, and only then is the agent told. So the HUD button is spent and
re-enabled whatever the extension does, and an already-taken or
ring-dropped offer is refused properly with `offer_unavailable`
(`shared/control/src/refusal.rs:67`). Withdrawn.

What is actually wrong is worse.

The same offer is bounded twice, by two different numbers, in two places:

- The host holds `CONVERSATION_ENTRIES = 200` messages
  (`shared/control/src/service.rs:21`, `host/service/src/conversation.rs:94`),
  each able to carry several receipts and up to two offers each. Every offer
  in that ring is open until taken.
- The extension holds `maxLiveOffers = 64` prompts
  (`agent/extensions/scufris/response.ts:34`), and evicts the oldest at
  `response.ts:124-128`.

The host is the larger bound and it is the one that decides. So once a session
has written more than 64 offers:

1. Alex presses an offer the host still holds open.
2. `take_offer` returns `Ok(true)`. The host marks it taken and persists it.
3. `broadcast(OfferTaken)` - every surface draws the button spent and quiet.
4. `send_agent(OfferTake)` succeeds, because the agent is connected.
5. `response.ts:183-185` looks the id up, finds nothing, and returns.

No turn starts. No refusal is sent. No log line is written. The button says
taken, so the surface says the work began. And the offer is now permanently
spent in the host's ring, so pressing it again gets `offer_unavailable`: there
is no way back to the thing Alex asked for except typing it out.

`response.ts:29-33` names this hazard exactly - "An offer whose words are gone
is a button that does nothing, so the store is what keeps a button honest" -
and then sizes the store below the thing it is keeping honest.

The same shape reaches a restart without needing 64 offers: `session_start`
clears `offerPrompts` and restores only what the session branch still carries
(`response.ts:194-199`), while the host's conversation is persisted separately
and comes back whole.

MAJOR, not BLOCKER: it needs a long session, and the ordinary press works.
The fix is one line of reporting - the extension refusing an id it cannot
honour, so the host can un-take it or say so - plus making the two bounds
agree.

### Baseline, continued

`nix develop --command cargo test --workspace` at `bbaabff`: 414 tests, 0
failures (330 desktop, 51 service, 24, 9). Rust is green.

Not run tonight: `nix flake check`, by the reviewer contract. `ruff` was not
run. Both are skips, not passes.

### G5 findings, adjudicated

Five lanes reported. Every claim below was re-derived from the tree in this
session; anything a lane asserted that I could not ground is not here. Two
candidate findings were dropped by the lanes themselves because a later commit
today already fixed them: the in-memory `archived` set (fixed in `ae316e8`)
and the unclipped project grid cell (fixed in `6c76c61`).

**G5-1 BLOCKER - an offer the extension has forgotten is recorded taken
everywhere and nothing runs.**
`agent/extensions/scufris/response.ts:183-185`,
`host/service/src/service.rs:619-650`.
Found independently by all five lanes and by me before they reported.
The same object is bounded twice: `maxLiveOffers = 64` in the extension
(`response.ts:34`, evicting at `:124-128`) against `CONVERSATION_ENTRIES = 200`
messages in the host ring (`shared/control/src/service.rs:21`), each able to
carry up to 4 receipts of 2 offers. The host is the larger bound and the one
that decides. Press an evicted offer: `take_offer` returns `Ok(true)`, the host
writes `taken: true` into `conversation.json`, broadcasts `surface.offer_taken`
so every screen greys the badge, then relays to the agent, where
`offerPrompts.get(id)` misses and the handler returns. No turn, no refusal, no
log line. The mark is durable and a second press is refused with
`offer_unavailable`, so there is no way back to the thing Alex asked for.
The service starts Pi with `--continue`, so the session is long-lived and 64
offers is days, not an edge. `refusal::OFFER_UNAVAILABLE` exists for exactly
this state but the agent has no channel to send it: `AgentRequestBody` has no
rejection variant.

**G5-2 BLOCKER - the desktop discards every job and offer refusal, showing
nothing and logging nothing.**
`surfaces/desktop/src/link.rs:283-287`, `surfaces/desktop/src/conversation.rs:281`,
`surfaces/desktop/src/state.rs:830`.
`Rejected { id: Some(..) }` becomes `LinkEvent::Refused(id, detail)` with
`operation` dropped. `Conversation::refused` returns false unless
`self.sending == Some(id)`, and the state machine arm is guarded by
`if *id == failed`. A job id or an offer id is never the in-flight submission
id, so both return false; only the `id: None` arm logs. Press `x`, `sure?`, `x`
on a live row while Pi is restarting: the host answers
`agent_unavailable`, the desktop says nothing, and the row keeps reading
`work`. Alex believes the job is stopping.

**G5-3 BLOCKER - a job control loses the keyboard on every line said, and the
next Enter sends the composer instead.**
`surfaces/desktop/ui/hud.ts:485-490` and `:687-703`.
`tail()` calls `jobs.remove()` unconditionally before re-appending, and it runs
from `append()`, `replace()`, `setThinking()` and `list()`. Removing the
subtree that holds the focused element moves focus to `<body>`. The window
`keydown` handler then reads `owner.tagName === "BUTTON"` as false and falls
through to `event.preventDefault(); void send()`. So: tab onto a row's `x`,
Scufris says one line or a worker event fires `publishRows()`, press Enter, and
a half-typed composer line is submitted instead of the button being pressed.
`list()` additionally does `rows.replaceChildren(...)`, which destroys the
focused button outright. Unharnessed: no X display was brought up tonight, so
this rests on the DOM focus-fixup rule rather than an observed WebKitGTK run.
It is the one finding worth reproducing on the harness before fixing.

**G5-4 MAJOR - an offer is spent and broadcast before the service knows the
agent can run it.**
`host/service/src/service.rs:625-656`.
`take_offer` persists, `broadcast(OfferTaken)` greys the badge everywhere, and
only then does `send_agent` run. With Pi between restarts the take is durably
recorded, the badge is spent on every screen and across a restart, and nothing
was submitted. The only report is `Rejected { agent_unavailable }` to the one
surface that pressed it, which G5-2 discards. `surface_message`
(`service.rs:392-408`) already has the right shape: relay first, record after.

**G5-5 MAJOR - one over-long project id drops every job list from then on, and
the tray with it.**
`agent/extensions/scufris/service/client.ts:98-104`.
`jobs()` clamps `summary` through `jobSummary` but passes `project` through
raw. `checkJobRows` (`service/protocol.ts:319-322`) bounds `project` to
`MAX_IDENTIFIER_LENGTH` = 64 bytes and throws `ProtocolError` for the whole
message. `tell()` catches it and calls `this.log`, which under the service is
`console.error` only. `PROJECT_ID` in `tools/jobs/scufris-jobs:26` is an
arbitrary-depth relative path with no length bound. One such job exists and
every subsequent `agent.jobs` dies the same way: surfaces hold the last good
list forever, and a job that fails overnight never turns the tray red.
`jobSummary`'s own comment argues against exactly this - "Refusing the whole
row list would leave the surfaces showing nothing while a job is failed, which
is the wrong half to keep."

**G5-6 MAJOR - the model can write an offer label the encoder refuses, and the
whole answer is lost.**
`agent/extensions/scufris/response.ts:333` against
`agent/extensions/scufris/service/protocol.ts:306`.
The tool schema bounds `label` to 48 _characters_; the wire bounds it to
`MAX_BADGE_BYTES` = 64 _bytes_. Any multi-byte script crosses that inside the
schema; the correctness lane confirmed it with a probe against the real module
("offer_label is invalid"). `encodeAgentRequest` throws for the whole
`agent.response`, `tell()` logs one line, and the tool has already returned
`terminate: true`, so the turn is over and no surface ever receives the answer.
Nothing between the model and the encoder clamps the label the way
`jobSummary` clamps a summary.

**G5-7 MAJOR - the `x` on a review job's row can never stop it.**
`agent/extensions/scufris/workflow/orchestration.ts:1018-1026` against
`tools/jobs/scufris-jobs:3741-3744`.
`runJobCommand` sends the pressed row's own id to `stop`, while the line above
it already resolves `job.root_job` for `closeWorkflowSurfaces`. The helper
refuses any non-root stop: "stop is workflow scoped; stop the workflow root".
A job spawned with `review_of` shares its source's `root_job`, is put in the
`jobs` map at `:1261` and published, so it is drawn with a live `x`. Every
press arms, fires, and fails. The catch then wakes the model with "The job is
still running", steering it to report a dead end rather than to stop the root
the error names.

**G5-8 MAJOR - a repeated drain failure after a second session start reports to
nobody.**
`agent/extensions/scufris/workflow/orchestration.ts:819-827` and `:1685-1689`.
`session_start` resets `archived`, `drainFailedAt`, `wakeMode`,
`acknowledgmentGate` and `deliveredEventIds`, but not `eventError`,
`eventStranded` or `drainWakes`. `reportDrainFailure` short-circuits on
`if (message === eventError) return;` _above_ the line that sets
`drainFailedAt` and above `publishRows()` and the wake. So on a second
`session_start` in the same process, a drain failing with the same message
publishes no `event-drain` row and sends no wake. `ctx.hasUI` is false under
the service, so the `ui.notify` on the line above is not a report. This is the
exact failure the row was introduced for; `tests/structure.test.ts:74-77`
asserts the row's existence for that reason.

**G5-9 MAJOR - one refused press paints the whole iPhone surface failed, with
no words.**
`surfaces/ios/Sources/ConversationStore.swift:693-696` and `:70-73`.
`case "surface.rejected"` sets `serviceState = "failed"` unconditionally, and
this range changed `showsStatusNotice` to `return false` while `.connected`,
which is the only state a rejection can arrive in. So the detail
"offer_unavailable: That offer is no longer open." is never drawn and the
header simply reads failed until the next `surface.state`. Two screens showing
the same offer is the ordinary case the host explicitly expects.

**G5-10 MAJOR - a row's age freezes at whatever it was when the list last
changed.**
`surfaces/desktop/ui/hud.ts:367-373`.
`age(row.since)` is evaluated once in `drawRow`, `list()` runs only from
`scufris://jobs`, `scufris://conversation` and `hud_ready`, `Hud::jobs`
(`hud.rs:184`) returns early on an unchanged list, and the page's only timer is
the 3-second disarm. A job that finishes at 02:00 draws `3h` and still draws
`3h` at 09:00 - which is precisely the "still there in the morning" case the
change exists for.

**G5-11 MAJOR - `docs/src/dev/surfaces.md:138` promises something the code does
not do.**
"live rows are never dropped at the cap". `boundedRows`
(`orchestration.ts:307-320`) removes terminal rows first and then does
`kept.slice(-MAX_JOB_ROWS)`, which discards the oldest live rows once live rows
alone exceed eight. Its own comment says the real rule. Nothing caps concurrent
jobs. A ninth job that blocks is invisible on every surface and never reddens
the tray, because the tray word is folded from the published rows.

**G5-12 MAJOR - `docs/src/dev/messaging.md:56-67` documents a mechanism this
range deleted.**
The page still describes the durable `scufris:event-drain` notice, a drain
clearing it, land/stop clearing a job's own notice, and a recovered job raising
it again. A tree-wide grep outside `tasks/` finds no `attention-notice`,
`EVENT_DRAIN_NOTICE` or `AttentionNotice`. Six other mdBook pages were updated
in this range; this one was missed. It is also the page that enumerates the
ingress paths into a turn and does not mention the new `scufris-offer`
follow-up.

**MINOR, verified, worth folding in**

- `surfaces/desktop/ui/hud.ts:458` - `list()` calls `disarm()` and then
  `replaceChildren`, so any republish inside the 3-second window silently
  un-arms a stop. A working row's summary advancing is enough. Fails safe: the
  second press re-arms rather than stopping the wrong job.
- `orchestration.ts:274-287` - the drain row carries `since = drainFailedAt`,
  so it sorts newest, and `boundedRows` culls terminal rows: with eight live
  jobs the stranded-drain row is the first thing dropped. Past
  `MAX_DRAIN_WAKES = 3` the wake has stopped too, so nothing is left to report
  it. The correctness lane probed this against the real module.
- `response.ts:140-145` - `cite()` breaks at `MAX_CITATIONS` and then calls
  `pending.clear()` unconditionally, so badges past the fourth job are
  discarded rather than carried to the next answer. The worker events that
  produced them were acknowledged and are never redelivered.
- `nix/checks/service.nix:62` - "Protocol v6 control is diagnostic, window, and
  wake only." v7 shipped in this range.
- `agent/extensions/scufris/shared/job-rows.ts:15` - `export type { JobRow }`
  has no importers; its twin in `shared/citations.ts:20` has one, while
  `workflow/citation.ts:4` takes the same type from the protocol. One type,
  two import paths, no rule saying which.
- `surfaces/desktop/src/conversation.rs:158` - `offer_taken` searches
  `self.lines`, capped at `LINES = 200`, while the page's `<ol id="lines">` is
  never trimmed. An offer older than the desktop's own ring can never be marked
  spent. Neither `listed` nor `offer_taken` has a unit test, in a file whose
  other methods all have one.
- `surfaces/desktop/ui/hud.css:133` - `.line:first-child { margin-top: auto }`
  does not cover `#jobs`, so a job list with no conversation sits at the top of
  the window instead of above the field.
- `surfaces/desktop/src/main.rs:1055-1058` - the comment says an arrival at
  `hud_job_command` is already the second press. The capability grants
  `widget-*` `core:default` and the command is in the global invoke handler, so
  a widget page can cancel a running job with no arming.

**Raised by a lane and not carried:** the `pending` badge strip attaching to a
later unrelated answer. The comment at `orchestration.ts:323-324` says this is
deliberate and the strip is labelled with its job id, so it is a misplaced fact
rather than a false one.

### G1 `315b931~1..997e057` - extension, host service, jobs and briefing helpers

Dispatched 5 lanes. Includes the three commits of 2026-09-08 that landed after
the previous briefing asked and were never covered.

**G1-1 MAJOR - an answer to a surface press is spoken by nobody and opens no
window.**
`host/service/src/service.rs:409`, `:522`, `:563`;
`surfaces/desktop/src/main.rs:292-300`.
Verified in this session. `associated_surface` is written in exactly one place,
`surface_message` at `:409`. `surface_job_command` (`:584`) and
`surface_offer_take` (`:619`) relay a press to the agent and never open a turn.
The `Response` arm reads the association at `:522` and clears it
unconditionally at `:563` - "The answer closes the turn, whatever had to be
dropped from it to get here."

So: Alex presses an offer. `response.ts:187` sends the prompt as a follow-up
with `triggerTurn: true`, the turn ends in `scufris_final_response`, and the
answer arrives with `associated_surface = None`. It is recorded against
`UNPROMPTED_SURFACE`, `widgets` is forced to `None` at `:527`, and
`local_presentation` gates both `speak` and `widgets` on
`message.surface == local_surface`. The desktop prints the text, says nothing
aloud, and opens no panel the answer asked for. The same holds for the
failed-`cancel` turn at `orchestration.ts:1031-1044`, which exists precisely so
the model can tell Alex his job is still running.

This is a seam between two of today's commits: the one-turn association is
`dd929ec` in this group, the presses are `31a877c` in G5. Neither is wrong
alone. Both `surface_job_command` and `surface_offer_take` already hold the
speaking surface from `speaking_surface`.

**G1-2 MAJOR - a test and a document both assert a path that does not exist,
and the test passes anyway.**
`host/service/src/service.rs:1474-1512`, `docs/src/dev/service.md:92`.
Verified in this session. The document says "A refusal is not an answer and
leaves it open, so the agent may correct the response and still reach the
owner." The `Response` arm has no early return: the `invalid_widgets` refusal
at `:545` and the `attachments_unavailable` refusal at `:552` both fall
through, deliver the answer, and run `associated_surface = None` at `:563`.
There is no refusal that preserves the association.

`a_refused_answer_leaves_the_turn_open_for_a_corrected_one` is green because
`drain` (`:1025`) is a cumulative `try_recv` loop and `one` is never drained
between the two responses, so the `surface == "one"` assertion matches the
_first_ response's message. The second is in fact recorded `unprompted`. The
lane ran it: `cargo test -p scufris-service service::tests`, 21 passed, this
one green.

Real behaviour: the agent names an expired attachment, the answer is delivered
and closes the turn, the agent is told `attachments_unavailable` and sends a
corrected response - which arrives `unprompted`, so Alex sees the same answer
twice, the second time unspoken and stripped of widgets. A verdict that can
lie.

**G1-3 BLOCKER - a worker summary with a non-printable character that is not a
C0 control wedges the event drain for every job, permanently.**
`tools/jobs/scufris-jobs:2106`, `:2248`, `:1437`, `:2471`.
Found independently by the correctness and red-team lanes and reproduced a
third time in this session.

The range's headline fix (997e057, "Stop one worker line from wedging every
job") unified the _bound_ at every door and left the _character predicate_
split. `write_report` (`:2248`) and `parse_event` (`:2106`) both test
`any(ord(character) < 32 ...)`. `valid_record_text` (`:1437`), which
`store_job` runs on the summary it copies into the record, tests
`str.isprintable()`. Measured here against the tree at `bbaabff`, seven
characters pass the first two doors and fail the third:

```
U+00A0 NBSP      parse_event_accepts=True  record_accepts=False
U+200B ZWSP      parse_event_accepts=True  record_accepts=False
U+00AD SHY       parse_event_accepts=True  record_accepts=False
U+2028 LINE SEP  parse_event_accepts=True  record_accepts=False
U+FEFF BOM       parse_event_accepts=True  record_accepts=False
U+007F DEL       parse_event_accepts=True  record_accepts=False
U+0085 NEL       parse_event_accepts=True  record_accepts=False
```

The sequence: a worker calls `report` with a summary containing a non-breaking
space - which models emit, and which a summary quoted from captured output
carries - `write_report` admits it, `parse_event` admits it, and
`read_events:2471` calls `store_job` bare, outside any `try`. `JobError("job
record is invalid")` propagates out of `read_events`, so the whole `events`
call fails for _every_ job in that poll, `event_offset` never advances past
the line, and every later poll fails identically. `recover_job:3810` takes the
same path, so a restart recovers nothing either. Only a hand edit of `status`
clears it. The new drain machinery reports the strand honestly and then retries
the same poisoned read at every settle, forever.

`scufris-jobs:51-56` states the intent this misses in as many words: "One
bound, measured in bytes, at every door." One door has a different predicate.

Fix: make the three doors one predicate - `valid_record_text`'s own test - so a
summary the record will refuse is refused where it is written and classified
`invalid` where it is read. `tools/jobs/scufris-report` needs the same, since
it checks only bytes, CR and LF.

**G1-4 MAJOR - one bad attachment id or widget name drops the whole set.**
`host/service/src/service.rs:543`, `:551`.
`attachments.resolve(&attachments, true)` is all-or-nothing: the first id
missing from the index returns `NotFound` for the batch. `validate_calls`
returns `Err` on the first bad call and `:549` then sets `widgets = None` for
all of them. `CHANGELOG.md:128` says "The offending call or attachment is
dropped and the answer is recorded." Alex asks for five screenshots, one has
aged past `UNREFERENCED_RETENTION`, and he gets prose describing five and zero
attachments with nothing saying why.

**G1-5 MAJOR - the doc makes the `sleep`/`wait` guard load-bearing for
something it cannot do.**
`docs/src/dev/messaging.md:91-95` against
`agent/extensions/scufris/workflow/orchestration.ts:519-534`.
The gate was narrowed in `db32817` to refuse only `scufris_job_inspect`, and
the doc explains that narrowing's safety by saying the bash guard "is what
prevents a foreground poll loop". `foregroundCommandWaits` matches only `sleep`
or `wait` as the executable word of a segment. After a spawn, `timeout 120 tail
-f <status>`, `python3 -c 'import time; time.sleep(120)'`, `read -t 120`,
`bash -c 'sleep 5'` and `tmux wait-for` all pass it, and the foreground turn
blocks for two minutes with no way for Alex to reach or steer it - the exact
state the gate existed to prevent. Before this range the gate blocked every
tool after an action, so the guard's narrowness cost nothing.

**G1-6 MAJOR - `scufris_briefing_run` reports nothing for a re-collected
briefing, and the failure branch it added is reachable only in that case.**
`agent/extensions/scufris/briefing/briefing.ts:281-289`.
`collect` reuses the same date+profile directory and never removes
`briefing.md` (`tools/briefing/briefing.py:1039-1041`), so `collected_runs`
skips the fresh run at `:198`, `pending` returns nothing, `waiting` is
`undefined`, and `manifest.state` is `collected` - neither `wake(waiting)` nor
`reportRunFailure` fires. The tool answered `started: true` and promised the
run "wakes you when it is ready". Nothing arrives and nothing says why.
The new `else if (manifest.state === "failed")` guard never fires for a
genuinely failed run: `finish` (`:1109`) sets `failed` only when
`contributions` is non-empty, so `pending` includes it and `wake(waiting)`
carries `failure_message`. The only state that reaches the new branch is this
same already-published one.

**G1-7 MAJOR - two documentation pairs are stale where this range changed the
behaviour.**

- `docs/src/dev/jobs.md:160-162`: "older history is discarded and the new
  complete entry is kept" is no longer true - `trimmed_report` keeps the newest
  whole entries that fit under a `# report trimmed` marker. The Bounds list
  also omits the 500-byte `MAX_SUMMARY` this range added and enforces at three
  doors, so a worker refused for a 600-byte summary finds only the 4 KiB event
  line written down, which it is comfortably under.
- `docs/src/dev/jobs.md:243`, `:259`: land now runs the dry run _first_, then
  records the intent, then stops the graph (`scufris-jobs:3685-3687`) - that
  reordering is the whole fix, and the page still describes the trap it
  removed. `:259` also says only `abandon` may be re-decided; `stop` now
  excludes `remove_workspace` from the immutable comparison too.

**MINOR, verified**

- `tools/jobs/scufris-jobs:2192-2201` - `report_entries` splits on `\n# `, so
  an ordinary Markdown heading in a worker's report body reads as an entry
  boundary. Both lanes reproduced it. At the 2 MiB ceiling the trim can keep a
  headerless `# Findings` fragment and drop the `# working: ...` header that
  named its generation, and the marker's "N older entries dropped" counts
  fragments. Split on `^# (working|blocked|done|failed): ` instead.
- `host/service/src/attachment.rs:221-228` - `stat` maps a record whose object
  file is gone to `StoreError::Io` and thence to 500 `attachment_unavailable`
  rather than 404 `attachment_not_found`. `load` prunes exactly this case, but
  only at startup, so until a restart a surface with retry-on-5xx retries a
  file that will never exist. The test closes with `assert!(...is_err())`,
  which holds for any variant, so nothing pins the mapping.
- `host/service/src/attachment.rs:349-357` - a record the quota declines at
  open goes to `unindexed`, so its bytes never enter `index.bytes` and `expire`
  (which walks `index.records`) can never reach it. After one start at quota
  the store under-reports what it holds until a restart, and `read_dir` order
  decides which records win.
- `agent/extensions/scufris/service/protocol.ts:185` vs
  `shared/control/src/service.rs:595` - the emptiness tests differ for U+0085:
  JavaScript's `trim` does not treat NEL as whitespace, Rust's does. A field
  whose whole content is one NEL passes the encoder and is refused by the host,
  which answers an invalid submission by closing the agent connection with no
  refusal - the exact teardown this range's `bounded()` was written to prevent.
- `host/service/src/bin/scufris-surface-gateway.rs:454-460`, `:653-661` - two
  routes gained a 400 (`attachment_incomplete`, `audio_incomplete`) that the
  published OpenAPI `responses(...)` blocks do not list, so a generated client
  has no case for it.
- `host/service/src/bin/scufris-surface-gateway.rs:629`, `:689` -
  `attachment_incomplete` is written as a literal three lines below
  `refusal::ATTACHMENT_TOO_LARGE`, and `audio_incomplete` has no name in the
  refusal module at all. The "named once on both sides" test reads the module,
  so the gateway is outside that guard.
- `host/service/src/bin/scufris-surface-gateway.rs:479` - `unwrap_or_default()`
  is unreachable (`split(';').next()` always returns `Some`), and a header of
  `"; charset=utf-8"` forwards an empty media type upstream instead of being
  refused locally.
- `host/service/src/service.rs:443` - `surface_abort` clears the association
  without checking that the aborting surface owns the open turn.
- `docs/src/dev/briefings.md:407` - "That session-start read is one file read
  and not a timer" no longer matches `readWhatIsWaiting`, which now reads
  yesterday and today. A run left pending across two whole days falls outside
  the window and is never read again; that edge is written down nowhere.
- `agent/extensions/scufris/briefing/briefing.ts:285-291` - a second copy of
  the failure prose in the extension, disagreeing with the helper's. `wake()`'s
  own docstring at `:165-170` is the rule this breaks.
- `tools/jobs/scufris-jobs:3652-3659` - `decide()` is a closure with two
  `nonlocal` bindings and one call site; nothing reads `undecided` after it.
- `agent/extensions/scufris/service/protocol.ts:11` - `MAX_DETAIL_BYTES` is
  exported and read by nothing since `31a877c` removed `agent.state`.
- `agent/extensions/scufris/shared/acknowledgment.ts` -
  `ACKNOWLEDGMENT_STATE_EVENT` is emitted on every gate change and nothing
  listens; `docs/src/dev/messaging.md:97-98` now says so in as many words.

**Not carried:** `docs/src/dev/messaging.md:54-67` describing the deleted
attention-notice mechanism. It is the same finding as G5-12 and is recorded
there; it was correct when this range wrote it and `31a877c` is what made it
stale.

**G1-8 MAJOR - `details` is held to the host's rules but never cleaned, so one
carriage return costs the whole answer, silently.**
`agent/extensions/scufris/service/protocol.ts:344-345`.
Raised by three of the five lanes. `bounded()` now rejects `\0` and `\r`
(`:183-189`) and is applied to `details`. `details` reaches the wire straight
from the model (`response.ts:249-250`, `:347`) with no cleaning, and a worker's
`report.md` may contain `\r` - `write_report` bounds only the length of
`detail` and applies the control-character check to the summary alone. So:
`scufris_job_inspect` with `include_report` hands that text to the model, the
model quotes it into `scufris_final_response(details: ...)`,
`encodeAgentRequest` throws before the line is built, `tell()` catches it into
one `console.error`, the turn has ended, nothing retries, and no screen says
anything. Alex is left looking at his own question with nothing under it -
which is the outcome the service-side half of this same commit
(`service.rs:533-536`) was written to prevent.

The same commit clamped the one field it named (`stateDetail`, now
`jobSummary`) for exactly this reason and left `text` and `details` to throw.
`text` survives because `plainProse` (`response.ts:73-83`) collapses whitespace
and rejects control characters first; `details` has no equivalent. The schema
also bounds `details` in characters while `bounded` bounds bytes, so a
multi-byte `details` under the schema limit is refused on the wire - the same
shape as G5-6.

### G4 `474a7fa..353198d` - briefing pipeline, widget hold, nightly source

Dispatched 5 lanes. Desktop lane in first; findings verified here.

**G4-1 MAJOR - a backend that dies keeps its panel for the full four-hour
ceiling, and two comments and the doc all say it does not.**
`surfaces/desktop/src/widgets/runtime.rs:767-776`.
Verified in this session. `health()` records `open.health` and returns; it
never touches `held` or `holding`. `feed()` is the only writer of `held`, so a
backend that stops writing can never lower it. A timer exhibit dims with its
last reading carrying `_hold: true`, the Python process is killed, and
`Backends::drain` emits `News::Health { Dead }` - which the runtime has in hand
and ignores. The panel holds a shelf slot for four hours of clock-ran time
showing a count frozen at the crash, instead of retiring after the sixty-second
grace.

Three places state the opposite, and they are the ones an author would rely on:
`runtime.rs:716-718` ("a hold survive nothing - not a crash, not a restart, not
a paused timer"), `docs/src/dev/widgets.md:123-124` ("a backend that dies,
restarts, or pauses stops holding by saying nothing"). Only
`backends/timer/backend.py:24` is accurate, because it claims pause and zero
only. Clearing `held` in `health()` on `Health::Dead` is the whole fix.

**G4-2 MAJOR - the grace does not restart when a hold ends, so a finished timer
can get five seconds of notice instead of a minute.**
`surfaces/desktop/src/widgets/runtime.rs:723-725`, `:878-886`.
Verified in this session: the six sites that zero `aging` are create (`:623`),
citation (`:694`), dim (`:829`), hover (`:917`), pin (`:1003`) and release
(`:1036`). None is where a hold begins or ends, and the hold branch `continue`s
past `surface.aging` without touching it. So a panel that accumulated 55s of
grace before a hold began retires five seconds after the hold ends. For the
timer that is the entire notice, against the "about a minute" both
`widget.toml:3` and `docs/src/dev/widgets.md:121` promise, and against
`runtime.rs:88` ("the moment the hold ends, the ordinary minute begins").
The new test cannot catch it: at `:1693` the first hold is fed with `aging`
still zero, so both semantics pass, while its own assertion message at `:1736`
reads "the grace did not start over when the hold ended".

**G4-3 MAJOR - the hold makes a running timer the guaranteed victim of the next
fourth exhibit.**
`surfaces/desktop/src/widgets/runtime.rs:1058-1061`,
`surfaces/desktop/widgets/timer/widget.toml:3`.
`crowd_out` pops `self.shelf.pop_back()` strictly by open order with no regard
for `held`. The hold is what keeps a counting timer alive past its grace, which
makes it the longest-standing exhibit and therefore exactly what `pop_back`
selects. It is `Unsubscribe`d, killing the counting process, and retired.
`WidgetReport::Closed` tells Scufris; nothing tells the person. This is new
pressure: before the hold, the timer retired at sixty seconds and was rarely on
the shelf to be crowded out. The new model-facing description says "It stays on
screen while it is counting, however far the conversation moves on", with no
qualification.

**MINOR, from the desktop lane**

- `backends/timer/backend.py:39` - `CEILING = 86400.0` against
  `HOLD_CEILING = 4h`. "Set a timer for six hours" is accepted and the panel is
  killed two hours short, silently. The old description warned "not for a
  reminder an hour from now"; the replacement removed the warning without
  naming the real bound.
- `runtime.rs:917`, `:1003` - `hover()` and `update()` zero `aging` and leave
  `holding`, so a panel somebody just hovered and Scufris just cited still hits
  the ceiling. The ceiling exists for a backend that says "not yet" and dies; a
  hovered panel is the opposite case.
- `runtime.rs:719` - `.and_then(Value::as_bool).unwrap_or_default()`, so
  `"_hold": 1` or `"_hold": "true"` gets no hold and no log line, in a crate
  that warns on every other widget-author mistake.
- `runtime.rs:726` - `_hold` is forwarded verbatim in the `Act::Update`
  payload, while `backend.py:22` says "`_hold` is the companion's, not the
  widget's" and `docs/src/dev/widgets.md:120` says a reading is the widget's own
  except for it. The reservation is convention only.

## Recovery and disposition - 2026-09-10

This section recovers the interrupted source review. It supersedes severity and
recommendation labels above where they differ. It does not erase the original
review record.

### Authority and tree state

- Exact reviewed revision: `bbaabff615a02cf7a38e14d62fc5d53909775e74`.
- Current `master`, `origin/master`, and tag `v2.4.1` all name that same
  revision. There are no post-tag commits, tracked edits, or staged edits.
- The only dirt in the main checkout is this untracked surviving task
  directory. Recovery copied it into the `recover-nightly-review` Sprout; it
  did not edit the main checkout.
- Therefore the reviewed implementation tree and current implementation tree
  are identical. Every accepted finding below was checked in the current tree;
  that also verifies it against the exact reviewed tree.
- Severity is present user harm. Confidence is confidence in the described
  behavior. Regression risk below means the risk of changing the behavior,
  not the severity of leaving it. Effort is S, M, or L.

### Interruption and transcript provenance

The diagnosis is in Scufris job `582a68032ce3`, chiefly
`~/.local/state/scufris/jobs/582a68032ce3/report.md`. The actual outer Claude
session is
`~/.claude/projects/-home-alex-personal-scufris2/48a3b8a8-97ac-4019-b8d9-dcdf001385cf.jsonl`.
Its children are under the adjacent
`48a3b8a8-97ac-4019-b8d9-dcdf001385cf/subagents/` directory.

| Group | Lane        | Child transcript    | Outcome, UTC                              |
| ----- | ----------- | ------------------- | ----------------------------------------- |
| G5    | craft       | `a26ba3484a019de63` | complete, 20:15:43                        |
| G5    | correctness | `a3bbe9ad8e2591345` | complete, 20:18:27                        |
| G5    | desktop     | `aea5a76407cbcb690` | complete, 20:14:24                        |
| G5    | contracts   | `a36e415b431fd6b6b` | complete, 20:15:59                        |
| G5    | red team    | `aef8dd43e420b60a4` | complete, 20:18:07                        |
| G1    | craft       | `adf9978462e4743f0` | complete, 20:34:50                        |
| G1    | correctness | `a46df4c543785f975` | complete, 20:34:23                        |
| G1    | desktop     | `a1a8686918ce8b0e0` | complete, 20:30:42                        |
| G1    | contracts   | `af55e7a4d2d602845` | complete, 20:35:38                        |
| G1    | red team    | `a775dfac3667e4033` | complete, 20:36:02                        |
| G4    | craft       | `ac08ee56aab971271` | complete, 20:48:31; recovered here        |
| G4    | correctness | `a49b83931b7b0231a` | complete, 20:50:30; recovered here        |
| G4    | desktop     | `aaeaf621fdcd19ba1` | complete, 20:43:51; already written above |
| G4    | contracts   | `a2c6c27fd8d323b7b` | complete, 20:48:19; recovered here        |
| G4    | red team    | `a1d5c76f009952b9e` | partial; exit 137 at 20:51:03             |

This is 14 completed lanes out of 15 dispatched. G2 and G3 were never
dispatched. The task stopped changing at 23:45:40 EEST, after only the G4
desktop report had been written. The outer source said, at 20:50:52 UTC,
"Four G4 lanes in ... while the red-team lane finishes." Its final source-read
command at 20:51:02 has no result. Neither the red-team child nor the outer
source has an `end_turn`, and the outer source wrote no final briefing envelope.

The G4 red-team child made a temporary profile file a symlink to `/dev/zero`
and called `apply_profile_bounds`. `tools/briefing/briefing.py` used unbounded
`Path.read_bytes()`. The Python process grew to about 29.2 GB RSS and 28.3 GiB
of swap before the kernel killed it. The nightly unit has `OOMPolicy=stop` and
`KillMode=control-group`, so systemd then stopped the collector and both source
process trees. The configured eight-hour deadlines had more than seven hours
left. This was an unsafe probe and cgroup-wide OOM stop, not a deadline. It must
not be repeated.

### Exact coverage status

`COMPLETE` means the intended five unattended lanes were adjudicated, or that
the small packaging group was reviewed directly as documented. `RECOVERED`
means a completed report absent from the surviving task was found, deduplicated,
and checked here. `PARTIAL` means at least one required lane did not complete.
`NOT RUN` means no lane was dispatched.

| Group | Status             | Exact coverage                                                                                             |
| ----- | ------------------ | ---------------------------------------------------------------------------------------------------------- |
| G1    | COMPLETE           | 5/5 lanes completed, written before interruption, rechecked here.                                          |
| G2    | NOT RUN            | 0/5 lanes dispatched.                                                                                      |
| G3    | NOT RUN            | 0/5 lanes dispatched.                                                                                      |
| G4    | PARTIAL, RECOVERED | 4/5 completed; desktop was written; craft, correctness, and contracts were recovered; red team is partial. |
| G5    | COMPLETE           | 5/5 lanes completed, adjudicated, written, and rechecked here.                                             |
| G6    | COMPLETE           | Direct review was written and rechecked here.                                                              |

No whole group is `INVALID` or `SUPERSEDED`. Those labels apply to individual
claims below. The overall review remains OPEN because G2 and G3 were not run
and G4 is partial.

### Recovered G4 findings

The three complete reports were not accepted verbatim. Their load-bearing
claims were read against `bbaabff` and the identical current tree. The safe,
useful part of the partial red-team transcript was also checked. Duplicates are
folded into existing IDs rather than counted again.

#### G4-R1 BLOCKER: profile bounds can make the host read an infinite special file

`tools/briefing/briefing.py:1195` calls `Path.read_bytes()` before any type or
size check. There is no no-follow open, `fstat` regular-file check, or byte
ceiling. A replaced generated file can therefore be a device, FIFO, symlink,
or oversized regular file. This is the path that read `/dev/zero` until the
nightly cgroup was OOM-stopped. The current tree still has the same call.

Recommendation: FIX NOW. Open without following symlinks, require a regular
file, and read no more than a small explicit maximum before JSON decoding. User
impact is loss of the nightly plus unrelated siblings in its cgroup and severe
host memory pressure. Confidence HIGH. Regression risk M; effort S-M. Add only
safe metadata and bounded regular-file tests. Do not execute a device or FIFO
probe. This comes before any other briefing reliability work.

#### G4-R2 MAJOR: timeout recovery accepts the prompt's example as a real answer

`tools/briefing/briefing.py:580-606` chooses the last decodable fenced object.
The prompt contains a syntactically valid example with `status: ok`.
`attempt()` at `:767-783` now parses partial stdout after a timeout. A source
that echoes the required template before doing work and is then cut off is
recorded as an OK source with placeholder title, headline, facts, body, and
offer. The focused safe probe reproduced this from the real prompt and parser.

Recommendation: FIX NOW. Make the example impossible to accept as an answer,
or bind a final envelope to a per-invocation sentinel. User impact is a
fabricated green briefing. Confidence HIGH. Parser changes have M regression
risk and M effort because corrections and fenced Markdown must still work.
This is independent of G4-R1 but belongs in the same parser-boundary tests.

#### G4-R3 MAJOR: two collectors for one date and profile overwrite one run

`collect()` at `tools/briefing/briefing.py:955-1095` uses one date/profile
directory with `mkdir(..., exist_ok=True)` and writes a new manifest without an
exclusive owner, generation, or lock. A timer, shell, or tool can start the
same key concurrently. The archived red-team regular-file repro had the faster
run report its own result and the slower run then replace the manifest and all
same-slug contribution files. The current code still has no owner fence.

Recommendation: FIX NOW. Give a run generation and exclusive owner, or refuse
a second live collector for the same key. User impact is silent loss and a run
whose caller observed a result that no longer exists. Confidence HIGH.
Regression risk H; effort L because stale-owner recovery and atomic publication
must agree. Do this before allowing concurrent in-process profiles (G4-R4) and
before re-collection fixes (G1-6).

#### G4-R4 MAJOR: one hand-run nightly blocks every briefing tool run for eight hours

`agent/extensions/scufris/briefing/briefing.ts:148,263-298` has one `running`
boolean. A hand-run nightly takes the profile's eight-hour timeout. Until its
`finally`, a morning or other profile returns only `a run is already going`.
There is no run identity or cancel operation in that response.

Recommendation: SCHEDULE LATER, after G4-R3. User impact is loss of all
hand-requested briefings for the long run. Confidence HIGH. A better refusal is
S; safe per-profile concurrency or cancellation is M-L with H regression risk.
Do not key the boolean by profile until same-key and stale-owner behavior is
safe.

#### G4-R5 MAJOR: a live second orchestrator is called abandoned and the user is told to kill it

`tools/jobs/scufris-jobs:3930-3956` calls every live pane with another
`owner_session` an orphan. `orchestration.ts:1660-1684` says it belongs to a
previous session and tells the user to run `tmux kill-session`. It does not say
that another live service or development session can still own and steer it.

Recommendation: FIX NOW. First change the message to say `another session` and
include owner/state evidence. Add owner-liveness classification before making a
kill recommendation. User impact is destruction of valid running work by
following a false verdict. Confidence HIGH. Wording is S/low risk; liveness is
M/medium risk. Never automate the kill.

#### G4-R6 MAJOR: an answer noted during collection has no durable body until every source ends

`note()` at `tools/briefing/briefing.py:1039-1057` writes only
`index_entry(contribution)` into the live manifest. Full contribution files are
not written until `finish()` at `:1098-1120`, after the pool returns. If a later
source or the collector dies, `read_run()` at `:1306-1321` synthesizes an empty
body for the source whose manifest entry says it answered.

Recommendation: FIX NOW as part of G4-R3's generation design. Atomically write
the contribution before publishing its completed index entry. User impact is a
headline that claims recovery while the evidence body is gone. Confidence
HIGH. Regression risk M; effort M. A generation fence must prevent a late
writer from entering a replacement run.

#### G4-R7 MINOR: the briefing document contradicts itself about manual bounds

`docs/src/dev/briefings.md:249` says every entry point reads generated profile
bounds. Lines 278-285 then say manual tool and shell runs get code defaults.
The first statement matches the current code.

Recommendation: SCHEDULE LATER with the bounds hardening. User impact is wrong
operator diagnosis and avoidable workarounds. Confidence HIGH. Regression risk
low; effort S. Update docs after G4-R1 and G4-R11 define the final contract.

#### G4-R8 MINOR: the TypeScript profile-bounds reader has no contract test

`agent/extensions/scufris/briefing/briefing.ts:49-82` independently spells the
file path, `deadline` key, and fallback. Python and Nix have tests; no TypeScript
test mentions `briefing-profiles` or `collectTimeout`. Drift restores the old
30-minute extension cutoff while the other checks remain green.

Recommendation: FIX NOW with G4-R1. User impact is a silently shortened manual
night after future drift. Confidence HIGH. Regression risk low; effort S. Test
the generated-file value and environment precedence without special files.

#### G4-R9 MINOR: the file called the whole widget contract omits `_hold`

`surfaces/desktop/widgets/widget.d.ts` claims to be the whole shared contract
but does not name the runtime-reserved `_hold` field documented in
`docs/src/dev/widgets.md:118-126` and read in `runtime.rs:715-726`.

Recommendation: SCHEDULE LATER with G4-1 through G4-3. User impact is an author
accidentally taking control of panel lifetime. Confidence HIGH. Regression risk
low; effort S. Decide first whether the runtime will strip the field before the
page sees it (G4-M4).

#### G4-R10 MINOR: the hold-grace test does not create a spent grace

The assertion near `surfaces/desktop/src/widgets/runtime.rs:1736` says the
grace restarted, but its surface had zero `aging` when hold began. Both current
and intended behavior pass. This confirms, rather than replaces, G4-2.

Recommendation: FIX NOW with G4-2. User impact is future false confidence in a
broken timer notice. Confidence HIGH. Regression risk low; effort S. Spend part
of the grace before setting `_hold: true`.

#### G4-R11 MINOR: a profile deadline above the Node timer range expires immediately

`collectTimeout()` returns unbounded seconds times 1000, and `runHelper()`
passes it to Node `setTimeout`. The partial red-team lane safely observed Node
clamp `2147483648` ms to about 1 ms. Home Manager requires positive integers
but gives no upper bound.

Recommendation: SCHEDULE LATER with profile validation. User impact is an
immediate timeout for an extreme but accepted configuration. Confidence HIGH.
Regression risk low; effort S. Bound or chunk the timer; do not merely increase
it.

### Existing finding dispositions

The anonymous bullets above get stable IDs here in their listed order. These
tables are the decision record for every existing product finding.

#### Baseline and G6

| ID                                 | Current severity / confidence | Recommendation and shape                                                                                                                                 |
| ---------------------------------- | ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F0                                 | MINOR / HIGH                  | FIX NOW. Ambient variables create false local failures but do not affect sandboxed CI or product runtime. Low risk, S, independent test-fixture cleanup. |
| F1                                 | MINOR / HIGH                  | FIX NOW. The next Python dependency can make the helper gate test a different interpreter; CI fails loudly. Low risk, S, import `nix/python.nix`.        |
| Original F2 disabled-button theory | NONE / HIGH                   | CLOSE INVALID. The host broadcasts `OfferTaken` before the agent lookup, so the button is spent and re-enabled. The corrected two-bound defect is G5-1.  |

#### G5 adjudicated findings

| ID    | Current severity / confidence | Recommendation and shape                                                                                                                                                                              |
| ----- | ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G5-1  | MAJOR / HIGH                  | FIX NOW. A press can be durably accepted while no turn starts. M risk/M effort. Design with G5-2 and G5-4; align retained offers or add agent rejection and rollback.                                 |
| G5-2  | MAJOR / HIGH                  | FIX NOW. Job and offer refusals are invisible on desktop. M/M. Preserve operation and route non-submission refusals before changing offer transactions.                                               |
| G5-3  | MAJOR / MEDIUM                | FIX NOW after one focused WebKitGTK keyboard repro. The wrong Enter can submit draft prose. H/M because focus fixes regress easily; preserve or restore the row control during `tail()` and `list()`. |
| G5-4  | MAJOR / HIGH                  | FIX NOW. Agent downtime spends an offer permanently. M/M. Implement after G5-2 so any failed relay is visible; then settle only after relay or support rollback.                                      |
| G5-5  | MAJOR / HIGH                  | FIX NOW. One long project path freezes every job row and the tray. Low/S. Clamp the display field before protocol validation; independent of row-cap policy.                                          |
| G5-6  | MAJOR / HIGH                  | FIX NOW. A valid schema-level Unicode label can discard the whole answer. M/S-M. Use byte-safe normalization with G1-8 and G1-M4 before encoding.                                                     |
| G5-7  | MAJOR / HIGH                  | FIX NOW. A review row exposes a stop control guaranteed to fail. Low/S. Send `job.root_job` and test descendant-row control.                                                                          |
| G5-8  | MAJOR / HIGH                  | FIX NOW. The same drain failure after a second session start has no row or wake. Low/S. Reset all drain state together; verify with G1-3's poison case.                                               |
| G5-9  | MAJOR / HIGH                  | SCHEDULE LATER, after G5-2 and offer transaction semantics. A normal refusal paints iPhone failed while hiding its detail. M/M iOS state and notice change.                                           |
| G5-10 | MINOR / HIGH                  | SCHEDULE LATER. A row age freezes and can mislead the morning reader. Low/S, add one display timer without republishing job state.                                                                    |
| G5-11 | MAJOR / HIGH                  | FIX NOW. A ninth live job is invisible and cannot turn the tray red. M/M. Decide overflow presentation with G5-M2 before changing the documented cap.                                                 |
| G5-12 | MINOR / HIGH                  | SCHEDULE LATER after offer and drain mechanics settle. Stale ingress and notice docs mislead maintainers. Low/S doc sweep.                                                                            |

The eight G5 minor bullets above are G5-M1 through G5-M8.

| ID and summary                                   | Current severity / confidence | Recommendation and shape                                                                                                                      |
| ------------------------------------------------ | ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| G5-M1, republish silently disarms stop           | MINOR / HIGH                  | SCHEDULE LATER with G5-3. Fails safe but wastes a press. M/S due focus/arming interaction.                                                    |
| G5-M2, drain row is culled at the cap            | MAJOR / HIGH                  | FIX NOW with G5-11. The final drain warning can disappear after wakes stop. M/M overflow policy.                                              |
| G5-M3, citations after the fourth are cleared    | MINOR / HIGH                  | SCHEDULE LATER. Measured badges are lost, but job completion still arrives. Low/S-M; retain overflow for the next answer.                     |
| G5-M4, protocol v6 check comment                 | MINOR / HIGH                  | SCHEDULE LATER in the docs/check sweep. No runtime impact. Low/S.                                                                             |
| G5-M5, unused `JobRow` re-export                 | NONE / HIGH                   | ACCEPT RISK. It has no user effect and removal can wait for nearby cleanup. Low/S.                                                            |
| G5-M6, old DOM offer is outside desktop's ring   | MINOR / MEDIUM                | SCHEDULE LATER after G5-2. The host usually drops the same old offer, but the stale DOM button then needs an honest refusal or trimming. M/M. |
| G5-M7, jobs-only HUD anchors at top              | MINOR / HIGH                  | ACCEPT RISK. Cosmetic empty-conversation layout only. Low/S.                                                                                  |
| G5-M8, widget page can invoke unarmed job cancel | MAJOR / MEDIUM                | SCHEDULE LATER as capability hardening. Packaged widgets are trusted today, but the capability defeats the stated two-press boundary. M/M-H.  |

#### G1 adjudicated findings

| ID   | Current severity / confidence | Recommendation and shape                                                                                                                                                                                                                    |
| ---- | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G1-1 | MAJOR / HIGH                  | FIX NOW with the offer/job transaction work. Follow-up answers are unowned, unspoken, and stripped of widgets. M/M. Open association from the pressing surface only after the press is accepted.                                            |
| G1-2 | MAJOR / HIGH                  | FIX NOW with G1-1. Code, test, and docs disagree about whether a refused answer closes the turn; a correction becomes an unprompted duplicate. M/M. Choose one transaction rule, then make the cumulative-channel test drain each response. |
| G1-3 | BLOCKER / HIGH                | FIX NOW. One admitted non-printable summary permanently wedges every job event. Low/S-M. Use one predicate at report, parse, and record doors, including `scufris-report`; then test the seven demonstrated code points.                    |
| G1-4 | MAJOR / HIGH                  | SCHEDULE LATER after response transaction rules. One bad optional attachment or widget drops every good peer. M/M-L. Resolve and validate per item while preserving prose and an honest rejection.                                          |
| G1-5 | MAJOR / HIGH                  | SCHEDULE LATER. The executable-word blacklist cannot enforce no foreground waiting. H/M-L policy change. Replace blacklist reliance with a bounded command capability or remove the claim that it is a complete guard.                      |
| G1-6 | MAJOR / HIGH                  | SCHEDULE LATER after G4-R3. Re-collecting a delivered key yields no wake and renders new cards over old prose. H/M. Define replace/refuse/new-generation semantics first.                                                                   |
| G1-7 | MINOR / HIGH                  | SCHEDULE LATER. Report trimming, summary bounds, and cleanup order docs are stale. Low/S after behavior choices are stable.                                                                                                                 |
| G1-8 | MAJOR / HIGH                  | FIX NOW with G5-6. A carriage return or byte-long `details` discards the whole answer. M/S-M. Normalize or omit only the invalid optional field before encoding.                                                                            |

The thirteen G1 minor bullets above are G1-M1 through G1-M13.

| ID and summary                                      | Current severity / confidence | Recommendation and shape                                                                                                               |
| --------------------------------------------------- | ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| G1-M1, Markdown headings split report entries       | MINOR / HIGH                  | SCHEDULE LATER. Trimming can detach evidence from its event header. Low/S; split only on status-entry headings.                        |
| G1-M2, missing attachment object maps to 500        | MINOR / HIGH                  | SCHEDULE LATER. Clients may retry a permanent miss. Low/S; map missing to not-found and pin the error variant.                         |
| G1-M3, quota-declined records stay unindexed        | MINOR / HIGH                  | SCHEDULE LATER. Disk and quota accounting disagree until restart. M/M; track or deterministically evict unindexed records.             |
| G1-M4, NEL whitespace differs across JS and Rust    | MINOR / HIGH                  | FIX NOW with G1-8. An edge string closes the agent connection and loses an answer. Low/S once one shared normalization rule is chosen. |
| G1-M5, OpenAPI omits two incomplete-upload 400s     | MINOR / HIGH                  | SCHEDULE LATER with G1-M6. Generated clients lack cases for real responses. Low/S.                                                     |
| G1-M6, upload refusal literals bypass lockstep list | MINOR / HIGH                  | SCHEDULE LATER with G1-M5. Drift can evade the cross-language test. Low/S.                                                             |
| G1-M7, empty media type reaches upstream            | MINOR / HIGH                  | ACCEPT RISK. Upstream still refuses it; only refusal locality and diagnosis degrade. Low/S.                                            |
| G1-M8, any surface abort clears the owner           | MAJOR / HIGH                  | FIX NOW with G1-1. An unrelated surface can make the real answer unspoken. M/S-M; require owner identity.                              |
| G1-M9, startup reads only today and yesterday       | MINOR / HIGH                  | SCHEDULE LATER. A briefing pending over two dates is never delivered. Low/S-M; use bounded pending discovery rather than a timer.      |
| G1-M10, duplicate briefing failure prose            | MINOR / HIGH                  | SCHEDULE LATER with G1-6. Drift can give tool and helper different verdicts. Low/S.                                                    |
| G1-M11, one-use `decide` closure                    | NONE / HIGH                   | ACCEPT RISK. Maintainability only; no wrong behavior. Low/S.                                                                           |
| G1-M12, unused `MAX_DETAIL_BYTES`                   | NONE / HIGH                   | ACCEPT RISK. Dead export only. Low/S.                                                                                                  |
| G1-M13, acknowledgment event has no listener        | NONE / HIGH                   | ACCEPT RISK. It is documented and harmless until an integration needs it. Low/S.                                                       |

#### G4 findings already written

The four anonymous G4 desktop bullets above are G4-M1 through G4-M4.

| ID and summary                                   | Current severity / confidence | Recommendation and shape                                                                                                                              |
| ------------------------------------------------ | ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| G4-1, dead backend retains hold                  | MAJOR / HIGH                  | FIX NOW. A frozen panel owns a scarce slot for four hours. Low/S; clear hold on `Health::Dead` and test it.                                           |
| G4-2, grace does not restart after hold          | MAJOR / HIGH                  | FIX NOW. A finished timer can vanish before it is noticed. Low/S with G4-R10. Reset grace at the hold transition.                                     |
| G4-3, crowd-out always selects oldest held timer | MAJOR / HIGH                  | FIX NOW. Opening a fourth exhibit can silently kill an active timer. M/M; prefer unheld exhibits and define all-held behavior.                        |
| G4-M1, timer accepts longer than hold ceiling    | MINOR / HIGH                  | FIX NOW with G4-3. A long timer is killed early. Low/S after the four-hour product limit is chosen; align model text and backend.                     |
| G4-M2, hover/citation does not renew hold budget | MINOR / HIGH                  | SCHEDULE LATER with hold policy. An actively reread timer can still hit the ceiling. M/S.                                                             |
| G4-M3, malformed `_hold` is silent false         | MINOR / HIGH                  | ACCEPT RISK. This is a packaged backend author error, bounded by normal grace. Low/S logging improvement.                                             |
| G4-M4, `_hold` is forwarded to widget data       | MINOR / HIGH                  | SCHEDULE LATER with G4-R9. Contract privacy is false though the shipped widget ignores it. Low/S-M; strip before update if that remains the contract. |

### Recovered candidates closed, accepted, or deduplicated

| Candidate                                        | Status        | Reason                                                                                                                   |
| ------------------------------------------------ | ------------- | ------------------------------------------------------------------------------------------------------------------------ |
| G4 contracts/correctness backend-death claim     | SUPERSEDED    | Duplicate of G4-1; stronger desktop evidence is already written.                                                         |
| G4 craft hold-grace claim                        | SUPERSEDED    | Duplicate of G4-2; the weak-test part is retained as G4-R10.                                                             |
| G4 correctness long-timer claim                  | SUPERSEDED    | Duplicate of G4-M1.                                                                                                      |
| G4 interpreter copies                            | SUPERSEDED    | `6c76c61` unified the runtime paths before the reviewed head; the one residual gate mismatch is F1.                      |
| G4 red-team re-collection stale prose            | SUPERSEDED    | Same root defect as G1-6; the archived repro strengthens that finding.                                                   |
| Nightly `model = "Opus"` casing                  | INVALID       | Claude accepted it and this exact run used it; lowercase consistency has no demonstrated product failure. CLOSE INVALID. |
| `orphans` returns unused `job_ids` beside `jobs` | ACCEPTED RISK | Redundant output has no user effect. Do not schedule it without an API cleanup.                                          |
| Unused Python `STATUSES` constant                | ACCEPTED RISK | Dead internal name, not an authority used by a product path.                                                             |
| Original G5 in-memory `archived` claim           | SUPERSEDED    | Fixed by `ae316e8` before the reviewed head.                                                                             |
| Original G5 unclipped project grid claim         | SUPERSEDED    | Fixed by `6c76c61` before the reviewed head.                                                                             |
| G5 pending badges on a later answer              | INVALID       | The code documents this as deliberate and labels badges with job IDs.                                                    |
| G1 duplicate stale attention-notice claim        | SUPERSEDED    | Deduplicated into G5-12.                                                                                                 |

All `INVALID` and independently `SUPERSEDED` candidates are NONE / HIGH as
separate findings. The two accepted-risk maintenance candidates are also NONE /
HIGH: both still exist, but neither has demonstrated product impact.

### Prioritized fix slate

1. Make profile-bound reads regular-file-only and byte-bounded, and add safe
   Python and TypeScript contract tests: G4-R1, G4-R8, and G4-R11's bound.
2. Make the job event doors use one text predicate and reset all drain state as
   one unit: G1-3 and G5-8.
3. Normalize every model- or helper-origin display field before wire encoding:
   G5-6, G1-8, G1-M4, and G5-5.
4. Repair the press transaction in dependency order: show typed job/offer
   refusals (G5-2), establish surface ownership (G1-1, G1-2, G1-M8), then make
   offer settlement atomic and recoverable (G5-1, G5-4), and pass the root for
   review-row stop (G5-7).
5. Add exclusive briefing run identity first, then persist each source answer
   before its index entry: G4-R3 then G4-R6. Only after that address re-collect
   and per-profile cancellation: G1-6 then G4-R4.
6. Correct widget hold lifecycle in one tested change: G4-1, G4-2, G4-R10,
   G4-3, and G4-M1. Decide the contract before G4-R9 and G4-M4.
7. Preserve job visibility under pressure: G5-11 and G5-M2. Separately
   reproduce G5-3 on WebKitGTK before changing focus-preserving HUD updates.

### Smallest remaining review coverage

The documented scope is not complete. The smallest coverage that completes it
without repeating accepted work is:

- G2 `997e057..19ce35a`: one unattended five-lane panel: craft, correctness,
  desktop, contracts, and red team.
- G3 `19ce35a..474a7fa`: one unattended five-lane panel with the same lanes.
- G4 `474a7fa..353198d`: one replacement red-team lane only, followed by outer
  adjudication. It should use static reasoning and bounded regular-file
  fixtures. It must not open or read `/dev/zero`, a device, FIFO, socket, or
  symlink target, and must not create an unbounded stream. The four completed
  G4 lanes must not be repeated.

No Feel lane is needed because this was an unattended review without `--live`.
The optional WebKitGTK reproduction for G5-3 increases confidence before its
fix; it is not missing group coverage.

### Landing readiness

`sprout land recover-nightly-review --dry-run` passed every Sprout guard and
left the main checkout unchanged. It does not probe untracked-file collisions.
The main checkout has the surviving untracked `TASK.md` at the same path as the
new tracked file, and the two files differ because this branch contains the
recovery. A real squash is therefore expected to refuse rather than overwrite
the untracked file.

The evidence commit is complete and the branch is clean, but it is not ready
for immediate landing. Before an explicit landing, the owner must preserve and
then remove or replace the main checkout's untracked copy. This recovery did not
do that because the request required inspecting the dirty main checkout without
changing it. Run the dry-run again after resolving that collision.

### Verification and skips

Recovery ran only checks needed to validate review claims:

- `git` proved current implementation `HEAD` is the exact reviewed revision and
  has no implementation diff from it.
- A structural parse found 15 child transcripts, 14 child `end_turn` records,
  and exactly one incomplete child, G4 red team.
- A safe parser probe against the real prompt and module confirmed that partial
  timeout output containing the prompt example is accepted as `status: ok`.
- A safe helper-module probe confirmed all seven recorded non-printable
  characters pass `parse_event` and fail `valid_record_text`.
- Static inspection confirmed the profile reader still has unbounded
  `read_bytes()` and no no-follow, file-type, or size guard. No special file was
  opened.
- Static inspection plus the retained regular-file transcript repro confirmed
  same-key collection has no exclusive owner or generation.
- The source pass rechecked every accepted path named in the disposition tables.
- `git diff --check` and the focused Prettier check for this task passed after
  normalizing its Markdown layout; no finding text was removed.

The earlier exact-revision evidence remains: 121 Node tests and typecheck
passed; 376 Python tests passed with ambient briefing variables removed; 414
Rust workspace tests passed. Those suites were not repeated because the
implementation tree has not changed. `nix flake check`, Ruff, an X display, and
G2/G3 review were not run. A skip is not a pass.

No implementation file, live briefing state, service, process, notification
path, release, remote, or tag was changed. This task stays OPEN.

## OOM diagnosis - 2026-09-10

The owner asked what the `/dev/zero` incident actually was, and whether it was a
defect in this project or a faulty command chosen by the agent. It was both,
plus a missing guardrail. Three things had to line up; removing any one of them
would have kept the night alive.

### The exact command

The G4 red-team child (`agent-a1d5c76f009952b9e.jsonl`, tool call 175) ran a
single Bash block that ended with this Python, executed in-process. It is
quoted exactly as the lane ran it, so the fence is `text`: a `python` fence
would let `ruff format` rewrite the evidence.

```text
sys.path.insert(0, '/home/alex/personal/scufris2/tools/briefing')
import briefing
...
f = p / "briefing-profiles.json"
...
# symlink to /dev/zero
f.unlink(); os.symlink("/dev/zero", f)
try:
    briefing.apply_profile_bounds("nightly"); print("devzero symlink -> ok (no hang)")
except Exception as e: print("devzero symlink -> ", type(e).__name__, e)
```

The preceding eleven cases in the same block are ordinary hostile-input
fixtures: bad JSON, wrong types, negative and huge numbers, a `__proto__` key,
a NUL in a key. All of those are safe and all of them returned. The twelfth
case is the one that killed the machine.

Note what the lane was actually testing: the comment it wrote for itself was
`devzero symlink -> ok (no hang)`. It expected the call to return a refusal.
The probe was written on the assumption that the product code was safe.

### Contributing factor 1: a real defect in this project

`tools/briefing/briefing.py:1195`:

```python
raw = (config_home() / PROFILE_BOUNDS_FILE).read_bytes()
```

`apply_profile_bounds` (`:1182`) reads the generated profile-bounds file with
no `O_NOFOLLOW`, no `fstat` regular-file check, and no byte ceiling. The
symlink was followed to `/dev/zero`, and `read_bytes()` on a character device
that never reaches EOF grows the buffer until the allocator gives up. The
Python process reached about 29.2 GB RSS and 28.3 GiB of swap.

This is G4-R1, and it is genuine. The path is reachable without any agent: any
process that can write `$XDG_CONFIG_HOME/scufris/briefing-profiles.json` can
replace it with a symlink or a FIFO and take the host down the next time a
briefing starts. Home Manager generates that file, but nothing in the reader
requires that the file it finds is the file Home Manager wrote.

### Contributing factor 2: an unsafe probe

The lane was entitled to test hostile profile bounds; that is its job, and the
other eleven cases were the right shape. Pointing product code at `/dev/zero`
was not. A device that never returns EOF is not a bounded fixture, and the
brief asks for "an oversized one", not an infinite one.

The safe form of the same probe is metadata plus static reasoning: `os.lstat`
the path, observe that the reader never calls `stat`, and report the missing
guard. That proves the defect without executing it. This is now written into
the lane instructions for the continued review.

The deeper problem is that `.agents/skills/scufris-review/lanes/red-team.md`
told the lane to drive states to their limits and never told it to bound its
own blast radius. The lane ran the probe in the same control group as the live
nightly collector, so the blast radius was the whole run.

### Contributing factor 3: no memory guardrail on the unit

`nix/home-manager.nix:639-652` builds the briefing service with `Type`,
`ExecStart`, `TimeoutStartSec` and `WorkingDirectory`. There is no
`MemoryMax`, no `MemoryHigh`, and no explicit `OOMPolicy`.

The earlier recovery section says the unit has `OOMPolicy=stop` and
`KillMode=control-group`. That is right about the behaviour and wrong about the
provenance: both are systemd's defaults, not settings this project chose. The
correction matters, because it means the containment that did eventually stop
the bleeding was the kernel OOM killer, not a bound this project set.

`TimeoutStartSec = deadline + 300` bounds time, not memory. The run had more
than seven hours left on its deadline when it died. A time bound cannot stop a
runaway allocation, and nothing else was watching.

### Verdict

A defect in this project, exposed by a probe that should not have been run the
way it was, inside a unit with no memory ceiling to contain it.

Fixing G4-R1 alone stops this exact incident. Fixing all three stops the class:
the reader refuses a non-regular file, the lane proves such defects without
executing them, and the unit caps what one bad allocation can take with it.

## Fixes applied - 2026-09-10

Working tree, uncommitted. Every check below was run after the last edit.

### The OOM class, closed at all three points

| What                                                                                                                                                                                                                                        | Where                                                                  | Proof                                                                                                                                                                                                     |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The profile-bounds reader is bounded, no-follow, and regular-file only. New `read_profile_bounds` opens with `O_NOFOLLOW\|O_NONBLOCK`, settles the type from the descriptor with `fstat`, and reads at most `MAX_PROFILE_BOUNDS + 1` bytes. | `tools/briefing/briefing.py`                                           | Two new tests. A symlink, a FIFO and a directory all read as nothing; an oversized regular file reads as nothing; one under the bound still works. The FIFO case is proved not to block with a `SIGALRM`. |
| `O_NONBLOCK` was added after the first attempt: `open()` on a FIFO with `O_RDONLY` blocks before `fstat` can run, so the guard against a FIFO would have been the hang it exists to prevent.                                                | same                                                                   | Alarm-guarded test.                                                                                                                                                                                       |
| The unit has a memory ceiling. `MemoryMax=4G`, `MemoryHigh=3G`.                                                                                                                                                                             | `nix/home-manager.nix:656`                                             | Static. `TimeoutStartSec` bounds time, not memory; the run had seven hours left when it died.                                                                                                             |
| Lanes must bound their own blast radius. A new "Bounded probes" section, written from the incident, plus a pointer at the top of the red-team brief.                                                                                        | `.agents/skills/scufris-review/lanes/reviewer.md`, `lanes/red-team.md` | The eight lanes this round carried the same rule in their prompts. One reached G4-R1 independently and recorded "Determined statically; not executed."                                                    |

### Defects fixed

| ID           | What changed                                                                                                                                                                                                                                                      |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| N1 BLOCKER   | `suggest` guards `lift_history` with `(OSError, ValueError) -> Refused`, matching `foods`. A corrupt day file now refuses on the badge instead of killing the backend on every keystroke. New test; proved to fail without the fix.                               |
| G1-3 BLOCKER | One `displayable` predicate at all four job-event doors: `write_report`, `parse_event`, `valid_record_text`, and `scufris-report`. New test over seven code points that passed `ord < 32` and failed `isprintable`.                                               |
| N4 MAJOR     | `_plain` refuses a Markdown heading, so `set_split`, `normalize_food` and `normalize_lift` can no longer forge a section header and orphan the Workout table. New test.                                                                                           |
| N3 MAJOR     | Ceilings moved off the staleness tolerance: claude and codex 180 -> 150, system 3 -> 2, and the three model-facing descriptions follow. Two new tests bind each ceiling and each description to the widget's own `cadence`; both proved to fail at the old value. |
| N6 MAJOR     | The three gateway refusal literals are now `refusal::` constants.                                                                                                                                                                                                 |
| N13 MAJOR    | The four `protocol.ts` `invalid_widgets` literals are now `REFUSAL.INVALID_WIDGETS`.                                                                                                                                                                              |
| N7 MAJOR     | The `attachment.rs` fixture is JSON again, and its status changed to `BAD_REQUEST` so the arm can only be reached through the code half.                                                                                                                          |
| N8           | The `report_launch_failure` docstring no longer claims to report a capability refusal, and says why the gate stays outside the guarded region.                                                                                                                    |
| N10          | `resolve`'s docstring corrected; a test pins the two-candidate behaviour rather than the old answer.                                                                                                                                                              |
| G4-1 MAJOR   | `health()` clears `held`, `holding` and `aging` on `Health::Dead`. New test.                                                                                                                                                                                      |
| G4-2 MAJOR   | `feed()` resets `aging` at the held-to-unheld transition only.                                                                                                                                                                                                    |
| G4-R10       | The grace test now spends most of the grace before the hold begins, so it can fail. Proved: it fails with the G4-2 fix removed.                                                                                                                                   |
| F0           | `setUpModule` in `tests/test_briefing.py` removes ambient `SCUFRIS_BRIEFING_*` and restores it after. 111 tests pass under the exact environment the timer unit exports, where six failed before.                                                                 |
| F1           | `nix/checks/helpers.nix` imports `../python.nix` instead of building its own interpreter.                                                                                                                                                                         |

### Also

- `.agents/skills/scufris-review/SKILL.md` now dispatches at most two lanes at
  a time. The old text said to send every lane in one message, which contradicts
  the owner's standing two-agent cap.
- A `python` fence around the verbatim OOM transcript in this file was changed
  to `text`. `ruff` 0.16.3 formats Python inside Markdown, and it wanted to
  rewrite `f.unlink(); os.symlink(...)` into two statements - silently
  falsifying evidence. Anything quoted verbatim in a `tasks/` record needs a
  `text` fence.

### Checks

- `cargo test -p scufris-desktop`: 331 passed.
- `python3 -m unittest discover -s tests -p 'test_*.py'`: 384 passed.
- `npm run check`: typecheck, 121 node tests, and `prettier --check` all pass.
- `ruff format --check .`: 252 files formatted. `ruff check .`: passed.
- `nix flake check` was NOT run. It is the slow integration gate and CI owns it.
  The `nix` edits here are `nix/checks/helpers.nix` and two unit properties;
  neither was evaluated. A skip is not a pass.

### Not fixed, and why

- The press and surface-ownership cluster (G5-1, G5-2, G5-4, G1-1, G1-2,
  G1-M8) and N2/N5, which sit in the same code. These need one transaction rule
  and one notice-slot decision written down first. N2 is a BLOCKER and is the
  most user-visible thing still open.
- Run identity and atomic publication (G4-R3, G4-R6), which gate G1-6 and
  G4-R4. H regression risk, and stale-owner recovery has to be designed.
- G4-R2, the fabricated green briefing. The parser change is the riskiest edit
  in the slate and wants its own pass.
- The stuck-pane half of N8: an orchestrator watch for a pane that exited with
  no terminal event.
- G5-3, which needs a WebKitGTK keyboard repro before anything touches focus.
- G5-11 and G5-M2, an overflow presentation decision.
- The remaining MINOR tail, including `identity.rs`'s fixed temporary name
  (3 lanes), `one_line`'s over-refusal of prefixed writers, `_unadorned`'s
  keycap regression, the `service.test.ts` regex that silently skips, and the
  `desktop-ui.test.ts` service-state guard.
