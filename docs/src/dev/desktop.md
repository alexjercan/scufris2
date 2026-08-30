# Desktop companion

[Previous: Background service](service.md)

```text
microphone -> STT -> pending textbox -> surface.message
surface.message -> HUD
assistant message -> text + optional speech + optional widgets
```

`scufris-desktop` is a registered protocol v5 surface. It owns local windows,
keyboard controls, recording, transcription, speech playback, and widget
presentation. It does not own Pi or the canonical conversation.

## Surface identity and connection

The desktop persists one opaque ID in its state directory as a private file. It
connects to `surface.sock` and sends `surface.hello` with that ID, a diagnostic
host name, and all installed widget definitions.

A reconnect clears the local 200-entry conversation and enters replay mode.
The desktop stores replayed messages and state. It becomes live only after a
matching `surface.ready`. Handshake EOF, closure, a wrong version, or a wrong
ready identity produces a local message that asks the user to update the host
and surface together.

The link reconnects with bounded exponential backoff. `SCUFRIS_DESKTOP_SOCKET`
can override the surface socket. `SCUFRIS_RUNTIME_DIR` moves all local Scufris
sockets for a coordinated staging run.

## Conversation window

The HUD stores at most 200 canonical `surface.message` entries. It displays the
LLM-style role, plain text, and optional Markdown details. It retains widget
call metadata as part of each message but does not execute calls from replay.

Typing sends `surface.message { id, text }`. The field clears only after the
local host accepts the IPC request. `surface.message_ack` settles that exact
submission. A rejection or disconnect leaves an explicit local failure.

The HUD is controlled locally from the tray, the pill, or `scufris-ctl hud`.
The agent has no conversation-window protocol.

## Live local presentation

Every ready surface stores every live canonical message. On any new live
message the desktop stops prior local speech. It performs assistant
presentation only when the message's `surface` equals its persisted ID:

- speak the mandatory plain `text` when local speech is enabled and unmuted;
- never speak `details`;
- animate local response presentation; and
- execute attached widget calls as best-effort presentation.

A widget call opens the named installed widget as an exhibit and passes its
arguments as initial data. Runtime outcomes stay local. No widget result,
acknowledgement, asynchronous update, or close message crosses protocol v5.

## Pill and voice interaction

The pill state machine is local. It owns these phases:

```text
resting -> listening -> transcribing -> editing -> sent -> resting
```

Recording and transcription failures enter a visible local failure state. A
transcript is saved before submission. If delivery becomes uncertain, it is
never resent without an explicit user confirmation. Submission IDs distinguish
late acknowledgements from the current transcript.

The popup key defaults to `Super+D`. A tap shows or hides the local workspace.
A hold records until release. The derived background and abort keys exist only
while the pill is on screen. The desktop command socket remains
`desktop.sock`; `scufris-ctl open`, `hud`, `show`, and `hide` use that local
surface protocol rather than the service control channel.

## Server and local state

The service sends one severity-first server state:

```text
failed > blocked > working > starting > idle
```

The desktop layers listening, transcribing, and speaking over it without
reporting those local states back to the service.

The tray remains available when the pill is hidden. It controls the HUD, voice,
local speech mute, sound cues, local widgets, the configured backend restart,
and process exit.

## Window and process safety

Window work is ordered outside state locks. The newest decision owns final
placement. Recording starts only after the privacy indicator is visibly up.
Focus restoration targets only a previously observed non-Scufris window.

Speech and widget backend processes are owned by recorded child handles or
process groups. Shutdown stops only those owned processes. No helper uses broad
process matching.

## Environment

| Variable                          | Meaning                                                         |
| --------------------------------- | --------------------------------------------------------------- |
| `SCUFRIS_DESKTOP_SOCKET`          | Surface socket override                                         |
| `SCUFRIS_DESKTOP_COMMAND_SOCKET`  | Local desktop command socket override                           |
| `SCUFRIS_RUNTIME_DIR`             | Coordinated local socket directory                              |
| `SCUFRIS_DESKTOP_STATE_FILE`      | Pending transcript path; its directory also stores `surface-id` |
| `SCUFRIS_STT_ENDPOINT`            | ai-tools-api transcription route                                |
| `SCUFRIS_TTS_ENDPOINT`            | ai-tools-api speech route used by the playback helper           |
| `SCUFRIS_DESKTOP_HOTKEY`          | Activation accelerator                                          |
| `SCUFRIS_DESKTOP_CANCEL_KEY`      | Local cancel accelerator or `none`                              |
| `SCUFRIS_DESKTOP_STOP_KEY`        | Local stop accelerator or `none`                                |
| `SCUFRIS_DESKTOP_SPEAK_COMMAND`   | Local stdin-driven HTTP synthesis and playback helper           |
| `SCUFRIS_DESKTOP_RESTART_COMMAND` | Owned service restart helper                                    |
| `SCUFRIS_WIDGET_PATH`             | Additional compiled widget roots                                |

## Limits

- canonical conversation: 200 messages;
- protocol line: 64 KiB;
- user and response text: 8 KiB UTF-8;
- response details: 32 KiB UTF-8;
- widget definitions or calls: 32 per message;
- local speech paragraph: 1000 UTF-8 bytes; and
- reconnect backoff: 250 ms to 5 seconds.

---

Next: [Pi extensions](extensions.md)
