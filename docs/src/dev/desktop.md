# Desktop companion

`scufris-desktop` is the Scufris voice pill and tray companion. It is a Tauri
application built from the `native/` cargo workspace and shipped as its own
flake package, so consumers who never enable it never build Tauri.

## Ownership

scufris-desktop owns:

- Activation, the pill window, the textbox window, the conversation window,
  and the tray.
- Microphone recording and cancellation.
- Local transcription against the configured endpoint.
- Transcript editing, in the textbox.
- Drawing the conversation, in the conversation window, and typing into it.
- Backend health presentation and bounded restart requests.
- Focus restoration.

- Speaking, which is the one thing here that makes sound.

`scufris-service` owns the authoritative conversation, session persistence,
agent execution, tools, delegation, and the decision about what is worth saying
aloud. The companion is one of its clients, in the `frontend` role. It never
writes a Pi session file. It submits accepted transcripts as ordinary user
messages and follows the state the service reports. See
[Background service](service.md).

A terminal holding `scufris-ctl debug` is the same conversation from the other
side. Putting the pill away does not stop the service, and a companion crash
does not stop the conversation.

## Interaction

- The activation accelerator, `Super+D` by default, carries two gestures. A tap
  brings the workspace up - the bottom-center pill and the panels beside it -
  and the next tap puts it away. Neither touches the microphone. The pill never
  takes the keyboard.
- Holding the accelerator is push to talk. The microphone opens at
  `keys::HOLD`, a quarter of a second, which no deliberate tap reaches and no
  voice beats; releasing ends the take. `keys::Hold` is where a press waits to
  find out which gesture it was, and it counts presses so a timer that woke
  late cannot open the microphone for the press after the one it timed.
- The pill rises into place from the bottom of the screen and shows a
  level-driven orb in the recording accent, with the recording duration on one
  dim line under it. The tray keeps the red privacy ring.
- Ending the take transcribes and raises the textbox over the pill with the
  words in it. The textbox is the one window here that takes the keyboard.
- `Enter` in the textbox sends what is in it. `Escape` discards it, and
  `Ctrl+C` copies it. They are ordinary keys in a focused window, so every
  other editing key is the field's own.
- `Super+Escape` cancels the take. It is grabbed from the display, so it
  reaches the companion wherever the keyboard is. `cancelKey` names another
  accelerator, and `"none"` leaves the key to the desktop. With panels on the
  layer it is a cancel that leaves the workspace standing, and the press after
  it is the dismissal; with nothing on the layer it is the dismissal outright.
  `App::escapes_to` makes that call, because whether there is a workspace to go
  back to is a fact about the widget layer and the layer is the host's.
- `Super+Delete` stops Scufris: it cuts the speech and ends the run. Grabbed
  the same way and on the same terms, and named by `stopKey`. Nothing else is touched - a transcript
  being edited stays where it is - and the pill goes on reporting `working`
  until the service says the run ended, because the service is what knows.
  With no run to end it does nothing.
- A click on the pill puts the conversation window up, and puts it away again.
  It is the pill's only pointer gesture and the shortest road to the
  conversation from an orb that carries no label.
- `scufris-ctl hud` does the same thing from a window manager binding. The
  window draws the transcript stream the service pushes to every frontend and
  types back on the same socket. `Enter` sends, `Shift+Enter` is a newline,
  and `Escape` closes it - ordinary keys in a focused window, the way the
  textbox's are. It has no accelerator of its own; see the command socket
  below for why.
- The pill is resident. The first tap or hold brings it up and it then stays on
  screen, resting between interactions and showing what the assistant is doing -
  idle, working, speaking, attention, disconnected. A tap, `Super+Escape`, and
  `scufris-ctl hide` put it away, and a tap, a hold, or `scufris-ctl show`
  bring it back. Nothing the assistant does raises a dismissed pill or takes a
  resting one down, with one exception: a panel Scufris opens to answer a
  running turn raises the layer, because an answer nobody can see is not an
  answer. `answers` in `main.rs` is that rule, and it is narrow - an exhibit,
  and the assistant working or speaking. A summoned instrument does not, and
  neither does an exhibit that arrives with nothing running.
- Holding the activation key always listens, so barge-in needs no key of its
  own.
  `DesktopSurface::present` cuts the speaker on every presentation that says
  the microphone is open, which makes it one rule rather than one transition:
  pressing the key while Scufris talks stops the sentence, and nothing is
  spoken for as long as the take is running.
- Cancellation restores focus to the previous window. Submission restores it at
  the handoff, when the textbox goes, and the pill stays up while the turn runs
  - sent, working, speaking - then settles back to resting.

If the microphone cannot start, or an open capture stream fails, the pill drops
the recording indicator and states why. If transcription fails, nothing is
submitted and the pill states why. If the service refuses a submission before it
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

The pill is the indicator, and an indicator that holds the keyboard is one that
took it away from whatever the person was typing in. So the pill refuses focus
on every show, and the two keys that reach it do not go through its window at
all. The textbox is the other half of the same rule: it takes the keyboard,
because a window with a caret in it is one the person meant to be typing in.

`native/scufris-desktop/src/command.rs` listens on a Unix socket at
`$XDG_RUNTIME_DIR/scufris/desktop.sock`, beside the service socket and not it.
This is the one place the companion is the server: `link.rs` connects out to
the service, and this listens for the person's own window manager. One LF-terminated JSON line
each way, one verb per connection, and the connection closes:

```
{"v":1,"verb":"open"}  ->  {"v":1,"answer":"taken"}
{"v":1,"verb":"hud"}   ->  {"v":1,"answer":"refused","detail":"..."}
```

`scufris-ctl open`, `hud`, `show`, and `hide` are the clients. They are their
own flake package, installed by
whichever half of Scufris is enabled, because a window manager binding runs it
by name and a terminal reaches the background service with it. Its exit status
is what a binding can branch on: 0 the verb reached the pill, 1 it did not, 2
the run was wrong. See [Background service](service.md) for its other verbs.

`open` goes straight to the state machine as `Activate`: it starts a take, and
it stops one that is running. That is the two-press gesture, not the hold - a
binding is one press, and a desktop that takes the hotkey for itself trades tap
and hold for this. `hud` puts the conversation window up if it is down and down
if it is up, and it is reported rather than assumed: the caller can see whether
a window came up, so a window that refused is worth saying out loud in their
terminal.

`show` and `hide` are `Reveal` and `Dismiss`, the workspace with no microphone
behind it. Two verbs rather than one toggle, unlike `hud`: what sends these is
not always a key, and a script that means to leave the screen clear has to be
able to say so without first asking what is on it. They are taken rather than
reported, as `open` is - the workspace is a layer, and asking for one that is
already up is not a refusal but a request that was already true.

All four are windows and none carries words. Everything that carries words goes
to the service socket, where `send` and `abort` already live.

This socket is also why the conversation window has no accelerator. The
companion grabs the activation key for the whole session already, and every
further grab is a key no other program on the desktop can use again; a binding
the person writes in their own window manager configuration costs it nothing.
The socket is the person's alone, in their own runtime directory under a private
one; anything that can open it can already act as them. A session with no
runtime directory gets no command socket and starts anyway.

`native/scufris-desktop/src/keys.rs` is the other half. It grabs two modified
accelerators from the display - `Super+Escape` and `Super+Delete`, for the
default `Super+D` - built from the activation hotkey's own modifiers, and it
grabs them only while the pill is on screen: an accelerator held all session is
one no other program can use. A hotkey with no modifier grabs nothing, because a
bare accelerator the display granted the companion is a key no other program
would see again.

Deriving them is the default rather than the rule. `cancelKey` and `stopKey`
name either of them instead, and `"none"` takes one off the companion entirely,
which is the answer for a desktop that already means something by
`Super+Escape`. An accelerator that will not parse leaves no key and says so in
the log, rather than quietly falling back to the derived one: a working key on
the wrong accelerator is harder to notice than a key that does nothing.

Stop is its own key rather than a second meaning for Escape. Escape puts a pill
away and throws away a take, and neither reaches the conversation; stop ends a
run that may be part way through changing something. A gesture with that much
behind it is not one to arrive at by pressing the dismiss key at the wrong
moment.

Neither road is required. A window manager that already holds the accelerator
refuses the grab, which is the good case: its own binding runs `scufris-ctl` and
arrives in the same place.

The activation accelerator itself follows the same rule. Where the window
manager owns `$mod+d`, the companion's own registration of it is refused; that
is logged and the companion starts anyway. X reports a key another client has
grabbed as `BadAccess`, which the display layer surfaces as "already
registered", so that is what the log line says.

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
| editing               | quartz   | ring, near-still; the box carries the text |
| working               | niagara  | orbits, fast                               |
| speaking              | green    | wave, same grammar as listening            |
| attention             | wisteria | ring plus two pulses, then a slow loop     |
| retained or uncertain | wisteria | attention-class: the words need you        |
| error                 | red      | ring, slowed to a third                    |
| disconnected          | gray     | web, desaturated                           |

Chrome exists only when there is something to act on. While the words are the
person's to answer - editing, retained, uncertain - the textbox rises above the
orb with the transcript in a real field and one line saying what the keys do.
It is the window that holds the keyboard, so the caret and the selection are
the browser's own and the page decides nothing: it reports which key was
pressed and what the field holds. Everything else - the state name, the error
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
TypeScript (`ui/pill.ts` and `ui/textbox.ts`) and compiled by `build.rs` into
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
socket among them - when the service's answer arrives on the link's own thread.
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
the answer. Visibility itself is the person's: a tap or Escape dismisses the
pill, a tap or a hold brings it home, and the one thing the assistant does that
changes either is opening a panel to answer a turn the person started.

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
what it achieved, and the runtime records only that. Showing a window asks it
afterwards whether it is up; raising the textbox asks it as well whether it
holds the keyboard, and treats a window that cannot answer as one that has not:
a window manager may accept `set_focus` and hand the keyboard elsewhere, or
later, or never. `hide` asks the same question in reverse, because an
always-on-top pill recorded as down is a pill nothing will ever take down again.

A show has three answers, not two.

- **Ready** - up, on top, holding the keyboard. Only the textbox reaches this;
  the pill refuses the keyboard on every show by contract.
- **Seen** - up and on top, keyboard elsewhere. The person can read it, so the
  indicator counts and the phase runs.
- **Doubtful** - up, but placement or always-on-top was refused, so it may be
  behind whatever the person is looking at. The phase has a window and runs, but
  nothing that rests on being seen may rest on this.

The focus hints go on before the window is mapped, on every raise. A window
manager reads them when it maps a window, so a pill that refuses the keyboard
once and a textbox that claimed it once are both wrong the second time round.

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
itself. A pill that would not go down is over the desktop and has no keys of
its own, so nothing outside the runtime can even send it an Escape; a textbox
that would not take the keyboard cannot be typed into; a textbox left standing
after the phase that needed it would swallow the keys the person went back to
typing with, so the repair takes it away. All are asked again after a
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

## The conversation window

`native/scufris-desktop/src/conversation.rs` holds every decision the window
makes and `src/hud.rs` runs them, the way `state.rs` and `pill.rs` split the
pill. It is a sibling of the pill's state machine rather than a phase of it:
the pill machine is about one take and ends with it, and the conversation
outlives every take. Typing here never raises the textbox, never ends a
recording, and never moves the pill off whatever it is showing.

Three ways reach it and all three toggle: a click on the pill, the tray's
"Show conversation", and `scufris-ctl hud`. The click is the pill's only
pointer gesture. Pointer input has nothing to do with focus, so an unfocusable
window still receives it, and the orb carries no label and never will - a
cursor is the only way it can say the click does something.

Scufris can put it up too, with `scufris_conversation`. That one says `show`
and `close` rather than toggling: the person's gestures toggle because the
person can see the screen, and the agent cannot, so a toggle from it would do
one of two opposite things and could not tell which. It travels as
`{"type":"conversation","id":...,"up":...}` on the service socket, and the
service answers it rather than the frontend - the window does not half raise,
so `no_frontend` is the only failure worth reporting and the service is what
knows it.

Two rules follow, and they are the whole of the design.

**Two senders, one verb.** The textbox sends a transcription and this window
sends a typed line; both are a `submit` on the same socket, and both are the
same message to the service. They share the process prefix, because that is
what makes an identifier this companion's, and they cannot share the counter:
an identifier is how an answer is matched to the line that asked, and two
senders naming a line the same thing would take each other's answers, so a `-h`
goes in the HUD's.

**No durable copy.** The pill persists an accepted transcript because spoken
words cannot be typed again, and this window persists nothing: the line is
still in the field until the service takes it. Nothing is appended locally on
the way out either. The line comes back as a transcript entry, which is what
puts it on screen - so what the window shows is the conversation rather than
this process's hopes about it.

The window holds a copy of the service's own 200-entry transcript ring and
never a longer history. A frontend that connects is replayed the whole ring, so
a reconnection would otherwise show every line twice; the window empties itself
on `Connected` and lets the replay refill it. The two then hold exactly the
same lines. A line typed in a terminal with `scufris-ctl send` appears here the
same way one typed into the window does, because both arrive as the same push.

One line is in flight at a time. A second `Enter` on an unanswered line would
put two questions in the conversation from one intention, and the words are
still in the field either way. A disconnection gives up on whatever is in
flight: no reconnection brings that answer, and a window that kept waiting
would refuse every line after it for the rest of the session.

What it draws is text. Not markdown, not tool calls, not thinking - the
service's transcript is what was said, and the session file and
`scufris-ctl debug` are where the rest of a run lives. The terminal is not a
fallback for this window; it is a whole Pi session, and this will not be one
for a long time.

## One identifier per transcript

A transcript keeps one submission identifier from the moment it is transcribed
until it is acknowledged - across edits in the textbox, across retries, and
across a
restart. Identifiers carry a per-process prefix drawn from the operating
system's randomness, because they outlive the process that made them: a process
identifier and a clock are reused, and a collision would have a genuinely new
request refused as a duplicate.

## A spoken prompt is an ordinary prompt

A transcript reaches the conversation the way a typed prompt does. The
companion writes one `submit` to the service; the service hands it to its agent
as an RPC prompt, which is the same path Pi takes for a person typing into it -
the same per-turn Scufris system prompt, the same pre-send compaction check,
the same handlers. A submission that arrives while the agent is working is
delivered as a steer rather than refused, which is what makes one activation
key enough.

The companion never writes a session file and never reads one. Whether the
words are in the conversation is the service's answer, not something the
companion works out for itself.

## An acknowledgment means the agent took the words

`submit` carries a companion-owned `id`, and the answer echoes it. `ok` means
the agent accepted the prompt. `refused` means it did not, and says which of
the stable refusal codes applies. Both name the submission they answer, and the
companion applies an answer only to that one: it holds one transcript at a time
and may have started another by the time a slow answer arrives.

There is no third answer on the wire. The service answers every submission one
way or the other, so the companion's remaining uncertainty is about the service
itself rather than about the conversation.

## Accepted, uncertain, and unsent are three different answers

| State     | How it is reached                                | What happens                                  |
| --------- | ------------------------------------------------ | --------------------------------------------- |
| accepted  | `ok` naming this submission                      | Acknowledged, the pill settles                |
| unsent    | `refused`, or a write that never left            | Retained and editable; `Enter` sends it again |
| uncertain | No answer within 15 seconds, or a recovered file | Only the person decides                       |

"Uncertain" is the one that matters. A request that reached the conversation
can have written files, sent messages, or started work, so treating "I did not
see it answered" as "it did not land" would run it a second time. Nothing
automatic ever resolves it: not a reconnection, not a companion restart, and
not an ordinary `Enter` in the pill.

What the person is offered instead is the truth and three choices. The pill
shows the transcript and says the outcome is unknown; `Ctrl+C` copies the words
somewhere safe, `Escape` discards them for good, and `Enter` first says what
sending again could repeat and only then, on a second `Enter`, sends it.

That confirmation is entirely the companion's. Nothing about it crosses the
socket: the service has no notion of a forced submission, and a second `Enter`
writes exactly the same `submit` the first one would have.

A transcript recovered after a companion restart is uncertain for the same
reason: the previous process may have submitted it.

## An ambiguous retry cannot confirm text it did not send

A retained transcript is editable only when the companion knows the words never
left - a refused submission, or a write that failed. An uncertain transcript is
frozen: an edit would let the pill close as though the new words landed while
only the original body may be in the conversation.

A discard is likewise final. Escape removes the durable record; if removal is
impossible the record is replaced by a tombstone, which still stops the text
being restored, and only when neither can happen does the pill reopen to say
the words are still on disk.

## The link to the service

`native/scufris-desktop/src/link.rs` holds one connection to
`$XDG_RUNTIME_DIR/scufris/service.sock` and reconnects with a bounded backoff,
250 ms doubling to 5 s, for as long as the companion runs. It says
`{"v":3,"type":"hello","role":"frontend"}` and then never says hello again on
that connection. A companion with no service reports the backend as
unavailable in the tray and refuses to submit rather than queueing.

The protocol itself belongs to the service; see
[Background service](service.md) for the message table. What a frontend is
pushed is state, transcript, spoken paragraphs, and widget commands; what it
sends is submissions, aborts, and widget reports.

Listening and transcribing stay companion-local. The service never sees audio.

## Speaking

The agent shapes every answer as one prose paragraph and pushes it as `speak`.
It is not a speech decision: the paragraph is the shape of a Scufris answer
whether or not anything is listening, and what it makes possible is that a
speaker has something safe to say. Every decision about sound is here.
`native/scufris-desktop/src/speech.rs` runs the configured
`SCUFRIS_DESKTOP_SPEAK_COMMAND` with the paragraph on its standard input. A
companion with no synthesiser configured stays silent, which is a deployment
without one rather than a fault.

The mute is here too, as "Mute Scufris" in the tray. It belongs to the speaker
rather than to the conversation, so nothing about it reaches the wire and the
agent is neither asked nor told. A muted companion still receives every
paragraph and cuts what is playing; unmuting takes effect on the next answer
with nothing to restore.

One utterance at a time. A new paragraph cuts the one being spoken, and so does
the microphone opening: a person who has started talking is not waiting for the
rest of the sentence. Paragraphs are stripped of control characters and cut to
1000 UTF-8 bytes on a character boundary, which is the same bound the helper
measures.

Speaking is not a service state. `ScufrisState` has no `speaking`, because
speaking is what a frontend is doing rather than what Scufris is doing. The
companion keeps its own flag beside the state the service reported and shows
the two composed, so the pill and the tray say `speaking` while the service
still says `working` or `idle`.

## Assistant state

The service reports one word out of a deliberately small vocabulary -
`starting`, `idle`, `working`, `detached`, `error` - and the companion never
parses a Pi event. An event Pi adds tomorrow is a service change and nothing
else. See [Background service](service.md).

The companion composes its own states over that word, because they are things
it is doing and the service cannot see them:

| State          | Where it comes from                    |
| -------------- | -------------------------------------- |
| `listening`    | The microphone is open                 |
| `transcribing` | A recording is at the endpoint         |
| `speaking`     | The synthesiser is playing             |
| `attention`    | A transcript is waiting for the person |
| `disconnected` | The link to the service is closed      |

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
service connect, and assistant state changes. DEBUG carries per-request detail:
whisper request timing and sizes, service submissions and answers, and the
webview console, which is forwarded through the `pill_log` command under the
`webview` target. WARN is degraded operation and ERROR is a user-visible
failure. `RUST_LOG` overrides all of it. Transcripts never reach the log at
any level; only their sizes do.

## Environment

| Variable                          | Meaning                                       |
| --------------------------------- | --------------------------------------------- |
| `SCUFRIS_DESKTOP_SOCKET`          | Service socket path override                  |
| `SCUFRIS_DESKTOP_COMMAND_SOCKET`  | Command socket path override                  |
| `SCUFRIS_DESKTOP_STATE_FILE`      | Durable accepted-transcript file              |
| `SCUFRIS_STT_ENDPOINT`            | Transcription endpoint                        |
| `SCUFRIS_DESKTOP_HOTKEY`          | Activation accelerator                        |
| `SCUFRIS_DESKTOP_CANCEL_KEY`      | Key that puts the pill away, or `none`        |
| `SCUFRIS_DESKTOP_STOP_KEY`        | Key that stops Scufris, or `none`             |
| `SCUFRIS_DESKTOP_CHAT_COMMAND`    | Absolute executable that opens the full chat  |
| `SCUFRIS_DESKTOP_RESTART_COMMAND` | Absolute executable that restarts the backend |
| `SCUFRIS_DESKTOP_SPEAK_COMMAND`   | Absolute executable that speaks one paragraph |

## Limits

- The companion is Linux and X11 only. Focus restoration uses
  `_NET_ACTIVE_WINDOW`; without a display it does nothing and the rest keeps
  working.
- No window manager is built. `i3` remains the app-level answer for ordinary
  windows, and the desktop session supplies the chat hook.
- Wake-word activation is not implemented. When it arrives it invokes the same
  activation path.
- Nothing deduplicates submissions. A transcript the person chooses to send
  again after an uncertain outcome is delivered again, which is why that
  choice is theirs and takes two presses.
- A removal of the durable transcript that keeps failing leaves a record
  behind. The next start recovers it as uncertain and waits for the person,
  rather than sending it.
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
