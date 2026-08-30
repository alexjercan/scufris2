# Automate signed iOS TestFlight builds

- STATUS: CLOSED
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

- Apple issued an Apple Distribution certificate and App Store Connect profile
  that match the local private key, team, and `com.alexjercan.scufris`. Both
  expire on 2027-08-30.
- Protected environment secrets contain the API key, team, profile, and
  macOS-compatible password-protected PKCS#12 identity.
- Run
  [33304253290](https://github.com/alexjercan/scufris2/actions/runs/33304253290)
  found the OpenSSL 3 PKCS#12 compatibility issue before archive creation. The
  identity was re-exported in the compatible legacy encoding.
- Run
  [33304286950](https://github.com/alexjercan/scufris2/actions/runs/33304286950)
  proved signed archive and export, then confirmed that App Store Connect now
  requires the iOS 26 SDK.
- Run
  [33304418421](https://github.com/alexjercan/scufris2/actions/runs/33304418421)
  used Xcode 26.3 and completed the signed archive, export, and App Store
  Connect upload with no errors.
- Unsigned simulator run
  [33304412669](https://github.com/alexjercan/scufris2/actions/runs/33304412669)
  also passed under Xcode 26.3.
