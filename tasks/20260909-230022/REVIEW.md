# Review - G2 and G3

Round 2, 2026-09-10. Completes the coverage the nightly review left open.
Reviewer: this session, adjudicating eight unattended lanes.

- G2 `997e057..19ce35a` - desktop surface, den helper, widgets, composer.
- G3 `19ce35a..474a7fa` - refusal codes, release, jobs, briefings.

Four lanes per group: craft, correctness, desktop, contracts. The red-team lane
was omitted at the owner's instruction after the previous run's lane caused a
machine-wide OOM. Every lane carried an explicit safety clause derived from that
incident; see "OOM diagnosis" in `TASK.md`. No Feel lane: unattended, no `--live`.

There is no X on this machine, so both desktop lanes reasoned statically and
reported display judgements as unharnessed. That is a skip, not a pass.

## Verdict

Both groups carry defects that ship. Two BLOCKERs, thirteen MAJOR, and a long
MINOR tail. Two lane-filed BLOCKERs were rejected on adjudication because later
commits already repaired them.

The dominant theme is not any single defect. It is that four of the commits
reviewed here shipped a test that certifies behaviour it does not exercise.

## BLOCKER

| ID  | Where                                                               | Claim                                                                                                                                                                                                                                                                                                                                                                                             | Corroboration                                                                                                                                                                                                                          |
| --- | ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| N1  | `surfaces/desktop/backends/den/backend.py:445`                      | A non-UTF-8 byte in a journal day file kills the den backend through `suggest`. `19ce35a` widened `Panel.read` to survive it, which is what makes the panel exist to type into; `lift_history` at `:445` and the `act` guard were not widened. Every keystroke in the lift form's exercise or split field kills the process, the restart tick respawns it, and the next keystroke kills it again. | 2 lanes, both with executed bounded repro. Re-derived here: `foods` at `:481` wraps `(OSError, ValueError) -> Refused`; `suggest` wraps nothing.                                                                                       |
| N2  | `surfaces/desktop/src/state.rs:1017`, `ui/pill.ts`, `ui/textbox.ts` | The copy-refusal feature `19ce35a` adds reaches no surface. `Presentation.detail` is carried on the wire and read by no page, and `Phase::Editing` has no `tray_presentation` arm, so it falls through `_ => {}` to the service's resting state. A refused copy changes nothing on the pill, the textbox, or the tray, and writes no `warn!`.                                                     | 1 lane. Re-derived here: `grep detail surfaces/desktop/ui/*.ts` finds only the `tauri.d.ts:40` declaration and `hud.ts`'s unrelated `entry.details`; `state.rs:1021-1030` has arms for Listening, Transcribing, Failed, Retained only. |

## MAJOR

| ID  | Where                                                                          | Claim                                                                                                                                                                                                                                                                                                                                                                                                                                                     | Corroboration                                                                               |
| --- | ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| N3  | `backends/claude/backend.py:57`, `codex/backend.py:52`, `system/backend.py:38` | Each poll ceiling equals the staleness tolerance. `cadence=60000` x `SILENCE=3` = 180 s `overdue`; `CEILING=180.0`. The loops sleep around the work, so print-to-print is `every + reading()`, and `quiet` over-counts by up to one 250 ms `BEAT`. The manifest advertises 180 to the model as the slowest safe value, steering callers onto the one value that badges STALE every cycle.                                                                 | **4 independent lanes.** Arithmetic re-derived here.                                        |
| N4  | `tools/den/den.py:1040`                                                        | `normalize_split` calls `_plain`, which refuses commas and newlines and nothing else. `set_split` writes caller text as its own line, so `### Habits` forges a section header: the Workout split, table header and every set stop being reachable through any reader while the bytes stay in the file. Reachable from `den gym split` and from a model-written widget payload.                                                                            | 2 lanes, both with executed repro. `_plain` re-derived here: no `_structure` call.          |
| N5  | `surfaces/desktop/src/state.rs:810`                                            | `Event::CopyFailed` in `Phase::Retained` replaces `reason` wholesale. `reason` is the sole carrier of why the words are held and the only on-screen statement of what Escape and Enter do, and for `Delivery::Uncertain` it carries `UNCERTAIN_CHOICES`. It also leaves `warned: true`, so the next Enter force-sends a possibly-delivered turn with the warning gone from every surface. `Phase::Editing` gets this right with a separate `notice` slot. | 3 lanes, complementary framings.                                                            |
| N6  | `host/service/src/bin/scufris-surface-gateway.rs:608,629,637`                  | `474a7fa` imported `refusal` into this file, converted one of four codes at `:623`, and left three literals - one in the same function as its converted sibling. `tests/service.test.ts` compares the two lists to each other, never to a send site, so it structurally cannot catch this.                                                                                                                                                                | **4 independent lanes.** Re-derived here.                                                   |
| N7  | `surfaces/desktop/src/attachment.rs:355`                                       | The rename was applied inside a raw byte string: `br#"{"error":{"code":refusal::INVALID_ATTACHMENT,...}"#`. That is not JSON, so `serde_json` returns `None`, `code` is `None`, and the test passes through the status-only arm. The `(_, Some(INVALID_ATTACHMENT))` half is now covered by nothing.                                                                                                                                                      | **4 independent lanes.** Re-derived here.                                                   |
| N8  | `tools/jobs/scufris-jobs:1870`                                                 | `report_launch_failure`'s docstring names "a capability that no longer authorizes this generation" among what it reports. That check raises at `:1730`, seven lines above the `try:` at `:1737`, so the case still exits silently and the job stays at `worker starting`.                                                                                                                                                                                 | 3 lanes. **See conflict resolution below.**                                                 |
| N9  | `agent/extensions/scufris/workflow/worker-report.ts:45`                        | CHANGELOG claims the summary is bounded "in bytes, in all three places that write one". This door declares `maxLength: 500`, which JSON Schema counts in characters. The commit's own comment at `:39-41` states the gap. A 250-character summary with em dashes is admitted here and refused two processes away with a message naming neither field nor bound.                                                                                           | 3 lanes.                                                                                    |
| N10 | `tools/briefing/briefing.py:186`                                               | `collected_runs` widened to include `failed`, and `resolve` is a second consumer that treats every entry as a candidate. A date holding one failed and one collected run now refuses where it used to answer. `resolve`'s docstring still says "gathered and still waiting for its prose"; a failed run was never gathered.                                                                                                                               | 3 lanes; one proved it by running the same bounded fixture against `19ce35a` and `474a7fa`. |
| N11 | `tests/desktop-ui.test.ts:853`                                                 | `577275e` added `SERVICE_STATES` to `tray.rs` precisely because "the tray's own test list held the nine names the tray happened to implement". The page half - `BASELINE`, `ORB_LOOKS`, `boundaryCue` - got the same fix and no mirrored guard. Delete `failed: 0.22` from `BASELINE` and every suite stays green while a dead agent breathes at idle amplitude.                                                                                          | 1 lane.                                                                                     |
| N12 | `surfaces/desktop/src/app.rs:2338`                                             | `a_restart_the_budget_refuses_is_said_on_the_pill` builds its harness with `restart_command: None`, so `restart_backend` returns at the first guard and the budget branch never executes. The tray item is `.enabled(restart_available)`, so it is disabled in exactly the state whose sentence the test asserts.                                                                                                                                         | 1 lane. Re-derived here.                                                                    |
| N13 | `agent/extensions/scufris/service/protocol.ts:347,357,431,434`                 | `invalid_widgets` is a wire code (`refusal.rs:52`, sent `service.rs:546`) declared as `REFUSAL.INVALID_WIDGETS` at `:154` and then written as a bare literal four times in the same file. The header comment implies `ProtocolError` carries only process-local codes; this one leaves the process.                                                                                                                                                       | 2 lanes. Re-derived here.                                                                   |

## Rejected on adjudication

| Filed                                                                                                     | Severity filed      | Verdict                                                                                                                                                                                                                |
| --------------------------------------------------------------------------------------------------------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `nix/checks/service.nix:88` asserts a deleted `RuntimeDirectory`, failing `nix flake check` at evaluation | BLOCKER, by 2 lanes | **SUPERSEDED.** Real at the range tip `474a7fa`; `ad6b97f` replaced the assert with an explanatory comment. Not live at HEAD. Both lanes found the repair themselves and said so.                                      |
| `ruff format --check` fails on three range files                                                          | MAJOR               | **SUPERSEDED.** All four named files are formatted at HEAD. The lane attributed the repair to `05feaa2`; that is wrong - `05feaa2` touches only this task file. The actual repair is `b32dd6e`, inside G3's own range. |
| `den.py:1683` over-long line                                                                              | MINOR               | **SUPERSEDED.** Corrected at HEAD.                                                                                                                                                                                     |

## Conflict resolution: N8

Two lanes filed N8 with opposite fixes.

- Contracts: "Move `load_job`/`load_report_auth`/the capability check inside the
  guarded region."
- Craft, and independently correctness: "Drop the capability clause from the
  docstring - this runs past the capability gate, and an unauthorized launch is
  intentionally not reported."

**Craft and correctness are right.** `c625e37`'s own message states the intent:
"The report sits past the capability check, so an unauthorized caller cannot use
it to mark a running job failed." Contracts' recommendation would introduce a
security regression - any caller able to invoke `launch` with a bad capability
could publish a `failed` event against a live job.

What survives from contracts is the availability half: a job whose auth file is
missing or truncated does leave the foreground reading `worker starting` forever.
That is real, and it must not be fixed by widening the `try` upward. The fix is
to correct the docstring now, and to handle the stuck pane separately by having
the orchestrator notice a pane that exited with no terminal event.

Correctness additionally observed that `report_launch_failure` passes
`expected_generation=job["generation"]` from the job it just reloaded, comparing
a value against itself, so `write_report`'s generation guard is a tautology on
this path. Its sibling `publish_harness_completion` takes the value from its
caller. Recorded as MINOR; no firing scenario was grounded.

## The cross-cutting finding

Eight tests across the two reviews are green while proving nothing, or while
proving a path that cannot be reached:

1. `attachment.rs:355` - fixture is not JSON; the code arm is unreached.
2. `app.rs:2564` - asserts `presentation.detail`, a field no page reads.
3. `app.rs:2338` - never reaches the restart budget it is named for, and the
   product path it describes is a disabled menu item.
4. `tests/service.test.ts:314` - compares two lists to each other, not to their
   send sites; a code missing from both passes green.
5. `tests/service.test.ts:328` - the regex silently skips any declaration it
   cannot match; the only backstop is `size > 0`.
6. `tests/desktop-ui.test.ts:853` - the page's service-state list has no guard.
7. `tests/test_den.py:304` - the name asserts a reader/writer lockstep the body
   never measures.
8. F0 - ambient `SCUFRIS_BRIEFING_*` silently overrides the bounds under test.

Plus G4-R10 from the first review, whose fixture cannot fail. Four of these were
introduced by the very commits that claimed to fix the thing they do not test.

This is why "master is green" cost the first review a full diagnostic pass. It
deserves its own workstream, not nine separate cleanups.

## Verified

- Every load-bearing claim above was re-derived in this session against HEAD
  `5b16f45`, which has no implementation diff from the reviewed revisions.
- `nix develop --command ruff format --check .`: 251 files formatted, clean.
- Lane-run suites, at the ranges rather than at HEAD: `cargo test -p
scufris-desktop` 330 passed; `python3 -m unittest tests.test_den
tests.test_den_backend` 151 passed; `tests/test_briefing.py` 101 passed;
  `tests/test_scufris_jobs.py` 51 passed; `TMPDIR=/tmp npm test` 121 passed
  inside the dev shell.
- The `resolve` regression was proved by executing the same bounded fixture
  against both `19ce35a` and `474a7fa`.

## Not verified

- `nix flake check`. Forbidden by the reviewer contract; CI owns it.
- Anything needing a display. No X, no `xdotool`/`xwininfo`. Every judgement
  about what the pill, tray, orb, shelf, or webview clipboard actually draw is
  unharnessed, including both BLOCKER N2's user-visible half and the WebKitGTK
  clipboard path.
- The G4 red-team lane, omitted by instruction. G4 remains 4/5.
- The claude and codex network paths; N3 rests on loop shape and arithmetic.
- The iOS Swift side of the gateway's refusal vocabulary.

A skip is not a pass.
