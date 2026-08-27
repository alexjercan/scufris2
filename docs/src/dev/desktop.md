# Desktop companion

`scufris-desktop` is the Scufris voice pill and tray companion. It is a Tauri
application built from the `native/` cargo workspace and shipped as its own
flake package, so consumers who never enable it never build Tauri.

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
- The pill rises into place from the bottom of the screen and shows a
  level-driven orb in the recording accent, with the recording duration on one
  dim line under it. The tray keeps the red privacy ring.
- `Escape` while recording stops and discards the recording.
- `Enter` while recording stops, transcribes, shows the sent text, and submits
  without another confirmation.
- The accelerator again while recording stops, transcribes, and opens an
  editable review state. `Enter` sends the reviewed transcript; `Escape`
  discards it.
- The pill is a resident HUD. The first activation brings it up and it then
  stays on screen, resting between interactions and showing what the
  assistant is doing - idle, working, speaking, attention, disconnected.
  `Escape` is the only thing that puts it away, and the next activation is
  what brings it back. Nothing the assistant does ever raises a dismissed
  pill or takes a resting one down.
- Cancellation restores focus to the previous window. Submission restores it
  at the handoff, and the pill stays up without the keyboard while the turn
  runs - sent, working, speaking - then settles back to resting.

If the microphone cannot start, or an open capture stream fails, the pill drops
the recording indicator and states why. If transcription fails, nothing is
submitted and the pill states why. If the daemon refuses a submission before it
leaves the companion, the transcript is retained and `Enter` sends it again. If
a submission leaves the companion and is not acknowledged, the transcript is
retained as **uncertain** instead: `Ctrl+C` copies it, `Escape` discards it, and
`Enter` says what sending it again would risk before a second `Enter` does it.
Activation never discards a retained transcript.

Every one of these transitions lives in `native/scufris-desktop/src/state.rs`
as a pure state machine, so the whole interaction is tested without a display.
Everything the runtime reaches outside itself - the microphone, the endpoint,
the socket, the window, the disk, and where deferred work runs - is a port on
`App`, so the failure paths are tested without any of them.

## Keys that do not need the window

A key typed at the pill only arrives while the pill holds the keyboard, and the
pill holding the keyboard is the pill taking it away from whatever the person
was typing in. So the same three keys have a second road in, and it does not go
through the window at all.

`native/scufris-desktop/src/command.rs` listens on a Unix socket at
`$XDG_RUNTIME_DIR/scufris/desktop.sock`, beside the daemon socket and not it.
This is the one place the companion is the server: `daemon.rs` connects out, and
this listens for the person's own window manager. One LF-terminated JSON line
each way, one verb per connection, and the connection closes:

```
{"v":1,"verb":"open"}      ->  {"v":1,"answer":"taken"}
{"v":1,"verb":"cancel"}    ->  {"v":1,"answer":"refused","detail":"..."}
{"v":1,"verb":"accept"}
```

`scufris-ctl <verb>` is the client. It is its own flake package, installed by
whichever half of Scufris is enabled, because a window manager binding runs it
by name and a terminal reaches the background service with it. Its exit status
is what a binding can branch on: 0 the verb reached the pill, 1 it did not, 2
the run was wrong. See [Background service](service.md) for its other verbs.

`open` and `cancel` go straight to the state machine, as `Activate` and
`Escape`. `accept` does not: the pill page holds the editable field, so the verb
is emitted to the page as `scufris://accept` and the page sends whatever a
person's own `Enter` would have sent. The socket is the person's alone, in their
own runtime directory under a private one; anything that can open it can already
act as them. A session with no runtime directory gets no command socket and
starts anyway.

`native/scufris-desktop/src/keys.rs` is the other half: it arranges, for each
posture the pill takes, where those keys are read.

- A **binding mode**, through the `SCUFRIS_DESKTOP_MODE_COMMAND` hook, run with
  one argument. The companion asks for `scufris` while the pill is focused and
  `default` for every other posture, including as it exits. i3 and sway hold
  bare Escape and Return inside a named mode, which is the only way a bare key
  reaches the pill without being taken off the desktop for every other program.
  The window manager enters the mode when the person opens the pill; the
  companion is what leaves it, so a pill that closed for a reason nobody asked
  for does not leave the keyboard in a mode.
- **Modified accelerators** the display grabs, for a desktop with no binding
  modes. They are built from the activation hotkey's own modifiers, so `Super+D`
  gives `Super+Escape` and `Super+Enter`, and they are grabbed only while the
  pill is on screen - an accelerator held all session is one no other program
  can use. A hotkey with no modifier grabs nothing: a bare accelerator the
  display granted the companion is a key no other program would see again.

Both run for every posture change and neither is required. A window manager that
already holds one of the accelerators refuses the grab, which is the good case:
its own binding runs `scufris-ctl` and arrives in the same place.

The activation accelerator itself follows the same rule. Under the binding mode
recipe the window manager owns `$mod+d`, so the companion's own registration of
it is refused; that is logged and the companion starts anyway. X reports a key
another client has grabbed as `BadAccess`, which the display layer surfaces as
"already registered", so that is what the log line says.

## Pill design

The pill is the orb. The window is a small square around the dotted thought
orb at the engine's tuned 64 pixel preset, and the orb's shape and accent are
the whole state - no label, no transcript line, no waveform. The design is the
Orb Study, section 03, kept at `tasks/20260825-231826/orb-study.html`. One rule
set governs it: input states react to you, processing states animate
themselves, and red belongs only to error and the mic ring. Motion pattern, not
hue, separates states, so every state survives grayscale.

| State                 | Color    | Orb                                        |
| --------------------- | -------- | ------------------------------------------ |
| listening             | yellow   | wave, breathing with the mic level         |
| transcribing          | brown    | ribbon, composing                          |
| review                | quartz   | ring, near-still; the box carries the text |
| working               | niagara  | orbits, fast                               |
| speaking              | green    | wave, same grammar as listening            |
| attention             | wisteria | ring plus two pulses, then a slow loop     |
| retained or uncertain | wisteria | attention-class: the words need you        |
| error                 | red      | ring, slowed to a third                    |
| disconnected          | gray     | web, desaturated                           |

Chrome exists only when there is something to act on. In review and in
uncertain a second window rises above the orb with the transcript, a caret
where the person's own caret is, and one line saying what the keys do. It is
display-only and never focused: every key still belongs to the orb window, so
the field the words come from lives there, invisible, and each edit is mirrored
next door over `scufris://draft`. Everything else - the state name, the error
reason, the notice on a retained transcript - is the tray's to say.

Both windows are exactly their opaque page. Margins around them would need
per-pixel alpha, which bare X11 without a compositor discards - the margins
render black. Painting the window in the same near-black the orb's far dots
fade into is what makes the frame disappear on a dark desktop.

The listening timer is one dim line under the orb, and its row is reserved in
every state. Equal min and max size hints cannot be changed while a window is
up without re-applying them, and a frame that resized under the orb would move
the orb.

The webview is CSS plus one small Canvas 2D orb, written in strict
TypeScript (`ui/pill.ts` and `ui/review.ts`) and compiled by `build.rs` into
`ui/dist`, the directory Tauri embeds. WebGL is banned: WebKitGTK silently
software-renders. The displayed level springs toward the 60 ms Rust tick with
`display += (target - display) * 0.25`, and one `requestAnimationFrame` loop
drives both it and the orb. The loop stops outright while the window is hidden,
because WebKit throttles a hidden page rather than pausing it.
`prefers-reduced-motion` stops every animation on both windows and paints one
still orb per state change, and the page reports the same preference to the
host over `pill_ready` so the window arrives in place instead of rising.

Four earcons mark the boundaries the eye can miss: mic open (rising), mic
close (falling), attention (one chime, also for a retained or uncertain
transcript), error (one low tone). Nothing plays for working or speaking.
They are soft Web Audio tones, enabled at every start, and the tray menu
mutes them for the session. Each cue logs at DEBUG under the `webview`
target, and a cue the audio policy silences is said once at WARN, so a quiet
pill is explainable from journalctl.

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

Where the pill belongs has three answers, not two. **Focused** holds the
keyboard for the phases the person types into. **Passive** is up without the
keyboard: the handoff, the turn after it, and every rest between
interactions, where the pill reports the assistant while the keys go to the
person's own window. **Off** is down, and only the person puts it there. The
passive posture is why the keyboard comes back at the handoff instead of at
the answer. Visibility itself is the person's: Escape dismisses the pill,
activation brings it home, and nothing the assistant does changes either.

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
a pill that would not take the keyboard cannot be typed into; a passive pill
still holding the keyboard would swallow the keys the person is typing into
their own window, so the repair hands them back. All are asked again after a
short delay, on the newest decision rather than the one that fell short, a
bounded number of times, one chain at a time. A window manager that has
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

## Control protocol v2

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
{"v":2,"type":"hello"}
{"v":2,"type":"submit","id":"pill-1","text":"show me a note"}
{"v":2,"type":"submit","id":"pill-1","text":"show me a note","force":true}
{"v":2,"type":"ping"}
{"v":2,"type":"widget_opened","id":"w-1","surface":"widget-3"}
{"v":2,"type":"widget_done","id":"w-4"}
{"v":2,"type":"widget_failed","id":"w-1","code":"widget_not_found","detail":"no widget named cpu"}
{"v":2,"type":"widget_event","surface":"widget-3","event":"closed"}
{"v":2,"type":"catalog","widgets":[{"id":"note","name":"Note","description":"A short note beside the pill."}]}
```

`force` is the person's own decision to send words that may already be in the
conversation. It is absent from every ordinary submission, and no timeout,
reconnection, or restart ever sets it.

Daemon to companion:

```json
{"v":2,"type":"welcome","session":"2026-08-24"}
{"v":2,"type":"ack","id":"pill-1"}
{"v":2,"type":"uncertain","id":"pill-1","detail":"the outcome is unknown"}
{"v":2,"type":"refused","id":"pill-1","detail":"the Scufris session is not ready"}
{"v":2,"type":"state","state":"idle","detail":""}
{"v":2,"type":"pong"}
{"v":2,"type":"widget_open","id":"w-1","widget":"note","posture":"exhibit","data":{"text":"the harness is green"}}
{"v":2,"type":"widget_update","id":"w-2","surface":"widget-3","data":{"text":"141 tests pass"}}
{"v":2,"type":"widget_close","id":"w-3","surface":"widget-3"}
{"v":2,"type":"widget_clear","id":"w-4"}
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

The widget commands are the first messages the daemon originates. Each carries
a correlation `id` that the companion echoes in its answer, so a caller waits
for its own command and never for another's. `widget_opened` answers an open
and names the surface it created; `widget_done` answers an update, a close, or
a clear, which name no new surface; `widget_failed` answers any of them with a
stable code. `widget_event` and `catalog` are unsolicited: the catalog is sent once per
connection, right after the welcome, so the daemon can type its tools from what
this companion actually has.

Both peers reject unknown message types and any version other than `2`; they do
not ignore them. A version 1 peer is refused at hello rather than half
understood. Submission identifiers and transcripts are bounded and
validated before a submission reaches the conversation. The transcript bound is
8 KiB measured in **UTF-8 bytes** on both sides: measuring UTF-16 code units on
one side would let non-ASCII text be accepted that the other cannot store or
read back. A daemon that finds the
socket already owned by a live peer refuses to bind rather than stealing it; a
stale socket file is replaced.

Listening and transcribing are companion-local. The daemon never sees audio.

Correlation, widget, and surface identifiers follow the submission identifier
rule: bounded ASCII that is also safe as a window label. Widget payloads are
capped at 8 KiB, well below the line cap, because the same bytes cross the
companion's per-window channel afterwards.

The protocol is implemented twice, once per side, and each side owns its tests:

- `native/scufris-control/src/lib.rs` for the companion.
- `extensions/scufris/desktop/protocol.ts` and `server.ts` for the daemon.

Neither implementation is the reference. `native/control-protocol-v2.json`
holds canonical, tolerated, and rejected lines for both directions, and both
suites read that same file, so the two sides cannot drift apart.

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
the status menu: the current status line, open chat, start voice input,
restart the backend, the sound cue switch, and quit. The chat and restart
items are disabled when the deployment configures no hook.

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

`nix/desktop.nix` builds `-p scufris-desktop` out of the workspace with
`rustPlatform.buildRustPackage`, runs that package's tests in its check phase,
and wraps the binary with the WebKitGTK and tray libraries it dlopens. The
result also ships a desktop entry and an icon. `scufris-ctl` is not in it:
`nix/service.nix` builds the client, and the module installs it beside whichever
half of Scufris is enabled.

`nix/checks.nix` asserts that the companion, WebKitGTK, and their closure stay
out of the default and voice launcher closures, that `--print-config` resolves
defaults and overrides exactly, that relative hooks and non-HTTP endpoints are
rejected, and that the module wires the services and the restart hook it claims.

`scufris-desktop --print-config` prints the resolved configuration and exits
without starting a window, which is what makes that check cheap.

## Logging

The companion logs through `tracing`. Without `--foreground` it sends
structured fields straight to journald and falls back to stderr when no
journald socket is reachable; `--foreground` forces pretty stderr output with
ANSI colors when stderr is a terminal, which is the development view:

```bash
RUST_LOG=debug nix run .#scufris-desktop -- --foreground
journalctl --user -t scufris-desktop -f
```

The level policy keeps the steady state quiet. INFO carries lifecycle and
state transitions only: starting and stopping, every phase change of the pill
state machine (one log point under the companion lock, so none is missed),
daemon connect, and assistant state changes. DEBUG carries per-request detail:
whisper request timing and sizes, daemon submissions and answers, and the
webview console, which is forwarded through the `pill_log` command under the
`webview` target. WARN is degraded operation and ERROR is a user-visible
failure. `RUST_LOG` overrides all of it. Transcripts never reach the log at
any level; only their sizes do.

## Environment

| Variable                          | Meaning                                                |
| --------------------------------- | ------------------------------------------------------ |
| `SCUFRIS_DAEMON`                  | `1` in the popup process, which then serves the socket |
| `SCUFRIS_DESKTOP_SOCKET`          | Control socket path override                           |
| `SCUFRIS_DESKTOP_COMMAND_SOCKET`  | Command socket path override                           |
| `SCUFRIS_DESKTOP_STATE_FILE`      | Durable accepted-transcript file                       |
| `SCUFRIS_STT_ENDPOINT`            | Transcription endpoint                                 |
| `SCUFRIS_DESKTOP_HOTKEY`          | Activation accelerator                                 |
| `SCUFRIS_DESKTOP_CHAT_COMMAND`    | Absolute executable that opens the full chat           |
| `SCUFRIS_DESKTOP_RESTART_COMMAND` | Absolute executable that restarts the backend          |
| `SCUFRIS_DESKTOP_MODE_COMMAND`    | Absolute executable that sets the binding mode         |

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
