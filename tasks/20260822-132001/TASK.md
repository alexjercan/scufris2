# Add scufris-desktop: voice HUD pill, tray, and conversation shell

- STATUS: IN_PROGRESS
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
  `{"v":1,"type":"uncertain","id":"...","detail":"..."}` (dispatched
  once already, outcome unknown),
  `{"v":1,"type":"refused","id":"...","detail":"..."}` (nothing left
  the daemon, so the words are still the companion's to edit and
  retry),
  `{"v":1,"type":"state","state":"idle|working|speaking|attention|error","detail":"..."}`,
  `{"v":1,"type":"pong"}`.
- Every answer about a submission names it, and the companion applies
  an answer only to the submission it names.
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

## Implementation (v1)

V1 is implemented. See `DECISIONS.md` for the choices taken while building it
and `VERIFICATION.md` for what each verification item above is covered by.

What landed:

- `desktop/`: a cargo workspace with `scufris-control` (the control protocol)
  and `scufris-desktop` (the Tauri pill and tray companion).
- `extensions/scufris/desktop/`: the daemon half of the protocol, served by the
  popup process only.
- `extensions/scufris/shared/assistant-state.ts`: the assistant state model,
  fed by signals from `voice/speech.ts` and `workflow/orchestration.ts`.
- `nix/desktop.nix`, `nix/whisper.nix`, and Home Manager options for the
  companion, the STT endpoint, and the bundled whisper-server.
- `docs/src/dev/desktop.md` plus user-guide and installation sections.

Twelve rounds of independent review found thirty-nine material failure-path
defects. All are fixed, each with a test that fails without its fix:

- An accepted transcript is on disk before anything is submitted, and storage
  failures stop the submission instead of being logged.
- Capture errors are scoped to the capture that raised them.
- One transcript keeps one identifier, drawn from the operating system's
  randomness, and a discard is final.
- A spoken prompt is an ordinary prompt. It is submitted with
  `sendUserMessage`, so it runs the same input handlers, the same pre-send
  compaction check, and the same per-turn Scufris system prompt a typed prompt
  runs, and it steers into a running turn the same way.
- An acknowledgment means the session holds this exact submission. Pi announces
  a prompt from inside the send that started it, so the asynchronous context -
  not the source class, which is `extension` for every extension alike -
  identifies this daemon's own prompt. A landing is credited only when that
  announcement was the only prompt in flight and landed as the words the
  companion submitted.
- A credited landing is committed against the entry it accepted, by that entry's
  own identifier, so reconciliation checks the prompt the commit names rather
  than whatever sits beside it. A branch taken at a prompt, and a crash between
  two appends, both leave records that adjacency would have believed.
- The entry a commit names is the entry that landing became, not whatever
  resembles it. Pi appends a prompt as a child of the leaf it holds while the
  extensions see the event, so the commit is written against the prompt that
  fills that place - and only while no other prompt has landed since, and only
  while that place is still on the branch. A session that is replaced cancels
  the commit outright.
- A submission the daemon refused before anything left it is answered as
  refused, naming that submission. Those words are still only the companion's,
  so they stay editable and an ordinary Enter retries them, rather than
  freezing behind the confirmation an uncertain outcome needs.
- A session says accepted, uncertain, or unsent. A submission that was
  dispatched and never committed may already have run, so nothing sends it
  again on its own - not the landing timeout, not a reset, not a daemon
  restart, not an ordinary Enter. The pill keeps and shows it, offers copy and
  discard, and sends it only after telling the person what that could repeat.
- A body the session does not hold under a known identifier is refused, whether
  or not that identifier is still in the daemon's bounded remembered set.
- Socket ownership is serialized by the kernel. The claim section is held under
  an advisory exclusive lock on the lock file beside the socket, so it covers
  every name that reaches that pathname and every process that shares the
  filesystem, whatever network namespace it is in. It cannot be stale, needs no
  lease, and leaves nothing to unlink. Every mutation of the socket pathname,
  including the one shutdown performs, is carried out by the process holding
  that lock, so a daemon whose lock is gone changes nothing.
- Handing a transcript to the daemon gives the desktop back at once. The pill
  reopens by itself if that submission is refused or its outcome turns out to
  be unknown.
- The change that ran last owns the window. A daemon answer can arrive while the
  handoff that asked for it is still running, so where the pill belongs is read
  from the phase each change leaves behind rather than from a list decided a
  moment earlier. An answer that needs the person cannot be closed by the
  handoff it overtook, and the pill cannot fall back to rendering a state that
  has been left behind.
- The transcript bound is 8 KiB of UTF-8 bytes on both sides, validated before
  persisting and before sending, so non-ASCII text that is accepted can also be
  recovered.

Nothing here is claimed beyond what a listed test exercises; the remaining
limits are stated in `VERIFICATION.md` and `docs/src/dev/desktop.md`.

Still open before this task closes: the live playtesting listed at the end of
`VERIFICATION.md`. It needs a real desktop session, microphone, speaker, and a
running backend, which the implementation workspace does not have.

## Market research (2026-08-25)

Market research for the pill redesign, diegetic feedback, focus-free keys,
wake word, and the post-v1 feature direction is in `RESEARCH.md`, with raw
sweep reports under `research/`.
