# Scufris for iOS

This directory contains the native SwiftUI companion. XcodeGen creates the
Xcode project from `project.yml`; the generated project and Info.plist are not
committed.

The initial application is an unsigned connection shell. It does not yet save
credentials or connect to the Scufris backend.

## Build

The `iOS` GitHub Actions workflow builds the application for the iOS simulator
without signing. On macOS with Xcode and XcodeGen installed, run:

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
