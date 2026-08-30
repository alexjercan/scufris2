# Apply the iOS conversation design

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: ios, surface, design

## Goal

Turn the proven text-only iOS transport into the conversation surface designed
in `tasks/20260828-232631/ios-app-design.html`. Keep voice out of this milestone.
Use semantic marketing versions for TestFlight while retaining Apple's required
unique build number.

## Decisions

- Preserve the mock's dark terminal palette, compact route/state header,
  speaker-column transcript, and bottom text composer.
- Do not add microphone, transcription, playback, mute, stop, or orb controls.
- Keep setup reachable but secondary to the conversation.
- Set marketing version 1.1.0. TestFlight will still show Apple's mandatory
  build number in parentheses; CI supplies its monotonic run number.
- Keep protocol v4 transport and Keychain behavior unchanged except where UI
  state needs read-only presentation data.
- Use the existing protected CI and TestFlight workflows to verify and deliver
  the physical-device review build; never expose signing material.

## Acceptance

- The text conversation follows the supplied visual mock on iPhone.
- Setup, reconnect, canonical replay, bounded text submission, details, and
  connection errors remain usable.
- The project reports marketing version 1.1.0 and a monotonic CI build number.
- Swift tests cover any new presentation-state mapping.
- iOS simulator CI-equivalent generation, build, and tests pass.

## Verification

- iOS workflow run
  [33309518380](https://github.com/alexjercan/scufris2/actions/runs/33309518380)
  generated the Xcode project with Xcode 26.3, compiled the application, and
  passed all simulator tests.
- TestFlight workflow run
  [33309657849](https://github.com/alexjercan/scufris2/actions/runs/33309657849)
  created the signed archive, removed ephemeral signing material, and uploaded
  marketing version 1.1.0 successfully.
- Static checks confirm the application source contains no microphone, audio,
  speech, hold-to-talk, or orb implementation.
- `git diff --check` passed. A local Nixpkgs Swift formatter attempt failed while
  building the Linux Swift toolchain; the authoritative Xcode 26.3 build passed.
