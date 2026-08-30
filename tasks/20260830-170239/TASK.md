# Show transient thinking feedback on every surface

- STATUS: OPEN
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
- Desktop UI tests, iOS simulator tests, and later physical-device review pass.
- This task does not deploy a desktop generation or upload a TestFlight build.
