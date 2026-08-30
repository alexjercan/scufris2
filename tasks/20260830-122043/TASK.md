# Automate signed iOS TestFlight builds

- STATUS: OPEN
- PRIORITY: 100
- TAGS: ios, release

## Goal

Build, sign, and upload Scufris to internal TestFlight entirely through a
protected GitHub Actions environment.

## Scope

- Keep the Apple Distribution private key outside the repository.
- Add the distribution certificate and provisioning profile as protected
  GitHub environment secrets.
- Add the required application icon and archive metadata.
- Add a manually dispatched TestFlight workflow with explicit environment
  approval.
- Upload through the existing App Store Connect API key.
- Prove an internal TestFlight build before adding network or microphone
  behavior.

## Decisions

- Use manual distribution signing so ephemeral runners do not generate and
  exhaust Apple certificate slots.
- Keep unsigned simulator checks independent from signing credentials.
- Do not run TestFlight uploads on ordinary pushes.

## Verification

Pending certificate issuance, profile creation, signed archive, and upload.
