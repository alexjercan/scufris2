# Verification (v1)

What was verified automatically, what was verified by hand in this workspace,
and what still needs a live desktop session.

## Automated

`npm run check` - typecheck, 112 node tests, Prettier: pass.

`cargo test` in `desktop/` - 104 tests: pass. Also run inside the Nix build's
check phase, so packaging cannot ship an untested binary.

`cargo fmt --all --check` and `cargo clippy --all-targets`: clean, no warnings.

`nix flake check`: all checks pass, including the full Tauri build of
`scufris-desktop`.

## Verification items from TASK.md

| Item                                            | Where                                                                                                                                                                                           |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fast-send flow                                  | `state.rs::enter_while_recording_transcribes_and_submits_without_confirmation`                                                                                                                  |
| Transcript-review flow                          | `state.rs::a_second_activation_while_recording_opens_the_editable_review`                                                                                                                       |
| Cancellation and focus restoration              | `state.rs::escape_discards_recording_and_review_and_closes_the_pill`, `app.rs::an_accepted_transcript_is_persisted_before_it_is_submitted` for the handoff                                      |
| Local transcription failure submits nothing     | `state.rs::failed_transcription_submits_nothing_and_reports_the_reason`                                                                                                                         |
| Backend communication failure retains the text  | `state.rs::an_undeliverable_transcript_is_retained_and_resubmittable`, `activation_never_discards_an_unsent_transcript`                                                                         |
| Pill messages reach the popup conversation      | `tests/desktop.test.ts::an accepted transcript enters the conversation and is acknowledged` (delivered through `pi.sendUserMessage`)                                                            |
| Tray state transitions                          | `state.rs::tray_state_distinguishes_recording_work_attention_and_backend_failure`, `tray.rs` colour and privacy-ring tests, `tests/desktop.test.ts::assistant state prefers the active run...`  |
| Tray survives a backend crash                   | `daemon.rs::backoff_grows_and_stays_bounded`, `state.rs` `disconnected` state, `desktop-interface` check on the restart hook                                                                    |
| Restarts only the owned backend service         | `nix/checks.nix::desktop-interface` greps the generated hook for exactly `systemctl --user restart scufris-popup.service` and for the absence of any other target                               |
| Protocol version rejection                      | `scufris-control::other_protocol_versions_are_rejected`, `tests/desktop.test.ts::unknown message types and other protocol versions close the connection`, plus the live cross-process run below |
| Unknown-message rejection                       | same, plus `shared_wire_fixtures_decode_the_same_way_on_both_sides`                                                                                                                             |
| STT endpoint override                           | `config.rs::a_configured_endpoint_overrides_the_bundled_one`, `nix/checks.nix::desktop-configuration`                                                                                           |
| Bundled whisper-server path                     | `nix/checks.nix::desktop-interface` asserts the unit's host, port, and inference path                                                                                                           |
| Desktop package absent from the default closure | `nix/checks.nix::desktop-closure`                                                                                                                                                               |
| Desktop stays unobstructed outside the pill     | `pill.rs` placement tests; the window is 560x96 logical, undecorated, and bottom-anchored                                                                                                       |
| Repository checks and Nix checks                | above                                                                                                                                                                                           |
| Live playtesting                                | not done - see below                                                                                                                                                                            |

## Independent review findings (job `e0cb2bc500f5`)

Each of the four findings has a test that fails without its fix.

| Finding                                                    | Covered by                                                                                                                                                                                                                                                                  |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. A companion crash loses an accepted transcript          | `app.rs::a_restarted_companion_recovers_the_accepted_transcript_and_reuses_its_identifier`, `app.rs::an_accepted_transcript_is_persisted_before_it_is_submitted`, the five `pending.rs` tests, `state.rs::a_restored_transcript_reopens_the_pill_and_keeps_its_identifier`  |
| 2. Microphone start failure leaves a false recording state | `state.rs::a_microphone_that_never_starts_leaves_no_recording_indicator`, `app.rs::a_microphone_that_never_starts_shows_the_error_and_stops_claiming_to_record`                                                                                                             |
| 3. A lost acknowledgment can duplicate a request           | `state.rs::a_transcript_keeps_one_identifier_across_review_edits_and_retries`, `app.rs::a_delivered_submission_with_no_acknowledgment_is_retried_under_one_identifier`, `tests/desktop.test.ts::a transcript delivered before a lost acknowledgment is not delivered twice` |
| 4. A stream failure after startup is only logged           | `state.rs::a_capture_stream_that_fails_mid_recording_reports_the_same_way`, `app.rs::a_capture_stream_that_dies_stops_the_recording_and_shows_a_local_error`                                                                                                                |

Related coverage added with the fixes: a submission that was never delivered is
still retried rather than suppressed, the daemon's remembered-identifier set
stays bounded, a discarded review clears the durable copy, a failed
transcription persists nothing, and per-process identifier prefixes differ.

## Re-review findings (job `4bba52f00a85`)

| Finding                                                  | Covered by                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. A persistence failure was treated as success          | `app.rs::a_transcript_that_cannot_be_saved_is_never_submitted`, `a_review_draft_survives_a_failed_save_and_says_so`, `an_unreadable_store_is_reported_instead_of_being_read_as_empty`, `a_removal_that_keeps_failing_does_not_reopen_the_finished_pill`, `pending.rs::a_write_that_cannot_land_is_reported_instead_of_logged`, `an_unreadable_record_is_reported_rather_than_read_as_absent`, `a_directory_in_place_of_the_record_is_corrupt_not_absent`, `state.rs::a_failed_durable_save_stops_the_submission_and_keeps_the_text`, `a_failed_durable_save_during_review_is_visible_without_losing_the_draft`, `a_startup_storage_failure_is_visible_rather_than_silent` |
| 2. Stream errors were not scoped to a recording          | `app.rs::a_stream_that_fails_inside_start_releases_the_capture_it_never_installed`, `a_stale_stream_error_cannot_kill_the_recording_that_replaced_it`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| 3. Duplicate suppression had a check/await/remember race | `tests/desktop.test.ts::concurrent retries of one identifier share a single delivery`, `a failed delivery leaves no reservation, so the next retry is delivered`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| 4. Idempotency did not survive a daemon restart          | `tests/desktop.test.ts::idempotency survives a daemon restart that resumes the session`, `accepted submissions are rebuilt from the session and stay bounded`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 5. An edited ambiguous retry sent different text         | `state.rs::an_unconfirmed_transcript_is_frozen_so_the_pill_cannot_confirm_other_text`, `app.rs::a_delivered_submission_with_no_acknowledgment_is_retried_under_one_identifier`, `tests/desktop.test.ts::a reused identifier carrying different text is refused, not acknowledged`                                                                                                                                                                                                                                                                                                                                                                                         |
| 6. Socket ownership had a probe/unlink/bind race         | `tests/desktop.test.ts::only one of two daemons racing to start owns the socket`, `a stale socket left by a dead daemon is replaced`, `a failure after the listener exists leaves no unreachable server behind`                                                                                                                                                                                                                                                                                                                                                                                                                                                           |

The concurrent-start test found a real defect on its first run: the private
socket path used millisecond granularity, so three simultaneous starts bound the
same path and fought over one inode. It is now random per attempt. The daemon
test file was then run eight times in a row, passing in about 220 ms each time
and leaving no open handles, to confirm the race is gone rather than hidden.

The superseded test that asserted an edited retry under one identifier is
acknowledged has been removed; it codified finding 5 rather than preventing it.

## Third re-review findings (job `bcf747dce447`)

| Finding                                                  | Covered by                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Acknowledged before Pi accepted the message           | `tests/desktop.test.ts::nothing is acknowledged until the session actually holds the message`, `a send that Pi rejects is never acknowledged`, `a queued message lost to a restart is delivered again, not suppressed`, `a message that landed before the commit was written counts as accepted`, `an intent that cannot be recorded stops the send`, `idempotency survives a daemon restart that resumes the session`, `reconciliation trusts commits, and intents only when the message landed` |
| 2. Concurrent same-ID different bodies both acknowledged | `tests/desktop.test.ts::concurrent different bodies under one identifier are not both acknowledged`                                                                                                                                                                                                                                                                                                                                                                                               |
| 3. Stale-socket cleanup had a check/unlink race          | `tests/desktop.test.ts::a stale socket is replaced by exactly one of several racing daemons`, `only one of two daemons racing to start owns the socket`                                                                                                                                                                                                                                                                                                                                           |
| 4. A durable-record failure poisoned local suppression   | `tests/desktop.test.ts::a commit failure does not leave the process suppressing the delivery`                                                                                                                                                                                                                                                                                                                                                                                                     |
| 5. A post-claim failure left the pathname behind         | `tests/desktop.test.ts::a failure after the public socket is claimed gives the pathname back`                                                                                                                                                                                                                                                                                                                                                                                                     |
| 6. A failed explicit discard silently stayed on disk     | `app.rs::a_discard_that_cannot_be_removed_is_tombstoned_so_it_cannot_come_back`, `a_discard_that_can_neither_be_removed_nor_tombstoned_reopens_the_pill`, `state.rs::a_discard_that_cannot_happen_reopens_the_pill_on_the_reason`                                                                                                                                                                                                                                                                 |

The fake session in these tests behaves like the real one: `send` only queues,
and a message becomes an entry when the session decides to deliver it. That is
what lets the acceptance boundary be tested at all - the previous fake host made
`submit` itself awaitable and could not express the defect.

Also added, not from the report: `a_late_acknowledgment_retires_a_retained_transcript`
in both `state.rs` and `app.rs`, for the acknowledgment that arrives after the
companion stopped waiting.

The Pi behaviour behind finding 1 was read directly from the installed runtime:
`agent-session.js:1827` drops the send promise, and `:117-152` shows the user
entry being appended at `message_end`.

## Fourth re-review findings (job `5e4b41ee1483`)

| Finding                                                                         | Covered by                                                                                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Digest-only landing could acknowledge the wrong entry                        | `tests/desktop.test.ts::text already in the conversation cannot acknowledge a new submission`, `two submissions of identical text cannot share one landing`, `landings are allocated to the intent that preceded them`, `a queued message lost to a restart is delivered again, not suppressed`, `a send that Pi rejects is never acknowledged` |
| 2. Stale-lock removal was not ownership-safe, and an empty lock wedged recovery | `tests/desktop.test.ts::only one of two separate daemons takes over a lock left by a dead one` (child processes), `a daemon killed between creating its lock and writing it does not wedge the next one` (child process, SIGKILL after exclusive create), `a live daemon's lock is never broken by another starter`                             |
| 3. `message_end` fires before persistence                                       | `tests/desktop.test.ts::a landing is observed even when no further Pi event follows`, `closing the session settles pending deliveries instead of leaving them`, and the reworked `FakeSession.land`, which now emits the event _before_ appending, as Pi does                                                                                   |

The fake session was the reason finding 3 could hide: it appended the entry and
then notified, the reverse of Pi's real order. It now emits `message_end` to its
handlers first and appends afterwards, matching `agent-session.js:138-152`, and
the harness defers the notify exactly as the extension does.

Also fixed, not from the report: round four processed one connection's messages
in order, so a liveness ping could sit behind a long submission. Dispatch is
concurrent again, which the identical-text test surfaced.

The daemon test file was run three times after the changes: 39 tests, about
2.4 seconds each, exiting cleanly with no open handles.

## Fifth re-review findings (job `cc91b6c223c6`)

| Finding                                                                                             | Covered by                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1. Positional digest allocation could not identify the pill's own message                           | `tests/desktop.test.ts::same-text input typed by the user cannot acknowledge the pill`, `a restart reconciles the user's own message the same way`, `a retry while the first send is still queued does not send twice`, `a send Pi never announces is not credited by a later message`, `landings are credited only to a send no other prompt can explain`                                                                                                                                                   |
| 2. Stale-lock recovery could strand a replacement owner, and a live PID or bad token wedged startup | `tests/desktop.test.ts::a lock replaced between the decision and the removal is left alone` (barrier-controlled child process), `a lock naming a live but unrelated process is released when its lease ends`, `a lock with a malformed owner is released after the grace period`, `a live daemon's lock is never broken by another starter`, `only one of two separate daemons takes over a lock left by a dead one`, `a daemon killed between creating its lock and writing it does not wedge the next one` |

The manual-input test is the reviewer's exact sequence: the pill's intent and
send record are written, the user then types the same words, that message lands
first, and the pill is _not_ acknowledged. Only when the pill's own message
arrives does the acknowledgment follow, and the conversation ends with two
distinct entries rather than one entry acknowledged twice.

The replacement-race test drives a real child process to the point where it has
judged a lock abandoned, pauses it there through a file barrier, installs a
different owner's lock, and then releases it. The child declines and reports
busy, and the replacement owner's lock is still byte-identical afterwards.

The fake session now emits Pi's `input` event with a source, as well as
`message_end` before appending. Both are required to reproduce these defects.

The daemon test file was run three times after the changes: 46 tests, about
3.6 seconds each, exiting cleanly.

## Sixth re-review findings (job `50303a6e9864`)

| Finding                                                             | Covered by                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. `input.source` cannot correlate a record to this daemon's prompt | `tests/desktop.test.ts::neither the user nor another extension can acknowledge the pill`, `nothing is acknowledged until the session actually holds the transcript`, `a send Pi refuses is never acknowledged`, `a restart reconciles from the transcript the session holds`, `a queued send lost to a restart is delivered again, not suppressed`, `a retry while the first send is still queued does not send twice`, `only this daemon's own transcripts are read back from a session` |
| 2. Lease expiry let the old holder unlink a live socket             | `tests/desktop.test.ts::a holder whose lease ran out cannot unlink the successor's live socket`                                                                                                                                                                                                                                                                                                                                                                                           |
| 3. Rust and TypeScript bounded transcripts differently              | `scufris-control::the_transcript_bound_is_utf8_bytes_on_both_sides`, `pending.rs::non_ascii_text_survives_the_round_trip_it_was_accepted_for`, `tests/desktop.test.ts::a transcript at the byte bound is accepted and one past it is not`                                                                                                                                                                                                                                                 |

Finding 1 is answered by construction rather than by another test: the
transcript is sent as a message carrying its own identifier, so a message from
the user or from another extension has neither this daemon's message type nor
its identifier. The test exercises exactly that - the user types the pill's
words and another extension sends them under a colliding `details.id`, and
neither acknowledges anything.

The lease test was confirmed to fail without its fix: with the recheck before
`rmSync` removed, the stalled child proceeds and the assertion "the stalled
holder carried on after losing its lease" fires.

The daemon test file was run three times after the changes: 42 tests, about
3.8 seconds each, exiting cleanly.

## Seventh re-review findings (job `8fe508f18eb9`)

| Finding                                                           | Covered by                                                                                                                                                                                                                                                                                                      |
| ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. A custom-message turn skipped the whole prompt preflight       | `tests/desktop.test.ts::a transcript starts the same turn a typed prompt starts`                                                                                                                                                                                                                                |
| 2. Both lock releases checked a pathname and then unlinked it     | `tests/desktop.test.ts::the ownership lock is released the moment its holder stops existing`, `no starter can interleave with the unlink of a stale socket`, `no starter can interleave with the unlink a departing daemon performs`, `only one of two separate daemons takes over a socket left by a dead one` |
| 3. A conflicting retry outside the bounded cache was acknowledged | `tests/desktop.test.ts::a conflicting retry is refused even after it left the daemon's cache`, `every body a reused identifier landed is acknowledged and nothing else`, `only this daemon's own transcripts are read back from a session`, `app.rs::identifier_prefixes_are_random_and_wire_safe`              |
| 4. The popup showed an internal type as a message label           | `tests/desktop.test.ts::a transcript starts the same turn a typed prompt starts` asserts the renderer is registered; the transcript itself is now a user message                                                                                                                                                |

Finding 1 drove the design change the other three are built on. The integration
test runs the desktop extension against a Pi that reproduces `prompt()`'s order

- the input event, the pre-send compaction check, then `before_agent_start` -
  and asserts that a submission arriving over the real socket produces a turn
  carrying the per-turn Scufris system prompt, a receipt, and an acknowledgment.
  The fake Pi throws if the extension reaches for `sendMessage`, which is the path
  that skipped all of it.

Finding 2 removed rather than repaired the racy code. There is no lock file, no
lease, no grace period, and no abandoned-lock recovery left to test; the tests
that replace them hold a child process at the barrier directly before each
unlink and link and show a successor cannot reach the pathname, and kill a
holder outright to show the kernel releases the name at once.

The barrier tests were confirmed to fail without their fix: with the socket
removal in `stop()` moved back outside the ownership lock, `no starter can
interleave with the unlink a departing daemon performs` fails, because the
successor claims the pathname while the departing daemon is still holding it.

## Eighth re-review findings (job `ac06ce15b577`)

| Finding                                                         | Covered by                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1. Receipt correlation trusted Pi's source class                | `tests/desktop.test.ts::a transcript starts the same turn a typed prompt starts`, `a spoken prompt queued behind a running turn is acknowledged`, `a prompt another extension sends does not acknowledge the pill`, `another extension's identical prompt is never taken for the pill's`, `a prompt a later handler answers itself never acknowledges the pill`, `a prompt rewritten before this daemon sees it is not acknowledged`, `a prompt rewritten after this daemon sees it is not acknowledged`, `a prompt typed while a spoken one waits leaves the pill unacknowledged`, `a restart is acknowledged from the session, not sent again` |
| 2. The abstract lock missed path aliases and network namespaces | `tests/desktop.test.ts::one daemon owns a socket whatever name reaches it`, `a daemon in another network namespace is serialized by the same lock`, and the existing barrier and crash tests, which now run against the file lock                                                                                                                                                                                                                                                                                                                                                                                                                |

The correlation tests run the real extension against a Pi that reproduces the
ordered input chain, including handlers registered before and after this
extension, a handler that rewrites a prompt in either position, a handler that
answers one itself, prompts from the person and over RPC, prompts queued behind
a running turn, and a restart. The fake Pi calls `sendUserMessage` directly, as
the runtime does (`agent-session.js:1854-1861`), so the asynchronous context the
correlation depends on behaves as it does in Pi.

The namespace defect and its fix were measured directly before choosing the
design. Two `node` processes, one under `unshare -rn`, both bound the same
abstract name; the same two processes taking `flock` on one file did not:

```
bound
host again: error EADDRINUSE
in netns:   bound
flock host:   busy
flock netns:  busy
```

Falsification, each confirmed by making the change and watching the named test
fail, then restoring it:

- `canonicalSocketPath` returning its argument unchanged fails `one daemon owns
a socket whatever name reaches it`.
- Dropping the only-prompt-in-flight rule from `ReceiptLedger.land` fails
  `a prompt another extension sends does not acknowledge the pill`, `a prompt a
later handler answers itself never acknowledges the pill`, and `a prompt typed
while a spoken one waits leaves the pill unacknowledged`.

## Ninth re-review findings (job `1bc51bd711ee`)

| Finding                                                                   | Covered by                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1. An orphan receipt or a branch taken at one could falsely acknowledge   | `tests/desktop.test.ts::acceptance is read back from what was committed, never from what is beside it`, `a session says accepted, uncertain, or unsent, and never guesses`                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2. Under-crediting permitted duplicate execution after restart or timeout | `tests/desktop.test.ts::a request that was dispatched and never landed is uncertain, not unsent`, `only the person's own decision sends an uncertain request again`, `a prompt that landed uncredited is uncertain rather than acknowledged`, `a landing that never comes leaves the request uncertain, never resent`, `a send the session refuses leaves nothing uncertain behind`, and `state.rs::an_uncertain_transcript_is_never_resent_without_the_person_saying_so`, `app.rs::a_restarted_companion_recovers_the_accepted_transcript_and_reuses_its_identifier`, `a_delivered_submission_with_no_acknowledgment_is_retried_under_one_identifier` |

The first finding's two shapes are covered as shapes rather than as stories,
because that is what they are: a commit whose named entry is absent from the
branch is what a crash between two appends leaves, and a commit whose named
entry is a stranger's prompt with the same words is what a branch taken at the
prompt leaves. Both are refused, and so is a commit whose named entry carries
different words.

The second finding is covered at both ends. The daemon refuses an uncertain
submission on the wire, after a landing timeout, after its own restart, and when
a prompt landed uncredited; the companion holds it as its own state, offers copy
and discard, and sends it only after the person has been told what that could
repeat and has said so twice.

Falsification, each confirmed by making the change and watching the named tests
fail, then restoring it:

- Dropping the check that a commit's named entry is a prompt with those words
  fails `acceptance is read back from what was committed, never from what is
beside it`.
- Dropping the uncertain refusal from `SessionAcceptance.deliver` fails `a
landing that never comes leaves the request uncertain, never resent`, `a
request that was dispatched and never landed is uncertain, not unsent`, and `a
prompt that landed uncredited is uncertain rather than acknowledged`.

## Tenth re-review findings (job `7f20da8fd738`)

| Finding                                                                   | Covered by                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1. A commit could bind to the wrong entry on the active branch            | `tests/desktop.test.ts::a commit names the entry this prompt became, not one that resembles it`, `an append that never happened commits nothing, whatever fills its place`, `an identical prompt landing beside a spoken one leaves it uncommitted`, `a branch taken while a prompt lands leaves it uncommitted`, `a session replaced while a prompt lands cancels its commit`                                                                                                                                                                         |
| 2. A definitely refused submission had no submission-specific wire result | `tests/desktop.test.ts::a send the session refuses leaves nothing uncertain behind`, `a reused identifier carrying different text is refused, not acknowledged`, `concurrent different bodies under one identifier are not both acknowledged`, and `app.rs::a_daemon_refusal_keeps_the_words_editable_and_ordinarily_retriable`, `state.rs::a_refused_send_says_so_and_keeps_the_words_editable`, `a_refusal_of_a_forced_send_does_not_forget_the_earlier_uncertainty`, `daemon.rs::the_link_greets_the_daemon_and_forwards_state_and_acknowledgments` |
| 3. The companion ignored the identifier on an uncertain answer            | `app.rs::a_late_answer_for_a_retired_submission_leaves_the_new_one_alone`, `state.rs::an_answer_for_another_submission_never_settles_the_current_one`, `daemon.rs::the_link_greets_the_daemon_and_forwards_state_and_acknowledgments`                                                                                                                                                                                                                                                                                                                  |

The first finding is covered at the four boundaries the report named. The commit
is created against the leaf captured while the prompt was landing rather than by
scanning for words: an append that never happened leaves that place for somebody
else's prompt to fill, which supersedes the landing instead of receiving it; a
second identical prompt in the same moment leaves both uncredited; a branch taken
in between removes the anchor; and a replaced session cancels the pending commit
before the append that follows it.

The second finding is covered on the wire and in the companion. The refusal
names the submission and reaches only the peer that asked; the real companion
state machine keeps those words editable and sends them again on one Enter; and
a refusal of a forced send returns the transcript to uncertain rather than
making a request that may already have run editable.

The third finding is covered by driving the real companion runtime through
`App::observe`, which is the mapping `main.rs` now uses, with an answer for a
submission that was already retired arriving while a second one is in flight.

Falsification, each confirmed by making the change and watching the named tests
fail, then restoring it:

- Letting a landing that is not this daemon's leave the previous one in place
  fails `an append that never happened commits nothing, whatever fills its
place` and `an identical prompt landing beside a spoken one leaves it
uncommitted`.
- Restoring the newest-matching-digest scan in place of the captured anchor
  fails `a branch taken while a prompt lands leaves it uncommitted`.
- Dropping the identifier guard from the `SubmissionFailed` and
  `SubmissionUncertain` transitions fails
  `state.rs::an_answer_for_another_submission_never_settles_the_current_one` and
  `app.rs::a_late_answer_for_a_retired_submission_leaves_the_new_one_alone`.

## Eleventh re-review findings (job `43fda6a60b29`)

| Finding                                                   | Covered by                                                                                                                                                                                                        |
| --------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. The mutating process did not hold the kernel lock      | `tests/desktop.test.ts::a daemon whose lock helper dies changes nothing afterwards`, `a successor takes over from a daemon whose lock helper died`, `no starter can interleave with the unlink of a stale socket` |
| 2. Submission did not restore focus                       | `state.rs::the_desktop_comes_back_when_the_words_are_handed_off`, `a_handed_off_transcript_brings_the_pill_back_when_it_goes_wrong`, `app.rs::a_late_acknowledgment_retires_a_retained_transcript`                |
| 3. The working-tree backend omitted the desktop extension | `tests/dev_helper.test.ts::expectedArgs`, and `docs/src/dev/maintenance.md` for the `SCUFRIS_DAEMON=1` invocation                                                                                                 |

The stale-socket barrier test now stalls the lock helper itself, between its
probe and its unlink, rather than stalling the daemon before it asks for the
mutation. That is the window the finding is about, and it is now inside the
process that holds the lock.

Falsification, confirmed by making the change and watching the named test fail,
then restoring it: giving `OwnershipLock.claim` back its old behaviour - link
here after checking that the helper has not ended - fails `a daemon whose lock
helper dies changes nothing afterwards` with a missing rejection.

## Twelfth re-review findings (job `36df4990ed48`)

| Finding                                                         | Covered by                                                                                                                                                                                    |
| --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. A fast daemon answer during the handoff left the pill hidden | `app.rs::an_answer_that_arrives_during_the_handoff_leaves_the_pill_on_screen`                                                                                                                 |
| Immediate focus restoration for a normal handoff, kept          | `app.rs::an_accepted_transcript_is_persisted_before_it_is_submitted`, `a_late_acknowledgment_retires_a_retained_transcript`, `state.rs::the_desktop_comes_back_when_the_words_are_handed_off` |
| A failed window operation cannot stop the ones after it         | `app.rs::a_window_operation_that_fails_does_not_freeze_the_pill`                                                                                                                              |
| The window order the action list used to carry                  | `app.rs::an_accepted_transcript_is_persisted_before_it_is_submitted` asserts `show, restore, hide`, `the_pill_opens_before_the_microphone_does`                                               |

The new test stops the submitting thread inside the write, where the daemon's
answer really arrives, and injects a refusal, an uncertainty, and both together -
three cases, each applied on another thread that is joined before the write
returns. Every outcome needs the person, so every one must end with the pill on
screen and with no hide and no focus restoration behind it. The window is
asserted as the surface's own state, not as a count of calls.

The window ordering also has to survive a surface call that fails, because one
thread applies decisions on behalf of the others: the second test fails a show
and then proves that a later decision still reaches the window.

The state machine no longer emits show and hide, so its tests assert where the
pill belongs (`Companion::on_screen`) and the runtime's tests assert where it
actually is. What the action list used to carry is asserted where it now
happens: focus returns before the pill goes, and the pill is up before the
microphone opens.

Falsification, each confirmed by making the change and watching the named test
fail, then restoring it:

- Dropping the version guard in `Ordered::apply`, so a superseded decision
  reaches the surface after the newer one, fails
  `an_answer_that_arrives_during_the_handoff_leaves_the_pill_on_screen` with the
  pill hidden in `retained`.
- Leaving the `Applying` guard disarmed, so a failed surface call keeps the
  surface claimed, fails `a_window_operation_that_fails_does_not_freeze_the_pill`
  with a pill that never opens again.

## Re-review findings (job `0f877530611c`)

The production adapters swallowed every Tauri failure while the runtime recorded
the visibility it had asked for, so a window state that never happened could be
recorded as final. `Surface` now reports what each operation achieved, and the
fakes model that the way production does: they return an error rather than
panicking. The existing panic test stays, because unwinding is a different path
and the guard that hands the surface back still has to hold.

| Failure                                              | Where                                                                                                                                                                                        |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A show that returns an error, before the microphone  | `app.rs::a_pill_that_will_not_come_up_never_opens_the_microphone`                                                                                                                            |
| A show that puts the pill up without the keyboard    | `app.rs::a_pill_that_comes_up_without_the_keyboard_is_asked_again`                                                                                                                           |
| A hide that returns an error                         | `app.rs::a_pill_that_will_not_go_down_takes_itself_down`                                                                                                                                     |
| A presentation the pill refuses                      | `app.rs::a_presentation_that_reaches_nothing_is_tried_again`                                                                                                                                 |
| A tray refusal under a newer, versioned presentation | `app.rs::a_newer_presentation_overtakes_one_the_tray_refused`                                                                                                                                |
| What a phase with no window does                     | `state.rs::a_pill_that_cannot_be_opened_stops_the_recording_and_says_so_on_the_tray`, `abandoning_an_accepted_transcript_never_throws_it_away`, `abandoning_during_transcription_cancels_it` |

The two interleaving tests drive the second change on its own thread and join
it, so the interleaving is real and the assertions are still deterministic.

Falsification, each confirmed by making the change and watching the named test
fail, then restoring it:

- Recording a failed show as a pill that came up fails
  `a_pill_that_will_not_come_up_never_opens_the_microphone` with a microphone
  open behind an indicator that is not there.
- Running the phase's actions even though its window never came up fails the
  same test the same way.
- Recording a pill that would not go down as down fails
  `a_pill_that_will_not_go_down_takes_itself_down` with an always-on-top pill
  left over the desktop.
- Recording a pill that never took the keyboard as ready fails
  `a_pill_that_comes_up_without_the_keyboard_is_asked_again`, which then never
  asks the window again.
- Trying a refused presentation only once fails
  `a_presentation_that_reaches_nothing_is_tried_again`.
- Retrying a refused presentation past a newer one fails
  `a_newer_presentation_overtakes_one_the_tray_refused`, which ends on a state
  the companion had already left.

## Re-review findings (job `3a92f439c282`)

Five findings, all closed. The window and what the surfaces say are one ordered
decision now, so one thread applies all of it and the runtime can ask whether
what the person can read is what the phase is in.

| Failure                                                             | Where                                                                              |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| A presentation the pill refuses, before the microphone opens        | `app.rs::the_microphone_stays_shut_behind_a_pill_that_rendered_nothing`            |
| The tray under the same refusal                                     | same test: the tray still takes `listening`                                        |
| Always-on-top refused while a capture is running                    | `app.rs::a_pill_that_may_be_behind_another_window_stops_the_recording`             |
| A key pressed while another thread is inside a surface operation    | `app.rs::a_key_pressed_while_the_surface_is_busy_waits_for_what_it_needs`          |
| A show that puts the pill up without the keyboard                   | `app.rs::a_pill_that_comes_up_without_the_keyboard_is_asked_again`                 |
| A hide that returns an error, recovered with no key from the person | `app.rs::a_pill_that_will_not_go_down_takes_itself_down`                           |
| A window that refuses every time                                    | `app.rs::a_window_that_keeps_refusing_is_not_asked_forever`                        |
| A socket path with spaces in it                                     | `tests/desktop.test.ts::a socket path with spaces in it is claimed and given back` |

The concurrency test drives the key press on its own thread and joins it inside
the surface call, so the interleaving is real, the applying thread is genuinely
mid-render, and the assertions are still deterministic. It records what the
microphone had done at the moment the key press returned, which is the thing
that must be nothing.

Falsification, each confirmed by making the change and watching the named tests
fail, then restoring it:

- Dropping the `StartRecording` precondition fails
  `the_microphone_stays_shut_behind_a_pill_that_rendered_nothing`: the
  microphone opens with nothing on screen saying so.
- Counting a doubtful pill as visible fails
  `a_pill_that_may_be_behind_another_window_stops_the_recording`: the capture
  keeps running behind a window nothing proves the person can see.
- Removing the repair chain fails four tests -
  `a_pill_that_will_not_go_down_takes_itself_down`,
  `a_window_that_keeps_refusing_is_not_asked_forever`,
  `a_pill_that_comes_up_without_the_keyboard_is_asked_again`, and
  `a_pill_that_may_be_behind_another_window_stops_the_recording`.
- Answering a queued caller with success instead of leaving its work for the
  applying thread fails
  `a_key_pressed_while_the_surface_is_busy_waits_for_what_it_needs`: the
  activation's actions run against a pill that is not up yet, so the pill ends
  on `error` rather than `listening`.
- Skipping the tray when the pill refuses fails
  `the_microphone_stays_shut_behind_a_pill_that_rendered_nothing`: the only
  surface left that could say anything is not told.
- Reverting both sides of the lock helper to the space-delimited protocol fails
  exactly `a socket path with spaces in it is claimed and given back` (65 of 66
  pass).

## Cross-implementation evidence

The protocol is implemented twice, in Rust and in TypeScript. Two things guard
against drift:

1. `desktop/control-protocol-v1.json` holds the exact wire lines. Both suites
   read it: `scufris-control::shared_wire_fixtures_decode_the_same_way_on_both_sides`
   and `tests/desktop.test.ts::both protocol implementations agree on the same
wire fixtures`.
2. A one-off cross-process run in this workspace drove the real Rust codec
   against the real TypeScript server over a real Unix socket. The first prompt
   was answered by Pi itself and never landed, the daemon then restarted, and the
   person typed the pill's exact words afterwards:

```
welcome:  Welcome { session: "cross-check" }
first:    Uncertain { id: "3f2a.1", detail: "... was dispatched and its outcome is unknown ..." }
retry:    Uncertain { id: "3f2a.1", detail: "... was dispatched and its outcome is unknown ..." }
forced:   Ack { id: "3f2a.1" }
after:    Ack { id: "3f2a.1" }
edited:   State { state: Error, detail: "submission 3f2a.1 was already accepted with different text" }
=== daemon session ===
entries ["dispatch","dispatch","prompt","accepted","prompt"]
```

A dispatched request whose outcome was unknown was refused, and refused again on
an ordinary retry across a daemon restart. Only the forced submission sent it.
Once committed, a further ordinary submission was acknowledged from the commit
without being sent again, and an edited retry under the same identifier was
refused. The session holds two prompts - the pill's and the person's identical
one - and exactly one acceptance.

## Not verified here

This workspace has no X display, microphone, speaker, or running Scufris
backend, so the following need a live session before the task closes:

- Super+D activation under i3, including whether the accelerator collides with
  an existing i3 binding.
- Microphone capture quality and transcription accuracy against a real
  whisper-server, and the recording level driving the orb.
- Focus restoration against real windows.
- Tray rendering and menu behaviour in the real system tray.
- Speech output raising and clearing the `speaking` tray state end to end.
- A real backend crash and the bounded restart action.
- Session resume: reconnecting to a popup that restarted.
- Which Tauri and WebKitGTK failures actually reach the adapters, and whether a
  window that has died answers `is_visible` at all. The runtime treats a window
  that cannot answer as one that is not up; that is the safe reading, but only a
  live session shows what a dying webview really returns.
- A webview that dies while the pill sits open. Nothing watches the window
  between decisions, so this is noticed at the next window operation or repair.
- Whether a real window manager that refuses `set_always_on_top` or `set_focus`
  answers differently on the next attempt. The repair chain assumes asking again
  is worth three tries; only a live session shows what the refusal rate is.
- Whether a real `XDG_RUNTIME_DIR` or configured socket path with spaces occurs
  in practice. The helper is tested with one, but no live session has used one.

The bundled `ggml-base.bin` model download was prefetched and hashed, but the
bundled whisper-server has not been started against a real recording.
