# Add scufris-desktop: voice HUD pill, tray, and conversation shell

- STATUS: OPEN
- PRIORITY: 100
- TAGS: voice, desktop, hud, tray

## Goal

Build scufris-desktop, Scufris's own desktop companion: a Tauri app in
this repository. It delivers voice interaction as an always-available
overlay while the desktop stays usable, and grows into the primary
visual shell for Scufris. The user-facing product remains Scufris.

Increments:

- V1 (implementation scope of this task): the voice pill - Super+D
  always-on-top bottom-center overlay, recording, local transcription,
  review, submission to the backend, tray with health states.
- V2: full-screen conversation mode - chat history and an editable
  input over the same authoritative Pi session. Requires session
  mirroring in the control protocol. Proceeds in parallel with other
  tracks after v1 works.
- V3: embed dashboardd-runtime as a third host. scufris-desktop
  becomes the primary widget host and the HUD canvas (briefings,
  references); dashboardd-desktop demotes to a manually launched tool.
  dashboardd itself stays Scufris-agnostic.

Decided at 2026-08-24 pairing (task 20260823-233541); see its NOTES.md
for the surrounding vision.

## Architecture decisions

- Framework: Tauri, deliberately the same stack as dashboardd-desktop
  so v3 runtime embedding is mechanical, not architectural.
- Home: a `desktop/` cargo workspace in scufris2, shipped as a
  separate flake package output. scufris2 is a Pi package (npm is LSP
  tooling only), so the workspace fights no existing build. Companion
  and extension release in lockstep under one tag; consumers who skip
  the desktop package never build Tauri.
- STT: the companion calls a configurable whisper-server-compatible
  HTTP endpoint. Scufris ships an optional bundled whisper-server
  service following the Piper precedent: the default package is
  STT-free, a voice-capable output or module option runs the bundled
  server when no endpoint is configured, and `stt.endpoint` overrides
  it (nix.dotfiles supplies the existing 127.0.0.1:10301). Works out
  of the box on any Nix system.
- No window manager gets built. i3 remains the app-level answer for
  ordinary windows.

## Accepted interaction (v1)

- Super+D opens the bottom-center pill, gives it keyboard focus, and
  starts recording immediately.
- The pill shows an unmistakable recording indicator, a waveform or
  orb, and recording duration.
- Escape while recording stops and discards the recording.
- Enter while recording stops, transcribes, shows the sent text, and
  submits without another confirmation.
- Super+D again while recording stops, transcribes, and opens an
  editable review state.
- Enter sends the reviewed transcript. Escape discards it.
- Cancellation and submission restore focus to the previous window.
- Super+S continues to open the full popup chat.
- The popup chat contains the exact pill messages and assistant
  responses.
- The pill shows listening, transcribing, working, speaking,
  attention, and error states.
- Future wake-word activation invokes the same start action. Wake-word
  detection is not part of this task.

## Ownership

scufris-desktop owns:

- Super+D activation, the pill, and the tray.
- Microphone recording and cancellation.
- STT calls to the configured endpoint.
- Transcript review and editing.
- Backend health monitoring and bounded restart requests.
- Focus restoration.

It starts with the desktop session and remains available when the
backend fails. It does not own the conversation and never writes Pi
session files.

The popup Pi process (the Scufris daemon) owns:

- The authoritative conversation and session persistence.
- Agent execution, tools, delegation, and human review.
- Assistant lifecycle events and speech output.

The Kitty popup remains the complete interface to this process. Hiding
the popup does not stop the backend. No second process runs an agent
against or writes to the same session.

## Control protocol v1

Modeled on dashboardd-desktop-control: a same-user Unix socket at
`$XDG_RUNTIME_DIR/scufris/daemon.sock`, LF-terminated JSON lines,
64 KiB message cap, explicit version field. The popup Pi process
serves it; only the popup role opens the socket.

- companion -> daemon: `{"v":1,"type":"hello"}`,
  `{"v":1,"type":"submit","id":"...","text":"..."}` (an accepted
  transcript, submitted as a normal user message),
  `{"v":1,"type":"ping"}`.
- daemon -> companion: `{"v":1,"type":"welcome","session":"..."}`,
  `{"v":1,"type":"ack","id":"..."}`,
  `{"v":1,"type":"state","state":"idle|working|speaking|attention|error","detail":"..."}`,
  `{"v":1,"type":"pong"}`.
- Listening and transcribing are companion-local states; the daemon
  never sees audio. Unknown message types are rejected, not ignored.
- If Whisper fails, nothing is submitted. If the daemon is unreachable
  after transcription, the transcript is retained in the pill.
- V2 extends this protocol with session mirroring; v1 messages must
  remain valid unchanged.

## Tray behavior

- Left-click opens the full chat.
- Right-click opens the status menu.
- The menu can open chat, start voice input, show concise failure
  details, restart an unavailable backend, and quit the companion.
- Tray states distinguish idle, microphone active, working, attention
  required, and backend failure.
- Recording always has a visible privacy indicator.
- A backend crash leaves the companion and tray available with an
  error state and a bounded restart action.
- A companion crash does not stop the backend conversation.

## Verification (v1)

- Verify both fast-send and transcript-review flows under i3.
- Verify cancellation, focus restoration, local transcription failure,
  backend communication failure, and session resume.
- Verify that pill messages and assistant responses appear in the full
  popup conversation.
- Verify tray state transitions for recording, work, attention, idle,
  disconnect, and backend failure.
- Verify that the tray survives a backend crash and restarts only the
  owned backend service.
- Verify protocol version rejection and unknown-message rejection.
- Verify the STT endpoint override and the bundled whisper-server
  path.
- Verify the desktop package is absent from the default package
  closure.
- Verify that the desktop remains usable and visually unobstructed
  outside the compact pill.
- Run repository checks and Nix checks.
- Complete live pill, tray, microphone, TTS, crash, restart, and popup
  playtesting.

## Completion criteria (v1)

- Super+D provides reliable voice interaction without opening or
  covering the full chat.
- The pill supports both immediate send and editable transcript
  review.
- One authoritative Pi session serves both pill and popup interfaces.
- The tray communicates useful health and attention state, including
  backend failure.
- The companion and backend fail independently without losing an
  accepted unsent transcript or corrupting the conversation.
- scufris-desktop builds as its own flake package from the `desktop/`
  workspace and works with either a configured STT endpoint or the
  bundled whisper-server.
