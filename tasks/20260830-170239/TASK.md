# Show transient thinking feedback on every surface

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: ios, surface, presentation

## Goal

Show a transient transcript-adjacent `SCUFRIS thinking...` row on the iOS and
desktop conversation surfaces while the service reports
`surface.state: working`.

## Decisions

- This is presentation state, not a canonical conversation message.
- Use the existing protocol-v4 state event. Do not change protocol schemas.
- Remove the row on a final response, idle, blocked, failed, or disconnect.
- Keep the composer available while Scufris works.
- Match each surface's existing speaker-column typography and state colors.
- Desktop and iOS derive the same presentation from the same service state;
  neither surface synthesizes a canonical assistant message.

## Acceptance

- Working state adds one visible thinking row and scrolls it into view.
- Repeated working events do not duplicate it.
- Every terminal state removes it deterministically.
- Canonical replay never contains it.
- Desktop UI tests and iOS simulator tests pass. Physical-device review is
  deferred to the combined 1.1.0 release candidate.
- This task does not deploy a desktop generation or upload a TestFlight build.

## Verification

- The complete 315-test desktop suite passed, including focused state tests for
  repeated working state, final response, terminal state, and disconnect.
- All 68 Node tests passed, including the headless desktop HUD test that proves
  one transient row, no duplicates, terminal removal, and final-response
  removal.
- All 93 Python helper tests and desktop Clippy with warnings denied passed.
- iOS workflow run
  [33318635065](https://github.com/alexjercan/scufris2/actions/runs/33318635065)
  generated with Xcode 26.3, compiled the app, and passed simulator tests.
- The mdBook package and Documentation workflow run
  [33318635107](https://github.com/alexjercan/scufris2/actions/runs/33318635107)
  passed.
- No Home Manager activation or TestFlight upload was performed.
