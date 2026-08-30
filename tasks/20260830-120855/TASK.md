# Bootstrap the Scufris iOS app

- STATUS: OPEN
- PRIORITY: 100
- TAGS: ios

## Goal

Create the first native iOS application without requiring a local Mac. Build it
on a GitHub-hosted macOS runner before introducing Apple signing credentials.

## Scope

- Add a declarative XcodeGen project under `surfaces/ios/`.
- Add a minimal SwiftUI connection shell for iOS 17 and later.
- Compile an unsigned simulator application in a dedicated GitHub Actions
  workflow.
- Document local and CI generation without committing generated Xcode files.
- Do not add signing, TestFlight upload, microphone access, credential storage,
  or network transport in this task.

## Decisions

- Use the bundle identifier `com.alexjercan.scufris` unless Apple reports that it
  is unavailable.
- Use Swift 6, SwiftUI, and Apple frameworks only.
- Generate the Xcode project in CI so the project remains maintainable without
  local Xcode access.
- Keep signing material out of the repository and GitHub until the unsigned
  build passes.

## Verification

Pending the first GitHub-hosted macOS build.
