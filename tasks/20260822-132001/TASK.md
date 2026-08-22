# Add Scufris voice HUD and desktop tray

- STATUS: OPEN
- PRIORITY: 100
- TAGS: voice, desktop, hud, tray

## Goal

Add a compact voice HUD and persistent desktop tray for the existing Scufris popup conversation. Keep the current page visible while recording, reviewing speech, and monitoring agent activity.

## Accepted interaction

- Super+D opens a bottom-center HUD, gives it keyboard focus, and starts recording immediately.
- The HUD shows an unmistakable recording indicator, a waveform or orb, and recording duration.
- Escape while recording stops and discards the recording.
- Enter while recording stops, transcribes, shows the sent text, and submits without another confirmation.
- Super+D again while recording stops, transcribes, and opens an editable review state.
- Enter sends the reviewed transcript. Escape discards it.
- Cancellation and submission restore focus to the previous window.
- Super+S continues to open the full popup chat.
- The popup chat contains the exact HUD messages and assistant responses.
- The HUD shows listening, transcribing, working, speaking, attention, and error states.
- Future wake-word activation can invoke the same start action. Wake-word detection is not part of this feature.

## Desktop companion

The user-facing product remains Scufris. The desktop component is `scufris-desktop`.

`scufris-desktop` owns:

- Super+D activation.
- The bottom-center HUD.
- The tray icon and menu.
- Microphone recording and cancellation.
- Local Whisper transcription.
- Transcript review and editing.
- Backend health monitoring and bounded restart requests.
- Focus restoration.

The companion starts with the desktop session and remains available when the backend fails. It does not own the conversation or write Pi session files.

## Backend ownership

The existing popup Pi process owns:

- The authoritative conversation and session persistence.
- Agent execution, tools, delegation, and human review.
- Assistant lifecycle events and speech output.

The Kitty popup is the complete interface to this process. Hiding the popup does not stop the backend. No second process can run an agent against or write to the same session.

## Communication

- Use a narrow same-user local control channel between `scufris-desktop` and the popup backend.
- The companion submits only an accepted transcript as a normal user message.
- The backend streams bounded health and lifecycle state to the companion.
- If Whisper fails, submit nothing.
- If communication fails after transcription, retain the transcript in the HUD.
- Select and record the native desktop framework and exact local protocol before implementation.

## Tray behavior

- Left-click opens the full chat.
- Right-click opens the status menu.
- The menu can open chat, start voice input, show concise failure details, restart an unavailable backend, and quit the companion.
- Tray states distinguish idle, microphone active, working, attention required, and backend failure.
- Recording always has a visible privacy indicator.
- A backend crash leaves the companion and tray available with an error state and bounded restart action.
- A companion crash does not stop the backend conversation.

## Verification

- Verify both fast-send and transcript-review flows under i3.
- Verify cancellation, focus restoration, local transcription failure, backend communication failure, and session resume.
- Verify that HUD messages and assistant responses appear in the full popup conversation.
- Verify tray state transitions for recording, work, attention, idle, disconnect, and backend failure.
- Verify that the tray survives a backend crash and restarts only the owned backend service.
- Verify that the page remains usable and visually unobstructed outside the compact bottom-center HUD.
- Run repository checks and Nix checks.
- Complete live HUD, tray, microphone, TTS, crash, restart, and popup playtesting.

## Completion criteria

- Super+D provides reliable voice interaction without opening or covering the full chat.
- The HUD supports both immediate send and editable transcript review.
- One authoritative Pi session serves both HUD and popup interfaces.
- The tray communicates useful health and attention state, including backend failure.
- The desktop companion and backend fail independently without losing an accepted unsent transcript or corrupting the conversation.
