# Desktop companion

`scufris-desktop` is the Scufris voice pill and tray companion. It is a Tauri
application built from the `desktop/` cargo workspace and shipped as its own
flake package, so consumers who never enable it never build Tauri.

The stack matches `dashboardd-desktop` on purpose. A later increment embeds the
Dashboardd runtime in this same host, and a shared stack makes that a mechanical
change instead of an architectural one.

## Ownership

scufris-desktop owns:

- Activation, the pill window, and the tray.
- Microphone recording and cancellation.
- Local transcription against the configured endpoint.
- Transcript review and editing.
- Backend health presentation and bounded restart requests.
- Focus restoration.

The popup Pi process, the Scufris daemon, owns the authoritative conversation,
session persistence, agent execution, tools, delegation, and speech output. The
companion never writes a Pi session file. It submits accepted transcripts as
ordinary user messages and follows the state the daemon reports.

The Kitty popup remains the complete interface to the daemon. Hiding it does
not stop the backend, and a companion crash does not stop the conversation.

## Interaction

- The activation accelerator, `Super+D` by default, opens the bottom-center
  pill, gives it keyboard focus, and starts recording immediately.
- The pill shows a red privacy ring, a level-driven orb, and the recording
  duration.
- `Escape` while recording stops and discards the recording.
- `Enter` while recording stops, transcribes, shows the sent text, and submits
  without another confirmation.
- The accelerator again while recording stops, transcribes, and opens an
  editable review state. `Enter` sends the reviewed transcript; `Escape`
  discards it.
- Cancellation and submission restore focus to the previous window.

If the microphone cannot start, or an open capture stream fails, the pill drops
the recording indicator and states why. If transcription fails, nothing is
submitted and the pill states why. If the daemon refuses a submission before it
leaves the companion, the transcript is retained and `Enter` sends it again. If
a submission leaves the companion and is not acknowledged, the transcript is
retained as **uncertain** instead: `Ctrl+C` copies it, `Escape` discards it, and
`Enter` says what sending it again would risk before a second `Enter` does it.
Activation never discards a retained transcript.

Every one of these transitions lives in `desktop/scufris-desktop/src/state.rs`
as a pure state machine, so the whole interaction is tested without a display.
Everything the runtime reaches outside itself - the microphone, the endpoint,
the socket, the window, the disk, and where deferred work runs - is a port on
`App`, so the failure paths are tested without any of them.

## The change that ran last owns the window

Submission gives the desktop back at the handoff, which puts two changes in the
same moment: the accepting phase is still running its actions - the write to the
socket among them - when the daemon's answer arrives on the link's own thread.
Either can be the one that finishes last.

So the state machine emits no show or hide. Where the pill belongs is a property
of the phase, and the runtime reads it from the phase each change leaves behind,
stamped with that change's place in the order the companion lock made them. A
stamp older than one already recorded changes nothing, so an answer that needs
the person cannot be closed by the handoff it overtook, and an acknowledgment
that lands in the same gap still closes the pill it finished with.

Where the pill belongs and what it renders are one decision, not two. What the
pill renders is only an indicator while the pill is up, so two orders would let
the window be settled by one change while the pill showed another, with nothing
able to answer whether the state the person can read is the state the phase is
in. One thread applies the window, the pill, and the tray together.

The surface operations run outside every lock, because they wait on the event
loop that also runs the pill's own commands. A thread that finds another already
applying leaves its decision behind and returns; the applying thread carries on
until nothing newer is left. It leaves its follow-up work behind too, and the
applying thread runs that work once it has driven the surface to the newest
decision - so the key pressed mid-render still acts on a pill that is up, and
never on the assumption that it is.

## The window is recorded by what it did, not by what it was asked

Asking a window to appear is not the window appearing, and the difference is
what the recording privacy indicator rests on: the pill is the indicator, so a
microphone that opens behind a pill that never came up is a microphone the
person was never told about.

So every surface operation whose outcome a later decision depends on reports
what it achieved, and the runtime records only that. `show` asks the window
afterwards whether it is up, and whether it holds the keyboard, and treats a
window that cannot answer as one that has not: a window manager may accept
`set_focus` and hand the keyboard elsewhere, or later, or never. `hide` asks the
same question in reverse, because an always-on-top pill recorded as down is a
pill nothing will ever take down again.

A show has three answers, not two.

- **Ready** - up, on top, holding the keyboard.
- **Seen** - up and on top, keyboard elsewhere. The person can read it, so the
  indicator counts and the phase runs.
- **Doubtful** - up, but placement or always-on-top was refused, so it may be
  behind whatever the person is looking at. The phase has a window and runs, but
  nothing that rests on being seen may rest on this.

The microphone rests on being seen. `StartRecording` refuses unless the pill has
taken a presentation that says the microphone is open and the window is
somewhere the person can see it, and a capture already running is stopped the
moment a decision reaches the surface saying `recording` while nothing on screen
says so. The refusal is the same failure a dead microphone gives, so the
companion already knows what to do with it.

The indicator is the pill, not the tray. A tray icon can be folded into an
overflow menu the person never opens. The tray is still told everything, and
told separately from the pill: a pill that refuses to render is exactly when
the tray is the only thing left that can say anything, so one failing never
skips the other. A presentation a surface refused is tried again a bounded
number of times, and stops early if a newer presentation is already waiting.

A phase whose window did not come up at all runs none of its actions. There is
nothing for it to do: the microphone has no indicator, and a transcript nobody
can see is one nobody can correct. The interaction is abandoned instead - the
microphone and any transcription stop, an accepted transcript stays on disk for
the next start to recover, and the tray, which is what is left, says what
happened. The person's next activation clears it.

A window that came up wrong is a different thing, and the runtime repairs it
itself. A pill that would not go down is over the desktop with focus already
given back, so the keys the person presses reach their own window, not the pill;
a pill that would not take the keyboard cannot be typed into. Both are asked
again after a short delay, on the newest decision rather than the one that fell
short, a bounded number of times, one chain at a time. A window manager that has
refused three times is refusing, and the next decision asks again anyway.

## An accepted transcript is never lost

A transcript the user accepted cannot be recreated by speaking again, so it
does not live only in memory. From the moment it exists it is written to a
private file, and it is removed only on an explicit discard or a matching
acknowledgment - not on shutdown, and not on a failure.

The file is `$XDG_STATE_HOME/scufris-desktop/pending.json` by default,
overridable with `SCUFRIS_DESKTOP_STATE_FILE`. It is written to a temporary
neighbour and renamed into place, so a crash mid-write cannot leave a partial
record.

Every store operation reports failure, and nothing is submitted until the save
is known to have landed. A full disk or a read-only state directory therefore
stops the submission, keeps the text in the pill, and says why, instead of
letting the pill claim a durability it does not have.

A record that exists but cannot be read is reported as a failure, never as an
empty store: treating it as empty is exactly what would let the next save
destroy it silently.

On startup the companion loads that file and reopens the pill on the recovered
transcript, keeping its original identifier. Whether the previous process
delivered it is unknowable, so the recovered text is frozen - see below.

## One identifier per transcript

A transcript keeps one submission identifier from the moment it is transcribed
until it is acknowledged - across review edits, across retries, and across a
restart. Identifiers carry a per-process prefix drawn from the operating
system's randomness, because they outlive the process that made them: a process
identifier and a clock are reused, and a collision would have a genuinely new
request refused as a duplicate.

## A spoken prompt is an ordinary prompt

A transcript is submitted with `sendUserMessage`, so the turn it starts is the
turn a typed prompt starts: the same `input` handlers, the same pre-send
compaction check, and the same per-turn Scufris system prompt from
`before_agent_start` - the foreground identity, the live delegated-job context,
and the final-response policy. A custom message that triggers a turn runs none
of that (`agent-session.js:1068-1090`), which would let a spoken request reach
the model as base Pi.

Desktop input is declared as extension input. Pi reports it with
`source: "extension"`, and it passes through the whole `input` handler chain
like any other prompt: nothing about it is exempt from a filter or a transform
another extension installs. A handler that rewrites the words means the daemon
never sees the landing it is waiting for, so the transcript is retained rather
than acknowledged.

## An acknowledgment means the words are in the conversation

Pi's send APIs report nothing back: `sendUserMessage` and `sendMessage` both
return `void`, the session manager extensions see is read-only, and no event
identifies which prompt produced which entry. Starting a send therefore proves
nothing, and neither text nor position can tell one send from another when two
sources can produce the same words.

Pi announces every prompt to its `input` handlers before deciding whether to run
it or queue it, and reports the source _class_ - `extension` for every extension
alike, which identifies nobody. What does identify the send is the asynchronous
context: Pi calls `sendUserMessage` directly rather than scheduling it
(`agent-session.js:1854-1861`), so the announcement of a prompt arrives inside
the call that started it. A prompt announced inside this daemon's own send is
this daemon's own prompt, and no other extension's send can be.

That settles whose announcement it is. It does not settle which landing is that
announcement's. A handler after this one may rewrite a prompt or answer it
outright, and neither outcome is visible here, so an announcement that never
lands would otherwise be free to claim somebody else's later words.

So the daemon under-credits on purpose. A landing is credited only when:

| Condition                                          | Why                                                   |
| -------------------------------------------------- | ----------------------------------------------------- |
| The announcement arrived inside this daemon's send | The source class says `extension` for every extension |
| Its words are the words the companion submitted    | A handler ahead of this one may have rewritten them   |
| It was the only prompt Pi had in flight            | Nothing says which of two landings is which           |
| The landed words are those same words              | A handler after this one may have rewritten them      |

Anything else - a prompt typed or sent while this one waits, a second
extension's identical prompt, a rewritten prompt, a prompt a later handler
answered itself - leaves the submission uncredited. An uncredited submission is
never acknowledged. It is not called undelivered either: the words may be in the
conversation, so it becomes uncertain, and only the person decides what happens
to it. That is the safe direction: Pi's public API cannot say which landing is
this daemon's, so the daemon says it does not know rather than guessing either
way.

When a landing is credited, the daemon commits it against the exact entry that
landing became. Pi appends the prompt only after the extensions have seen it
(`agent-session.js:363-379`), so at that moment the prompt has no identifier -
but it does have a place, because Pi appends a message as a child of the current
leaf. The daemon reads that leaf while the prompt is landing and finds the
prompt afterwards as the first user message after it. Searching the branch for
the newest entry carrying the same words would name whatever resembles the
prompt rather than the prompt itself.

Two rules keep that place unambiguous:

| Rule                                                       | What it stops                                       |
| ---------------------------------------------------------- | --------------------------------------------------- |
| A landing supersedes the one before it                     | A later prompt filling the place a lost append left |
| The anchor must still be on the branch, holding that place | A branch taken between the event and the commit     |

Every user prompt reaches this daemon as `message_end` before Pi appends it, so
the only entry that can appear in the anchor's place without superseding the
landing is the landing's own prompt. A session that is replaced or shut down
cancels the commit outright.

The commit is `scufris-desktop-accepted-v1`, carrying the submission identifier,
the digest of the words, and the entry the words landed as. A custom entry is
persisted without entering the model's context.

"Did this land" is then answered by reading the commit the way it was written:

| Branch holds                                            | Meaning                            |
| ------------------------------------------------------- | ---------------------------------- |
| A commit whose named entry is a prompt with these words | Accepted; suppress the retry       |
| A dispatch and no such commit                           | Uncertain; only the person decides |
| Neither                                                 | Never sent; deliver it             |

Nothing weaker will do. A session is a tree: a branch can be taken at any entry,
which leaves a record with a stranger behind it. A process can die between two
appends, which leaves a record with nothing behind it at all. Both shapes look
identical to a rule that reads whatever sits beside a record, and both would
otherwise acknowledge a prompt somebody else sent.

A record an earlier build wrote before its prompt landed cannot prove anything,
so it is read as a dispatch: the words may be in the conversation, and that is
all it says.

One identifier can carry more than one body, because a branch can hold a reused
identifier over different words. Each of those bodies is in the conversation, so
each is acknowledgeable and nothing else is: a retry under a known identifier
with words the session does not hold is refused, whether or not that identifier
is still in the daemon's bounded set of remembered submissions.

A transcript is sent at most once per process, and at most once per person's
decision after that. A retry that arrives while the first send is still queued
waits rather than sending again.

Pi emits `message_end` to extensions _before_ it appends the entry, so the
observer runs on the next tick. A pending delivery also re-reads the session on
a short interval: a provider that stalls after the message lands emits nothing
further, and the acknowledgment must not wait on an event that never comes.
Replacing or shutting down the session settles every pending delivery instead of
leaving its timers behind.

The popup renders a commit as a short note that the prompt it names was spoken
through the pill, and a dispatch that no commit answered as a warning that a
request left the pill and its outcome is unknown. The transcript itself is a
user message, so it reads exactly like anything else the person said.

## Accepted, uncertain, and unsent are three different answers

A submission is in exactly one of three states, and the daemon reads which from
the session rather than from memory:

| State     | The session holds                         | What happens                                  |
| --------- | ----------------------------------------- | --------------------------------------------- |
| accepted  | A commit naming a prompt with these words | Acknowledged, nothing is sent                 |
| uncertain | A dispatch with no such commit            | Answered `uncertain`; only the person decides |
| unsent    | Neither                                   | Dispatched and sent                           |

"Uncertain" is the one that matters. A request that reached the conversation can
have written files, sent messages, or started work, so treating "I did not see it
land" as "it did not land" would run it a second time. Nothing automatic ever
resolves that: not the daemon's landing timeout, not a reset, not a daemon
restart, not a reconnection, and not an ordinary `Enter` in the pill.

What the person is offered instead is the truth and three choices. The pill
shows the transcript and says the outcome is unknown; `Ctrl+C` copies the words
somewhere safe, `Escape` discards them for good, and `Enter` first says what
sending again could repeat and only then, on a second `Enter`, sends it. That
second press is the only thing in the whole system that sets `force` on the
wire, and the only thing that will send words that may already be in the
conversation.

A transcript recovered after a companion restart is uncertain for the same
reason: the previous process may have submitted it.

## An ambiguous retry cannot confirm text it did not send

A retained transcript is editable only when the companion knows the daemon never
saw it - a refused write, or a save that failed before the submission. An
uncertain transcript is frozen: an edit would let the pill close as though the
new words landed while only the original body is in the conversation.

The daemon enforces the same rule from its side: a reused identifier carrying
different text is refused with an error rather than acknowledged, whether that
identifier is already accepted, already dispatched, or still in flight.

A discard is likewise final. Escape removes the durable record; if removal is
impossible the record is replaced by a tombstone, which still stops the text
being restored, and only when neither can happen does the pill reopen to say the
words are still on disk.

## Control protocol v1

The daemon serves one same-user Unix socket at
`$XDG_RUNTIME_DIR/scufris/daemon.sock`, created inside a `0700` directory and
chmodded to `0600`. Ownership is taken atomically: the listener is bound to a
private path and hard-linked onto the public one, and `link` fails when the name
already exists.

That covers an absent path but not a stale one, where every starter sees the
same dead inode and each could remove it after another has replaced it. Probe,
removal, and claim therefore all happen while holding one ownership lock, so
exactly one starter runs that sequence at a time.

The lock is the kernel's own: an advisory exclusive lock on the lock file beside
the socket, taken by `tools/desktop/scufris-socket-lock` because Node cannot
take one itself. Every lock Node could build from the calls it does have is a
pathname check followed by a separate mutation, which is not an
ownership-conditional mutation at all.

The lock lives on that file's inode, and the inode is what makes it the right
lock:

- Every name that reaches the file is the same lock. A `.` detour, a `..` climb,
  a symlinked parent, or a different mount of the same filesystem all open one
  inode. The socket path itself is canonicalised the same way - the parent
  directory resolved, the basename kept - so two daemons given different names
  for one socket agree on what they are guarding.
- It is not scoped to a network namespace. An abstract-namespace socket is, so
  two processes that share this runtime directory through different network
  namespaces can both bind one abstract name and both believe they hold the
  lock. They cannot both hold this one.
- It is released when its holder ends, however it ends. There is no lease to
  wait out, no record to judge stale, and nothing to unlink - so no former
  holder can remove a name a successor has taken. The lock file is therefore
  never deleted; an empty file that nobody removes is exactly the point.

The helper is the lock. It holds it for as long as the daemon keeps its standard
input open, and every mutation of the socket pathname first asks whether the
helper is still there. The daemon asks in JSON, one object per line, one field
per value: a socket path is whatever the person configured and
`XDG_RUNTIME_DIR` is whatever the session set, so both may hold spaces, and a
line of space-separated fields loses such a path or claims the wrong one. Every
one of those mutations, including the removal a departing daemon performs at
shutdown, happens inside the lock. Shutdown waits
only briefly for it: a pathname left behind is harmless, because its listener is
already closed and the next starter probes it, finds it dead, and removes it
under the same lock.

Shutdown removes the socket only if it is still the one this process created,
and any failure after the claim gives the pathname back. Messages are LF-terminated JSON lines bounded at 64 KiB with
an explicit `v` field.

Companion to daemon:

```json
{"v":1,"type":"hello"}
{"v":1,"type":"submit","id":"pill-1","text":"open the tasks widget"}
{"v":1,"type":"submit","id":"pill-1","text":"open the tasks widget","force":true}
{"v":1,"type":"ping"}
```

`force` is the person's own decision to send words that may already be in the
conversation. It is absent from every ordinary submission, and no timeout,
reconnection, or restart ever sets it.

Daemon to companion:

```json
{"v":1,"type":"welcome","session":"2026-08-24"}
{"v":1,"type":"ack","id":"pill-1"}
{"v":1,"type":"uncertain","id":"pill-1","detail":"the outcome is unknown"}
{"v":1,"type":"refused","id":"pill-1","detail":"the Scufris session is not ready"}
{"v":1,"type":"state","state":"idle","detail":""}
{"v":1,"type":"pong"}
```

Every answer about a submission names the submission it answers, and the
companion applies it only to that one: it holds one transcript at a time and may
have started another by the time a slow answer arrives.

`refused` and `uncertain` are two different answers and the difference decides
what the person is offered. `refused` means nothing left the daemon - a preflight
that failed, a dispatch that could not be recorded, an identifier already
carrying other words - so those words are still only the companion's, stay
editable, and an ordinary `Enter` retries them. `uncertain` means they may
already be in the conversation, so nothing sends them again on its own. Both
answer the companion that asked rather than being broadcast: only that companion
holds the transcript, and only the person using it can decide what happens next.
A failure the daemon cannot classify is answered as `uncertain`, which is the
safe reading, and is also reported as a daemon error state because it is one.

Both peers reject unknown message types and any version other than `1`; they do
not ignore them. Submission identifiers and transcripts are bounded and
validated before a submission reaches the conversation. The transcript bound is
8 KiB measured in **UTF-8 bytes** on both sides: measuring UTF-16 code units on
one side would let non-ASCII text be accepted that the other cannot store or
read back. A daemon that finds the
socket already owned by a live peer refuses to bind rather than stealing it; a
stale socket file is replaced.

Listening and transcribing are companion-local. The daemon never sees audio.

Version 2 extends this protocol with session mirroring for the full-screen
conversation mode. The version 1 messages above must stay valid unchanged.

The protocol is implemented twice, once per side, and each side owns its tests:

- `desktop/scufris-control/src/lib.rs` for the companion.
- `extensions/scufris/desktop/protocol.ts` and `server.ts` for the daemon.

## Assistant state

`extensions/scufris/desktop/index.ts` runs only when `SCUFRIS_DAEMON=1`, which
the popup launcher sets. It resolves one state from independent signals in
`extensions/scufris/shared/assistant-state.ts`:

| Signal                             | State       |
| ---------------------------------- | ----------- |
| An agent run is in progress        | `working`   |
| Speech playback is running         | `speaking`  |
| A delegated job reported `blocked` | `attention` |
| A delegated job reported `failed`  | `error`     |
| A submission was not delivered     | `error`     |
| Nothing else                       | `idle`      |

An active run wins, because that is what the user just asked for, and starting a
run clears an unattended job signal. The speech module emits its own signal, and
the orchestration module maps worker events, so neither has to know about the
socket.

The companion adds two of its own tray states that the daemon cannot see:
`listening` and `transcribing`, plus `disconnected` when the socket is closed.
Each state has a distinct tray colour drawn at runtime from the state name, so
there is no per-state image to keep in sync.

## Tray

Left-click opens the full chat through the configured hook. Right-click opens
the status menu: open chat, start voice input, the current status line, restart
the backend, and quit. The chat and restart items are disabled when the
deployment configures no hook.

The restart hook is generated by the Home Manager module and restarts only the
Scufris backend service that module owns. It is bounded to three restarts in ten
minutes. The companion never builds a command line and never uses a shell: both
hooks are absolute executables run with no arguments.

## Transcription

The companion posts one 16 kHz mono PCM WAV as `multipart/form-data` to a
whisper-server-compatible endpoint and reads `{"text": "..."}` back. Capture,
downmix, resampling, and WAV encoding are pure functions with their own tests.

The endpoint is chosen by the deployment, following the Piper precedent:

- `programs.scufris.desktop.stt.endpoint` names an existing server.
- Otherwise the module runs a bundled loopback `whisper-server` on
  `127.0.0.1:10302` with a pinned model, so voice works out of the box.

Setting both is an error.

## Packaging

`nix/desktop.nix` builds the workspace with `rustPlatform.buildRustPackage`,
runs the Rust tests in its check phase, and wraps the binary with the WebKitGTK
and tray libraries it dlopens. The result also ships a desktop entry and icon.

`nix/checks.nix` asserts that the companion, WebKitGTK, and their closure stay
out of the default and voice launcher closures, that `--print-config` resolves
defaults and overrides exactly, that relative hooks and non-HTTP endpoints are
rejected, and that the module wires the services and the restart hook it claims.

`scufris-desktop --print-config` prints the resolved configuration and exits
without starting a window, which is what makes that check cheap.

## Environment

| Variable                          | Meaning                                                |
| --------------------------------- | ------------------------------------------------------ |
| `SCUFRIS_DAEMON`                  | `1` in the popup process, which then serves the socket |
| `SCUFRIS_DESKTOP_SOCKET`          | Control socket path override                           |
| `SCUFRIS_DESKTOP_STATE_FILE`      | Durable accepted-transcript file                       |
| `SCUFRIS_STT_ENDPOINT`            | Transcription endpoint                                 |
| `SCUFRIS_DESKTOP_HOTKEY`          | Activation accelerator                                 |
| `SCUFRIS_DESKTOP_CHAT_COMMAND`    | Absolute executable that opens the full chat           |
| `SCUFRIS_DESKTOP_RESTART_COMMAND` | Absolute executable that restarts the backend          |

## Limits

- The companion is Linux and X11 only. Focus restoration uses
  `_NET_ACTIVE_WINDOW`; without a display it does nothing and the rest keeps
  working.
- No window manager is built. `i3` remains the app-level answer for ordinary
  windows, and the desktop session supplies the chat hook.
- Wake-word activation is not implemented. When it arrives it invokes the same
  activation path.
- Duplicate suppression is scoped to the authoritative session and bounded to
  the most recent 256 submissions. A retry that arrives after 256 newer ones, or
  against a different session, is delivered again.
- A removal of the durable transcript that keeps failing leaves a record behind.
  The next start recovers and resends it under its original identifier, which
  the daemon suppresses, so it cannot reach the conversation twice.
- The runtime learns that the pill window is gone only from a window operation
  that fails. Nothing watches the window between decisions, so a webview that
  dies while the pill sits open is noticed at the next operation, not when it
  happens.
- A decision that arrives while another thread is inside a surface operation is
  left to that thread rather than waited for, because waiting on the thread that
  holds the event loop is how the main thread deadlocks. Its actions run
  afterwards, on the applying thread, so they are ordered but not immediate.
- The repair chain asks a refusing window three more times and then stops. A
  window manager that recovers later is not noticed until the next decision.
