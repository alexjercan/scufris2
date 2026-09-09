# Verification

Protocol version 7. Ran on 2026-09-09 at the implementation commit.

## Checks

| Command                                                          | Result                    |
| ---------------------------------------------------------------- | ------------------------- |
| `cargo test --workspace`                                         | 24 + 330 + 51 + 9 passed  |
| `npm run check` (`tsc --noEmit`, node tests, `prettier --check`) | 119 passed, 0 failed      |
| `python3 -m unittest discover -s tests -p 'test_*.py'`           | 375 passed                |
| `nix flake check`                                                | all checks passed         |
| `tools/release/check_versions.py`                                | product 2.3.0; protocol 7 |

## What the design asked for

- A final response with two citations, four badges, and one offer validates; a
  citation with seven badges or three offers does not:
  `tests/service.test.ts` "badges are grouped by job and an offer names one
  offer in the message", `shared/control/src/service.rs`
  `badges_are_grouped_by_job_and_one_job_gets_one_group`,
  `surfaces/ios/Tests/ProtocolTests.swift`
  `badgesAreGroupedByTheJobTheyAreAbout`.
- `offer.take` submits the stored prompt, adds no user entry, and a second
  take of the same id does nothing: `tests/response.test.ts` "an offer is a
  label over words the surface never sees". The host side is idempotent in
  `shared/control/src/service.rs`.
- Two live jobs produce two rows; both finishing leaves two terminal rows;
  archiving both leaves zero: `tests/agents.test.ts` "a row outlives its job,
  and filing it is what clears it".
- The tray word stays `failed` while a failed row is unfiled:
  `host/service/src/service.rs`
  `the_tray_word_is_folded_from_the_rows_and_a_failed_row_holds_it`.
- A failed event drain is a row and archiving it is the acknowledgement:
  `tests/agents.test.ts` "a drain that stopped is a row, because nothing else
  reports it".
- The HUD draws the strip, the offer, and the rows, and the stop control arms
  before it fires: `tests/desktop-ui.test.ts` "badges are grouped by job and
  led by the job they are about", "an offer sends its identifier and is marked
  spent by the service", "a job row outlives its job and the list is the last
  thing in the flow", "stopping a job arms before it fires and forgets it was
  pressed", and "two or more finished rows can be filed at once".

## Not verified here

- The iOS changes are not compiled. No Swift toolchain is installed on this
  machine, so `surfaces/ios/` is reviewed against
  `surfaces/ios/Tests/ProtocolTests.swift` and the Rust validator only.
- No staging run with a job in flight. The design asks for one; it needs a
  real worker and a rebuilt desktop.
