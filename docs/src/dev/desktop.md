# Desktop companion

[Previous: Background service](service.md)

```text
microphone -> STT -> pending textbox -> surface.message
surface.message -> HUD
assistant message -> text + optional speech + optional widgets
```

`scufris-desktop` is a registered protocol v10 surface. It owns local windows,
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
LLM-style role, literal plain `text`, and optional Markdown `details`. Markdown
punctuation in `text` stays visible; only bare HTTP and HTTPS URLs become links.
Details render paragraphs, emphasis, strong text, inline and fenced code,
headings, ordered and unordered lists, block quotes, thematic rules, Markdown
links, and bare URL autolinks. It retains widget call metadata as part of each
message but does not execute calls from replay.

Both fields are untrusted. The renderer creates semantic elements and text nodes
instead of injecting HTML. Raw HTML stays inert text. Only credential-free HTTP
and HTTPS URLs with a host can be opened, and the Rust command repeats that
validation before it passes a URL to packaged `xdg-open`. Links use the existing
Gruber Niagara color, underline, and yellow focus state. Code, hierarchy, quotes,
and rules use the existing Gruber tokens.

Typing sends `surface.message { id, text, attachments }`. The `+` control opens
a native file picker. The desktop gives the selected local path only to the
private `content.sock` import route, stores the returned canonical descriptor in
composer state, and submits only its opaque ID. A message can contain at most
eight different files, each at most 16 MiB.

Canonical attachment descriptors render below their message with name, media
type, and size. Bounded raster images and recognized video thumbnails render
inline through a private custom webview scheme backed by `content.sock`.
Video thumbnails are bounded PNG frames extracted by packaged `ffmpeg`. Tapping
an image or video thumbnail opens a private mode-0600 runtime copy through
packaged `xdg-open`. SVG and general files remain metadata cards. The only
labeled file action is Save, which uses a native
destination picker and an atomic mode-0600 write. Attachment bytes or host paths
never enter logs or protocol messages.

Receipts are drawn at the foot of the message that reports them, one strip per
job and each strip led by that job's short ID. A badge is quartz when the fact
was measured, red when it was measured and false, yellow when only the worker
claimed it, and muted when nobody could measure it. A muted badge is never
drawn as a no.

An offer in the same strip is a button. Pressing it sends `offer.take` with the
offer identifier and nothing else, and no user line appears: the words behind
the offer stay with the agent, so the answer that follows is the only report of
it. A taken offer stays where it is and goes quiet.

The job list is the last item in the conversation flow and holds at most eight
rows. A row draws the job ID, the project, one of `work`, `block`, `done`, and
`fail`, the age, and the summary. A row outlives its execution. `clear` files
the finished generation and touches nothing else: the filing survives a
restart, and the logical job record stays where `scufris-jobs` keeps it until
`stop` or `land`. Steering that job starts a new generation and puts its active
row back in the list; its next terminal row needs a separate `clear`. `x` stops
a live job and keeps its unmerged branch; it arms on the first press, says
`sure?`, and forgets after
three seconds, because stopping the wrong job costs an hour of an agent's work.
Two or more finished rows also offer one control that files all of them.

The `BRIEF` section is one compact drawer in the conversation flow. Its
collapsed state shows every active briefing plus at most the newest attention
row. A circuit-stopped delivery is attention, not active work. The header
reports active and attention totals plus the number of relevant rows actually
hidden. Expanding it shows all relevant rows in stable order without moving a
reader who has scrolled up or detaching the focused disclosure control on an
unrelated redraw. A delivered success disappears. A delivered collection
failure or measured partial has one accessible `dismiss` control; a stopped
delivery instead shows its message-or-restart recovery and cannot be dismissed.
The desktop sends only an opaque generation ID and waits for the service's
whole-list update before a dismissed row disappears.

Each message occupies two columns: the speaker marker in a fixed gutter and
everything the message is made of - words, attachment cards, details - in one
body column. A message longer than the window wraps inside that column and never
onto the marker's line. A run of messages from one speaker is separated by a
hairline drawn across the body column; a change of speaker is separated by
space alone.

The window follows the newest line only while the reader is at or near the
bottom of it, within 24 logical pixels. A reader who has scrolled up keeps their
position when lines arrive, and gets a compact down-arrow control in the corner
of the conversation. It is accented while lines they have not reached are
waiting, states the count in its accessible name, mirrors that in an offscreen
status region, scrolls to the newest line when used, and hands the keyboard back
to the composer. Its glyph is generated by
`surfaces/desktop/tools/generate_glyphs.py`.

The composer clears only after the local host accepts the IPC request.
`surface.message_ack` settles that exact submission. A rejection or disconnect
leaves an explicit local failure.

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
acknowledgement, asynchronous update, or close message crosses protocol v10.
Briefing dismissal is separate surface presentation state and does cross as
`briefing.dismiss`; it is not a widget outcome or delivery acknowledgment.

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
- attachments: 8 unique references per message and 16 MiB per object;
- receipts: 4 job groups per message, 6 badges and 2 offers per group;
- job rows: 8;
- briefing audit rows: 64;
- local speech paragraph: 1000 UTF-8 bytes; and
- reconnect backoff: 250 ms to 5 seconds.

---

Next: [Pi extensions](extensions.md)
