# Scufris for iOS

This directory contains the native SwiftUI companion. XcodeGen creates the
Xcode project from `project.yml`; the generated project and Info.plist are not
committed.

The application connects to the authenticated protocol-v4 surface gateway over
`wss://`, stores its URL, bearer token, and stable surface identity in the iOS
Keychain, replays the canonical conversation, and submits text messages. Its
text-only interface follows the terminal-style interaction study in
`tasks/20260828-232631/ios-app-design.html`. Voice and widgets remain outside
this milestone.

## Build

The `iOS` GitHub Actions workflow builds and tests the application on an iOS
simulator without signing. On macOS with Xcode and XcodeGen installed, run:

```bash
xcodegen generate --spec surfaces/ios/project.yml
xcodebuild \
  -project surfaces/ios/Scufris.xcodeproj \
  -scheme Scufris \
  -configuration Debug \
  -destination 'generic/platform=iOS Simulator' \
  CODE_SIGNING_ALLOWED=NO \
  clean build
```

Signing credentials are not required for this build and must not be committed.

## TestFlight

The manually dispatched `TestFlight` workflow uses Xcode 26.3 and the protected
`testflight` GitHub environment. `MARKETING_VERSION` is the semantic product
version shown first in TestFlight; `GITHUB_RUN_NUMBER` supplies Apple's required
unique, monotonic build number in parentheses. It imports the distribution
identity and App Store profile into an ephemeral keychain, archives the app, uploads it through
the App Store Connect API, and removes the imported material.

The environment supplies `APPLE_DISTRIBUTION_CERTIFICATE_P12`,
`APPLE_DISTRIBUTION_CERTIFICATE_PASSWORD`, `APPLE_PROVISIONING_PROFILE`,
`APPLE_TEAM_ID`, `ASC_ISSUER_ID`, `ASC_KEY_ID`, and `ASC_PRIVATE_KEY`. Never put
their values in the workflow or repository.
