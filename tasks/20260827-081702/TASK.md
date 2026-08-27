# Scufris as a service: the architecture inversion

- STATUS: CLOSED
- PRIORITY: 90
- TAGS: architecture, desktop

## Design (2026-08-27)

Design page: `scufris-service.html` in this directory. Published copy:
https://claude.ai/code/artifact/1e6a2360-639d-4fe3-b51b-451ca216b3de

The proposal, from Alex: a `scufris-service` daemon owns `pi --mode rpc`
and the session, serves one socket, and both the agent and the desktop
app connect to it as clients. The desktop app owns whisper and piper, so
the service handles text only. The pill becomes an indicator, one focused
textbox is the only thing that sends, and the HUD renders the Calm
conversation.

Three corrections the design makes to the sketch, all recorded on the
page:

- Voice is a capability, not a mode. The HUD is a surface, not the other
  half of a toggle. Reading the transcript while talking is ordinary.
- The textbox is the single sender. Voice fills it; text mode opens it
  empty. That collapses the review state and the recording state into one
  window, at the cost of two keypresses plus Enter for a voice
  submission.
- Pi's RPC event stream already answers what Scufris computes by hand:
  assistant state (`agent_start` / `agent_settled`), the transcript
  (`message_end`, `get_entries`), talking over a running turn
  (`streamingBehavior`), and stop (`abort`). Reading
  `node_modules/@earendil-works/pi-coding-agent/docs/rpc.md` before
  building removes work rather than adding it.

One requirement found while reading that document: in RPC mode, extension
UI requests (`notify`, `confirm`, `input`, `select`, `editor`) are emitted
on stdout and wait for an answer on stdin. The TUI answers them today. A
service that does not answer them hangs the agent. Scufris extensions
currently use only `notify`, so the exposure is small, but the service
must answer every type.

## Revision 2 (2026-08-27)

Alex reviewed revision 1 on the page: fifteen comment threads. All five
decisions settled, and four laws added that bound the rest.

- **L1** One of everything, on one machine. One `pi --mode rpc`, one
  `scufris-desktop`, one person, one host. "Service" means a background
  process on Alex's own machine: no tenancy, no account, no
  authentication, no network listener, no second device. The socket is a
  Unix socket in the user's runtime directory, so anything that can open
  it can already act as them. Every "what if two..." question is answered
  by this law rather than by code.
- **L2** Nothing is migrated. No backwards compatibility in any
  direction. Protocol v3 replaces v2 rather than coexisting. Stale state
  on disk, including the job tree under `$XDG_STATE_HOME/scufris/`, is
  deleted rather than converted.
- **L3** The agent keeps working as it does now, plus one change. Jobs,
  tmux, delegation, skills, Calm, response shaping all stay inside `pi`.
  The only file this design touches there is the desktop extension, from
  server to client. The one change that does land inside the agent is not
  this design's: `tasks/20260826-183008/TASK.md`, "Scufris does what is
  asked, not a workflow" (p75, tags identity and workflow). It lands
  independently. Nothing here waits on it or is blocked by it.
- **L4** One job, one agent, no pipeline. The shape `20260826-183008`
  leaves the agent in. It bounds this design: the service routes a
  conversation, not a workflow, and nothing in the protocol knows a
  request can become several pieces of work.

Decisions, as settled by Alex:

- **D1** Two systemd units, one Home Manager surface. The flake exposes a
  single output for configuring Scufris and wires the units underneath.
- **D2** Rust for the service and everything native. TypeScript only
  where the code must run inside Pi as an extension.
- **D3** `desktop/` becomes `native/` with three crates.
- **D4** Nothing is kept as a fallback. The popup unit is retired
  outright. The HUD is built when we want it; the pill is the priority.
- **D5** Speech shaping does not move. The agent decides what to say
  aloud and its prose rules are unconditional; the frontend synthesises
  what it is handed and may refuse it.

Design changes from the feedback:

- The service maps RPC events onto one small `ScufrisState` enum. The
  frontend never parses a Pi event.
- Speaking and listening are frontend-local and never cross the socket.
  The pill, the tray and the widget clocks are all in the process that
  owns the speaker, so nothing outside it needs to know. There is no
  `speaking` or `voice` field on the wire.
- The activation key always means listen. Barge-in falls out of it:
  pressing it while Scufris speaks cuts the playback and reopens the
  microphone, and what is said then lands as a steer rather than a
  prompt.
- The textbox is voice-only. It is where a transcription is read before
  it is sent. Typing belongs in the HUD, and until the HUD exists typed
  input is `scufris-ctl send`.
- The HUD is what `Super+S` opens today, rebuilt as our own window.
- Fixed a broken arrow in the first figure: the ctl line pointed into
  the gap between two boxes instead of at the service.

## Answered: can a TUI attach to `pi --mode rpc`?

No. Checked against the installed package: no attach verb on the CLI, no
socket or multi-client mode in `docs/rpc.md`, nothing in `dist`
implementing session sharing. RPC mode is bound to the stdin and stdout
of whoever spawned it, one client, for its lifetime. A second `pi` on the
same session file would be a second writer.

Two things are possible instead, and both are in the design:

- `scufris-ctl debug` is one verb, not a pair. Alex asked for this: "so I
  don't have to `detach` `continue` `attach`, it's basically like starting
  a debugger". It stops the agent child, gets the exact session path back,
  runs `pi` on that path in the terminal it was called from, and gives the
  agent back on exit. No unit and no popup required.
- Pi appends the session as JSONL and publishes the path as
  `PI_SESSION_FILE`, so `tail -f` is a raw live transcript with no second
  writer.

### The detach is a lease, not a mode

The failure to design out is ending up detached with nothing to put it
back. So the detach is held by the control connection that asked for it.
When that connection closes the service starts the agent again: clean
exit, Ctrl-C, closed terminal, or the ctl killed outright. The kernel
closes the socket when the process dies, so nothing is remembered and
nothing is trapped. Same reasoning as
`tools/desktop/scufris-socket-lock`, where a lock is held by a pipe.

The rest is bookkeeping the verb does:

- The agent child is stopped the way any child is: stdin closed,
  `SIGTERM`, `SIGKILL` after a bound.
- The ctl runs `pi` on the returned path, not on `--continue`, so there
  is no question which session resumed.
- State goes to `detached`. Voice input is refused with a code rather
  than swallowed, because prompts travel on the agent's stdin.
- Widgets keep working. The terminal `pi` loads the same extensions and
  connects to the same socket in the `agent` role. Only prompts have
  nowhere to go.
- One at a time by L1. A second `debug` is refused while a lease is held,
  and so is a `debug` from something that is not a terminal.

## Increments

1. The service, headless. Rename the workspace to `native/`. Rust crate
   supervising one `pi --mode rpc`, owning the session directory, mapping
   events to `ScufrisState`, holding the transcript ring, answering
   extension UI requests, serving v3 with three roles. `scufris-ctl send
| state | abort | debug`. Nothing graphical changes.
2. The switch, one commit. Extension becomes a client, frontend becomes a
   client, piper moves into the frontend, and `desktop.sock`,
   `command.rs`, `keys.rs`, the socket lock, `SCUFRIS_DAEMON`, protocol
   v2, the popup unit and its options are deleted.
3. The textbox. Review state and Enter-while-recording deleted with it.
4. Listening is one rule: barge-in, steer, and a key for abort.
5. The extension keeps only what must run inside Pi. `voice/speech.ts`
   deleted, mute moves to the companion, `SCUFRIS_SPEECH` and
   `SCUFRIS_VOICE_AVAILABLE` deleted. See "D5 reversed" below.
6. The HUD, when we want it. Done; see "Increment 6 done" below.

## D5 reversed (2026-08-27)

D5 said speech shaping does not move. It is half right and the half that
is wrong has cost a working run: `SCUFRIS_SPEECH` was unset, the agent
decided nothing was worth saying, and a companion with a synthesiser
bound sat silent with nothing in any log to say why.

What is actually true, and what D5 should have said:

- **The prose rules stay.** They are not about speech. Scufris answers in
  one plain paragraph plus optional Markdown detail because that is the
  shape of the assistant, and it is that shape whether or not anything is
  listening. `plainProseParagraph` and `spokenResponseInstruction` in
  `voice/response.ts` are the whole of it, and `scufris_final_response`
  already computes the paragraph and puts it in the response entry.
- **The mode does not stay.** Whether to make sound is a property of the
  speaker, and the companion owns the speaker. `voice/speech.ts` is 246
  lines that re-derive the paragraph `response.ts:320` already has,
  wrapped around an off/on/once switch kept in the session and seeded
  from an environment variable on a process that makes no sound.

So `SPOKEN_EVENT` carries both halves from one place, `response.ts:320`,
and the companion decides. A companion with no synthesiser is silent
already (`native/scufris-desktop/src/speech.rs`), so "off" needs no wire
field: it is a tray toggle over a speaker that is already there.

Deleted by this: `voice/speech.ts`, the `/speech` command, the
`scufris-speech-state-v1` session entry, `SCUFRIS_SPEECH`,
`SCUFRIS_VOICE_AVAILABLE` and the dynamic import it gates,
`tests/speech.test.ts`, the `voice.enable` line in `nix/home-manager.nix`
that sets the variable on the service, and the two assertions in
`nix/checks/service.nix` that pin it. `voice.enable` then means one
thing: the companion gets a synthesiser.

`voice/` is a misnomer once this lands. It is the response extension.

## What stays, and why (2026-08-27)

Asked directly whether `service/` and `widgets/` are needed:

- **`service/` stays.** It is the agent-role socket client and it is the
  output of increment 2, not legacy. Something must run inside Pi to read
  Pi's events, report what was said, and carry widget commands. By D2
  that something is TypeScript. 850 lines across the client, the
  protocol, and the wiring.
- **`widgets/` stays and is already right.** It defines the three tools
  and hands each command to the companion's widget runtime over the
  wire. It manages nothing itself. The one tangle is that it imports the
  control event and the catalog type from `service/`, which is fine but
  reads backwards; `shared/` is where they belong.
- **`calm.ts` is on notice.** It filters the Pi TUI, and in this
  architecture the only TUI left is `scufris-ctl debug`. It survives on
  that alone.
- **`workflow/orchestration.ts` is the real legacy mass**, 1364 lines of
  delegated job loop. It is not this task's: `20260826-183008` (p75,
  "Scufris does what is asked, not a workflow") owns it and lands
  independently, exactly as L4 says.

## Increment 1 done (2026-08-27)

Landed on `master`. The service starts, the ctl talks to it, and `debug`
hands a terminal the session and takes it back.

Deviations from the design page, both deliberate:

- **Two roles, not three.** `frontend` and `control`. The `agent` role has
  no behaviour until the extension becomes a client in increment 2, and
  the project forbids empty placeholders. Adding it is a one-line change
  to the enum when there is something for it to do.
- **`Starting` added to `ScufrisState`, `Attention` deferred.** The agent
  is spawned before it has answered anything, and a frontend that showed
  `idle` for that window would be wrong. `Attention` waits until dialog
  routing exists, which is increment 2; today the service answers
  extension UI requests itself.

### What was built

- `native/` is the workspace, renamed from `desktop/`. `nix/desktop.nix`
  now builds `-p scufris-desktop` only.
- `native/scufris-control/src/service.rs`: protocol v3. `read_line` split
  out of `read_message` so the version is checked before the body shape,
  and a v2 peer is told which version it spoke.
- `native/scufris-service/`: `config`, `rpc`, `agent`, `service`,
  `server`, `logging`, `main`, and `bin/scufris-ctl.rs` moved here from
  `scufris-control`. Arguments are parsed with `clap`.
- `nix/service.nix` splits one build into the `scufris-service` and
  `scufris-ctl` packages; neither pulls GTK or WebKitGTK.
  `programs.scufris.service.enable` gives it a systemd user unit wanted by
  `default.target`, off by default.
- `docs/src/dev/service.md` is the chapter.

### Verification

- `cargo test -p scufris-service -p scufris-control`: 35 + 25 passed.
  The end-to-end one is
  `server::tests::a_debug_lease_hands_the_session_over_and_closing_gives_it_back`,
  which drives a `/bin/sh` stand-in agent through the real socket: hello,
  `debug`, assert the command line, close the connection, assert the agent
  starts a second time.
- `nix flake check`.
- By hand, against the real `scufris` launcher: the service comes up on
  `idle`, `send` submits and the state cycles `idle -> working -> idle`,
  `watch` follows it, `debug` from a pipe is refused with exit 2, and a
  raw control client gets back the exact session file the agent was on,
  is refused a second lease with `debug_held`, is refused a submission
  with `detached`, and sees `idle` again a moment after it closes. `SIGTERM`
  stops the agent and removes the socket, leaving `desktop.sock` alone.

### Found while testing: the assistant says nothing on the transcript

Scufris does not answer with an assistant text block. Its spoken answer is
the `spoken` argument of a `scufris_final_response` tool call, which the
`desktop` extension forwards today. So the service's transcript ring gets
the user's line and nothing back, and `watch` shows only what was said to
it.

Left as it is. The service reads text blocks, which is right for any
agent, and teaching it one extension's tool name is work increment 2
deletes: there the extension is a client in the `agent` role and pushes
the spoken response itself.

One test needed a bounded wait, and the reason is worth keeping: another
test spawns children, and a forked child holds a copy of every open
descriptor until it execs. So a just-closed listener still answers for a
few microseconds, and `a_stale_socket_is_replaced_and_a_live_one_is_not`
saw it on a loaded builder. Measured at 6 in 4000 locally.

### Testing it by hand

```bash
cd native
nix develop --offline -c cargo build -p scufris-service
SCUFRIS_SERVICE_AGENT="$(command -v scufris)" ./target/debug/scufris-service

# another terminal
./target/debug/scufris-ctl state          # starting, then idle
./target/debug/scufris-ctl watch          # follows state and conversation
./target/debug/scufris-ctl send hello     # prompt, or a steer while working
./target/debug/scufris-ctl abort
./target/debug/scufris-ctl debug          # the session opens here; leaving
                                          # gives the agent back
```

## Increment 2 done (2026-08-27)

Landed on `master`. The inversion is complete: `scufris-service` is the
only server, and the Pi agent, the desktop companion, and `scufris-ctl`
are all clients of it.

What moved:

- **The extension became a client.** `extensions/scufris/service/`
  replaces `extensions/scufris/desktop/`. It connects in the `agent`
  role and reports `said`, `speak`, and widget commands. It reads no
  socket of its own and serves nothing.
- **The frontend became a client.** `native/scufris-desktop/src/link.rs`
  replaces `daemon.rs`. It connects out in the `frontend` role with a
  bounded backoff and no ping thread.
- **Piper moved into the frontend.** `native/scufris-desktop/src/speech.rs`
  runs `SCUFRIS_DESKTOP_SPEAK_COMMAND`, which `nix/speak.nix` builds with
  the pinned model bound inside it. No synthesiser and no player are in
  either launcher closure any more, which `nix/checks/voice.nix` asserts
  against all three.
- **Speaking is a companion overlay.** `ScufrisState` has no `Speaking`.
  `Companion.speaking` sits beside the service's word and
  `shown_assistant()` composes them.
- **Uncertainty is companion-local.** The wire has no `uncertain` answer
  and no `force`. `Event::SubmissionUncertain` is raised only by the
  companion's own 15 s `ACK_TIMEOUT` and by `restore()` of a recovered
  transcript, and the two-Enter confirmation never crosses the socket.

Deleted: protocol v2 (`native/control-protocol-v2.json`, the v2 module,
`extensions/scufris/desktop/`), `SCUFRIS_DAEMON`,
`tools/desktop/scufris-socket-lock`, `nix/popup.nix`, the whole
`programs.scufris.voice.popup` option block and its unit, and
`tests/desktop.test.ts`.

### Deviation: `desktop.sock`, `command.rs` and `keys.rs` stay

The increment text listed them for deletion. They are not part of
protocol v2. `command.rs` serves the companion's own `desktop.sock` on
`scufris_control::command`, which is a different socket with a different
protocol, and it is how a window manager binding reaches the pill through
`scufris-ctl open`. Deleting it now would take activation-by-binding away
before increment 3 replaces it. Only `accept` and `cancel` retire, with
the review state, in increment 3.

### Verification

```
TMPDIR=/tmp npm run check          # tsc clean, 73 tests pass, prettier clean
cd native && cargo test --workspace # 22 + 236 + 42 pass
cargo clippy --workspace --all-targets  # clean
cargo fmt --all --check            # clean
nix flake check --offline          # all 36 checks passed
```

## Increment 3 done (2026-08-27)

Landed on `master`. The pill is an indicator that never takes the
keyboard, and the textbox is the one focused window a voice submission
passes through.

What changed:

- **The review state became a phase called `Editing`, and the review
  window became the textbox.** `src/review.rs` -> `src/textbox.rs`,
  `ui/review.{ts,css,html}` -> `ui/textbox.{ts,css,html}`. The window is
  built focusable, claims the keyboard on every raise, and holds a real
  `<textarea>`. The fake caret, the selection bands, the measuring probe
  and the `scufris://draft` mirror are all gone: the caret, the
  selection, and every editing key are the browser's own.
- **The pill lost its keys.** `ui/index.html` has no hidden `<input>`,
  `ui/pill.ts` has no keydown handler, no mousedown guard and no
  `scufris://accept`. `pill::focused` became `pill::holds_the_keyboard`,
  which is what `nobody_has_the_keyboard` reads: the pill is the dead end
  the window manager hands focus to when a window takes the keyboard and
  goes away.
- **`Posture` has four states, not three.** `Editing`, `Watched`,
  `Passive`, `Off`. `Watched` is the microphone's: the phase waits for
  the pill because the person has to be able to read that the microphone
  is open, and it wants no keyboard. That split is what keeps the privacy
  guarantee now that listening is not a focused phase.
- **`keys.rs` is one accelerator.** The i3 binding mode, the
  `SCUFRIS_DESKTOP_MODE_COMMAND` hook and the mode-enter/mode-leave dance
  are deleted, with `programs.scufris.desktop.modeCommand`. What is left
  is `Super+Escape`, built from the hotkey's own modifiers and grabbed
  only while the pill is on screen.
- **`(Listening, Enter)` is deleted.** One take is one key: `Super+D`
  starts it and `Super+D` stops it. The max-recording ticker sends
  `Event::Activate` rather than sending by timer.
- **`scufris-ctl accept` and `cancel` retire.** `Verb` is one variant,
  `Open`, and a caller that still types the old words is told they are
  not verbs.

### A finding, fixed while adapting the tests

`raise()` recorded `Screen::Ready` whenever the textbox answered `Ready`,
even when the pill under it had not been proved up. `Ready` is about both
windows, so a raise over an unproved pill now stays short of it and the
repair chain asks for the pill again. The test that pins it is
`a_restore_nothing_can_confirm_keeps_the_words_on_screen`.

### Verification

```
TMPDIR=/tmp npm run check           # tsc clean, 74 tests pass, prettier clean
cd native && cargo test --workspace # 22 + 234 + 42 pass
cargo clippy --workspace --all-targets  # clean
cargo fmt --all --check             # clean
nix flake check --offline           # all checks passed
```

Not verified by hand: the companion needs a display, and the keyboard
contract is the half no headless test reaches. What to try on the
machine - `Super+D`, `Super+D`, type into the box, `Enter`; then a second
turn, because a window manager reads the focus hints when it maps a
window and the second map is where the old review box lost the keyboard.

## Increment 5 done (2026-08-27)

`voice/speech.ts` is gone and the extension tree is one file flatter.

What changed:

- `voice/response.ts` became `extensions/scufris/response.ts` and `voice/`
  was deleted. The directory was named for a capability the code inside it
  does not have; what is left shapes the answer and makes no sound.
- Both halves of `SPOKEN_EVENT` are emitted where the paragraph is decided.
  `appendResponse` sends `said` and `speak` together, because the tool path's
  answer is a tool argument nothing outside the extension can read. The direct
  path sends only `speak`, because its rewritten message is already visible to
  the service through Pi's own events. `speech.ts` used to re-derive the same
  paragraph from the session branch at `agent_settled`; there was never a
  second decision to make.
- The mute moved to the companion, as `Speaker::mute` and "Mute Scufris" in
  the tray. Muting cuts what is playing; unmuting restores nothing, because
  the next answer is what a person wants to hear. Nothing about it crosses the
  wire and the agent is neither asked nor told.
- Deleted with it: the `/speech` command and its `replay`, the
  `scufris-speech-state-v1` session entry, `SCUFRIS_SPEECH`,
  `SCUFRIS_VOICE_AVAILABLE`, `tests/speech.test.ts`, the `SCUFRIS_SPEECH=1`
  line on the service unit, and the two assertions pinning it.
- The build variants went with them, which was the surprise of this increment.
  `nix/resources.nix` had a `voice` parameter and `nix/scufris.nix` built a
  second launcher, purely to ship `speech.ts` and set one variable. Gone:
  `voiceResources`, `voiceLauncher`, the `voice-resources` package, the
  `scufris-voice` package and app, `npm run dev:voice`, and
  `scufris-dev --voice`. `nix/checks/launcher.nix` lost its second half.
  `scufris-dev` also still listed `extensions/scufris/desktop/index.ts`, which
  increment 2 deleted, and had no `service/index.ts`; the working-tree
  launcher has been broken since then and now is not.
- `voice.enable` means one thing: the companion gets a synthesiser.
- Documentation: the extensions chapter's "voice" section is "response" and
  says what the paragraph is for; the operation, architecture, desktop, using
  and installation chapters lost the variables, the command, and the packages.

### Verification

- `npm run check` - pass. 67 tests, down from 75 with `speech.test.ts` gone.
- `cargo test --workspace` - pass, 234 in the companion including the new
  `a_muted_speaker_cuts_what_is_playing_and_takes_nothing_new`.
- `cargo clippy --workspace --all-targets -- -D warnings` - clean.
- `cargo fmt --all --check`, `shellcheck scripts/scufris-dev`, `prettier` -
  clean.
- `nix flake check --offline` - pass.

Not verified on a display. The tray mute is wired the way the sound cue
switch is and its behaviour is covered in `speech.rs`, but nobody has clicked
it.

### Found while landing this

`widgets::backends::tests::two_widgets_asking_the_same_question_share_one_process`
fails intermittently in the Nix sandbox and passes on the developer machine.
It is unrelated to this increment, which touches nothing in the widget
runtime, and it is a real gap rather than only a slow test: a panel that
subscribes to an already-running shared backend is never handed the last
reading. Filed as `20260827-142259` (p80).

## The silent agent (2026-08-27)

An afternoon went to this, so the service now says it.

The service reads Pi's own RPC event stream for the state and the transcript.
An agent carrying none of the Scufris extensions therefore holds a working
conversation: it answers, the transcript fills, `send` works. What it cannot do
is everything only the agent reports - what it said, the paragraph to speak,
and every widget command - and none of that fails loudly. The symptom is a pill
that goes straight to idle and a speaker that never makes a sound, which is
indistinguishable from a broken synthesiser.

The cause here was an older `scufris` earlier on `PATH`, built before the
inversion, with no `service/index.ts` in its extension list.

`Service::start_agent` now clears `agent_joined` and starts a watcher thread.
Ten seconds later `report_a_silent_agent` warns if that generation's agent is
still running and has never registered in the `agent` role, and names the
binary. Ten seconds because this is a Node process loading extensions, and the
cost of saying it too early is a warning about nothing.

`agent_is_silent(generation)` is the predicate, split out so a test does not
have to wait for the grace. The generation is what makes a late watcher
harmless: an agent that has already been replaced is not the one anybody is
waiting on.

### Verification

- `cargo test -p scufris-service` - pass, 43, including the new
  `an_agent_that_never_connects_back_is_noticed`. It starts a real `/bin/sh`
  agent that reads its stdin, so the running-agent case is reachable, and
  asserts both directions plus the stale generation.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -D
warnings`, `cargo fmt --all` - clean.
- `nix flake check --offline` - pass.

## Increment 4 done (2026-08-27)

Listening was already one rule by the time this increment started. Two of the
three parts landed with the increments before it:

- **Steer**, in increment 1. `Service::submit` sends `streaming_behavior:
"steer"` when the state is `Working` and nothing otherwise, so what you say
  is a prompt to an idle agent and a steer to a working one.
- **Barge-in**, in increment 3. `DesktopSurface::present` cuts the speaker on
  every presentation that says the microphone is open. One rule rather than
  one transition, and it says something stronger than "the key cuts speech":
  nothing is spoken for as long as a take is running.

What this increment adds is the third: a key for abort.

### The stop key

`Super+Period`, built from the activation hotkey's own modifiers the way
`Super+Escape` is, and grabbed on the same terms - only while the pill is on
screen, because an accelerator held all session is one no other program can
use. `keys.rs` now holds two accelerators and one grabber thread for both, and
a key a window manager already owns is skipped without losing the other.

It is its own key rather than a second meaning for Escape. Escape puts a pill
away and throws away a take; neither reaches the conversation. Stop ends a run
that may be part way through changing something, and a gesture with that much
behind it should not be reachable by pressing the dismiss key at the wrong
moment.

What it does, in two halves that never meet on the wire:

- The speaker is the companion's own, so `main.rs` cuts it in the key handler.
- The run is the service's, so `Event::Stop` goes to the state machine, which
  answers with `Action::Abort { id }` when the assistant is `Working` and with
  nothing otherwise. `App::abort` writes `ClientBody::Abort` and waits for
  nothing.

Stop belongs to the assistant rather than to the phase, so the phase is left
exactly where it was: a transcript being edited while a run is stopped is still
a transcript in the textbox. The pill goes on reporting `working` until the
service says the run ended, because the service is what knows.

A stop that cannot be sent is a line in the log and nothing else. There is
nothing to retain and nothing that could be sent twice, and the pill still
saying `working` is the truth.

Identifiers now come from one counter for every command rather than one for
submissions, so an abort can never carry a live submission's identifier and no
answer can be read as the answer to something else.

### Verification

- `cargo test --workspace` - pass, 240 in the companion and 43 in the service.
  New: `stopping_a_run_ends_it_and_leaves_the_words_alone`,
  `stopping_a_settled_assistant_asks_the_service_for_nothing`,
  `the_stop_key_ends_the_run_and_leaves_the_pill_reporting`,
  `the_stop_key_says_nothing_to_a_settled_assistant`,
  `stopping_scufris_is_not_the_key_that_puts_the_pill_away`.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all`,
  `prettier` - clean.
- `nix flake check --offline` - pass.

Not verified on a display. `Super+Period` parses as an accelerator and the
grab is arranged the way the cancel key's is, but nobody has pressed it.

### Fixed on the way: a real flake in the speech tests

`a_muted_speaker_cuts_what_is_playing_and_takes_nothing_new` failed about one
run in three. The cause is `ETXTBSY`: both speech tests wrote a `/bin/sh`
script and then executed it, and a file this process has open for writing is a
file it cannot execute. One thread writing while another starts a child is all
that takes, and `cargo test` runs them on threads beside each other.

Confirmed rather than guessed: the spawn error was printed in a loop until it
failed, and it is `ExecutableFileBusy` (os error 26).

The stand-in programs are now checked in - `tests/fixtures/speaker` in the
companion and `tests/fixtures/agent` in the service - so nothing writes them at
run time and the race cannot happen. The service test written in the increment
before this one had the same shape and now uses the fixture too.

`widgets::backends::tests::two_widgets_asking_the_same_question_share_one_process`
still fails about one run in four. That one is not this: it is
`20260827-142259` (p80), a late subscriber to a running shared backend never
being handed the last reading.

## Widgets: bigger panels and useful ones (2026-08-27)

Asked for with increment 4: keep the style, change the content. The gruber
palette, the square high-contrast frame, the pill and the textbox are
untouched. What changed is how large a panel is and what is on it.

### Bigger

`--sw-size-big` 26 -> 34, `--sw-size-body` 11.5 -> 13.5, `--sw-size-small`
9.5 -> 11, and every panel grew with them. Type that grew inside a window that
did not would only have less room to say the same thing.

The padding grew for a second reason. `--sw-pad-top` and `--sw-pad-bottom` are
now 34px, the height of a tick plus the gap it sits in. The chrome draws over
the widget rather than beside it, and at the old 24px bottom the `pin` tick
sat on top of the CPU widget's own last row. A strip one pixel short is a row
of a widget's own with a tick through it.

### `cpu`

The graph stays and takes whatever height the panel leaves it. Beside the
headline is the package temperature, which turns `--sw-warn` at 85C; under the
graph are memory in use and the one-minute load average.

The `system` backend finds its thermometer once at start: hwmon by chip name
(`k10temp`, `zenpower`, `coretemp`), then the input whose label is the package
rather than one core, then the `x86_pkg_temp` thermal zone. A machine with
none reports `null` and the panel shows a dash, which is not the same as zero
degrees.

### `claude` and `codex`

Every usage window is a meter; the one closest to its limit is the headline,
with how long until it starts over. Accent under 75 percent, `--sw-warn` at
75, `--sw-alarm` at 90.

They read the token the vendor's own CLI already keeps on this machine, and
read it again on every poll rather than holding it, because the CLI refreshes
that file in place. Nothing to sign in to and nothing to configure. A machine
that never signed in gets "not signed in" rather than a stale number.

Claude's usage is `GET https://api.anthropic.com/api/oauth/usage` with the
OAuth bearer and `anthropic-beta: oauth-2025-04-20`. Its `limits` array is
read rather than the named fields beside it: the named ones come and go with
whatever is offered that month, and every one of them appears in `limits` too.
A scoped weekly limit is labelled with the model it scopes, because "weekly"
twice says nothing about which is about to bite.

Codex's is `GET https://chatgpt.com/backend-api/codex/usage` with the bearer
and `chatgpt-account-id`. It refuses the interpreter's default user agent with
a 403 whatever the token is, so the backend sends one of its own that says
what it is rather than dressing up as the CLI. Its two windows are sorted
shortest first, because which of `primary` and `secondary` is the short one
depends on the plan.

The answer carries the account behind it, an email address among it. Three
fields leave the backend - the window's label, its percentage, and the seconds
until it resets - and a unit test asserts that a window carries nothing else.

Polling is once a minute, clamped to [15s, 3600s], with a `refresh` action
behind the panel's `rfr` tick for the moment after a long run. The first
reading goes out at once rather than after an interval: a percentage is whole
on its own.

### Gone

`note` and `tasks`, and the `tasks` backend with them. Nothing migrated. The
tray now offers every shipped widget, because every shipped widget fills
itself.

### Verification

- `cargo test -p scufris-desktop` - 241 pass.
- `python3 -m unittest discover -s tests` - 45 pass, 12 of them new in
  `tests/test_usage_backends.py`. Nothing there reaches the network: the two
  tests that call `reading()` point the backend at an empty directory.
- Both backends run live on this machine and answer with the shape above.
- `npm run check` and `nix flake check --offline` - pass.
- The four panels rendered in the real shell markup with the real stylesheets
  and screenshotted, which is how the tick collision was found. Not seen on
  the i3 desktop yet.

### Note

`tests/*.py` are not run by any check. They pass, and this adds to them, but
nothing in CI or `nix flake check` calls them. Worth wiring up separately.

## Increment 6 done: the conversation window (2026-08-27)

The last surface. `scufris-ctl hud` puts it up and puts it away; the tray
shows it on a left click and from a new "Show conversation" entry. It draws
the transcript stream every frontend is pushed and types back on the same
socket. 760 by 560, centered, gruber, square, Iosevka - the widget grammar.

### Decisions

- **D-HUD-1 A sibling, not a phase.** `conversation.rs` is its own state
  machine beside `state.rs` rather than a posture inside it. The pill machine
  is about one take and ends with it; the conversation outlives every take.
  Typing here never raises the textbox, never ends a recording, and never
  moves the pill off what it is showing.
- **D-HUD-2 Two senders, one verb.** The textbox sends a transcription and
  the window sends a typed line; both are a `submit`. They share the process
  prefix and not the counter - identifiers are what the service suppresses
  duplicates by - so the window's are `{prefix}-h{n}`.
- **D-HUD-3 No durable copy.** The pill persists an accepted transcript
  because spoken words cannot be typed again. A typed line is still in the
  field until the service takes it, so `pending.rs` stays the textbox's alone.
  Nothing is appended locally either: the line comes back as a transcript
  entry, so the window shows the conversation rather than its own hopes.
- **D-HUD-4 No accelerator. The command socket instead.** The reversal of the
  plan, and the better answer. `keys.rs` grabs only while the pill is up
  because an accelerator held all session is a key no other program can use;
  a third permanent grab would have broken that rule for a window. So
  `Verb::Hud` joins `Verb::Open` on `desktop.sock` and the person binds it in
  their own window manager, which costs the desktop nothing. `desktop.sock`
  is now two verbs and both are windows.
- **D-HUD-5 A copy of the service's ring, and nothing longer.** 200 lines,
  the same bound. A frontend that connects is replayed the whole ring, so the
  window empties itself on `Connected` and lets the replay refill it; keeping
  what it had would show every line twice. `get_entries` for deeper history
  stays deferred - the ring is what the service has, and a window that showed
  more would be showing something nothing can refill after a reconnection.
- **D-HUD-6 Not on top, and focusable.** The pill and the textbox are
  indicators that must be seen over what is under them. This is a window the
  person works in, and one they have moved away from belongs behind what they
  moved to. Toggling it is cheaper than fighting it for the screen. Built
  unfocusable and hidden, raised with `set_focusable` before `show` - the
  ordering `textbox.rs` documents, because i3 reads the hints at map time.
- **D-HUD-7 Text only.** `TranscriptEntry` and nothing else. No markdown, no
  tool calls, no thinking. The session file and `scufris-ctl debug` are where
  the rest of a run lives.
- **D-HUD-8 The terminal is not a fallback.** "Open chat" became "Open in
  terminal" and stays, under the new entry rather than replaced by it.
  `scufris-ctl debug` is a whole Pi session and this window will not be one
  for a long time. Two tools.
- **D-HUD-9 One line in flight.** A second Enter on an unanswered line would
  put two questions in the conversation from one intention, and the words are
  in the field either way. A disconnection gives up on what is in flight: no
  reconnection brings that answer, and a window that kept waiting would refuse
  every line after it for the session.

### Files

`src/conversation.rs` (pure state, 13 tests), `src/hud.rs` (window and
wiring), `ui/hud.{html,css,ts}`, `Verb::Hud` in `scufris-control`, the
`scufris-ctl hud` subcommand, `tray::MENU_HUD`, `"hud"` in
`capabilities/default.json`, and the routing in `main.rs`:
`Transcript` to the window, `Accepted`/`Refused`/`Disconnected` to both
senders, `Connected` to both.

An `open` flag on `Conversation` was written and then deleted: nothing read
it. `hud::up` asks the toolkit, which is the truth. An i3 kill on the window
is answered with `prevent_close` and a hide, so the window is never destroyed
and never has to be rebuilt and refilled.

### Verification

- `cargo test --workspace` - 323 pass, 258 of them `scufris-desktop`, 12 of
  them new in `conversation.rs` and `hud.rs`.
- `cargo clippy --workspace --all-targets` - clean.
- `npm run check` - pass, including `prettier --check .`.
- `tsc -p ui/tsconfig.json --noEmit` - clean.
- `nix flake check --offline` - pass.
- The page rendered headlessly at 760x560 in four states - short, long,
  overflowing, empty and disconnected - and screenshotted. That is what
  caught the conversation floating at the top of an empty window; it hangs at
  the bottom now, on `margin-top: auto` on the first line rather than
  `justify-content`, which would push the overflow out of reach. Not seen on
  the i3 desktop yet.

## Increment 7 done: the click, and the keys are the deployment's (2026-08-27)

Two things asked for after increment 6 went up: a click on the pill should
show the conversation, and the keybinds should be configurable.

### The click

`pill.ts` listens for a left click on `.pill` and invokes `hud_toggle`, which
is `Hud::toggle` - the same call `Verb::Hud` and the tray make. Three ways in
and all three toggle.

- **D-HUD-10 Pointer input is not focus.** The pill is built unfocusable and
  stays that way. An unfocusable window still receives pointer events, which
  is already how the widget panels take their chrome ticks, so nothing about
  the click touches the keyboard or the raise ordering.
- The cursor becomes a pointer. The orb carries no label and never will, so
  the cursor is the only way the window can say the click does something.
- Left button only. The right button belongs to the window manager's menu.

### The keys

`cancelKey` and `stopKey` in the module, `SCUFRIS_DESKTOP_CANCEL_KEY` and
`SCUFRIS_DESKTOP_STOP_KEY` in the environment.

- **D-KEY-1 Deriving is the default, not the rule.** Unset still means
  `Super+Escape` and `Super+Period` beside a `Super+D` hotkey: one modifier to
  remember is the right thing to ship. A desktop that already means something
  by `Super+Escape` is a reason to move the key, not to lose it.
- **D-KEY-2 `"none"` turns a key off.** A real answer and not a missing one,
  which is why `--print-config` says `derived` for unset rather than reusing
  `none`. The tray puts the pill away without the cancel key and
  `scufris-ctl abort` stops a run without the stop key, so neither is load
  bearing.
- **D-KEY-3 A bad accelerator leaves no key.** Warned about and dropped rather
  than quietly derived. A working key on an accelerator the person did not ask
  for is harder to notice than a key that does nothing and says why in the log.
- **D-KEY-4 No `hudKey`.** The conversation window still has no accelerator of
  its own, for the reason D-HUD-4 gives. `scufris-ctl hud` in a window manager
  binding is already maximally configurable, and it costs the desktop nothing.

The two keys stay grabbed only while the pill is on screen whichever
accelerator they are on. Configurability does not change what a permanent grab
costs.

`config.rs` takes them as a `Keys` struct beside `Hooks` rather than three more
arguments; an empty environment variable is a key nobody named, because the
unit file writes what the person left blank.

### Verification

- `cargo test --workspace` - 328 pass. New: `config.rs`
  `the_keys_beside_the_hotkey_are_reported_as_the_deployment_named_them` and
  `a_key_set_to_nothing_is_a_key_that_was_not_named`; `keys.rs`
  `a_key_the_deployment_named_is_the_key_that_is_grabbed`,
  `a_key_turned_off_is_grabbed_by_nothing`, and
  `an_accelerator_that_will_not_parse_leaves_no_key_rather_than_the_default`.
- `nix flake check --offline` - pass. `desktop-configuration` diffs both
  `--print-config` blocks, and `desktop-interface` asserts the module writes
  neither variable when neither key is named and writes both when they are.
- `prettier --check`, `tsc --noEmit`, `cargo fmt --all` - clean.

Neither the click nor a named key has been pressed on the i3 desktop. This
needs a home-manager rebuild first.

## Increment 8 done: a key that is free, and a window Scufris can open (2026-08-27)

Two things, both from using increment 7 on the real desktop.

### The stop key moved

`Super+Period` is rofi-emoji on this machine, and that is not a local
accident: `Super+.` is the Windows emoji picker, and the rofi and Hyprland
desktops that copied it mean the same thing by it. The premise the key was
chosen on - "it belongs to nothing on the desktop" - was simply false.

The default is `Super+Delete` now. Checked rather than assumed: `Delete` parses
as an accelerator and `Del` does not, so the constant is spelled out. The
scratch test that established this was deleted with the answer written into the
constant's doc comment.

Nothing was migrated, because nothing had shipped on `Period`: the key was
added in this same unreleased cycle.

### The conversation window, from the agent

`scufris_conversation`, in a new one-file extension `extensions/scufris/
conversation.ts`.

- **D-CONV-1 Show and close, never toggle.** The user asked for a toggle. A
  toggle from a caller that cannot see the screen does one of two opposite
  things and cannot tell which, so Scufris would say "I have opened the
  conversation" having just hidden it. The person's own three gestures still
  toggle, because the person can see the screen. Asking for what is already
  there is harmless, which is what makes the explicit verb the cheap one.
- **D-CONV-2 Its own verb, not a widget.** `ClientBody::Conversation { id, up }`
  beside `ClientBody::Widget`. The window is the frontend's own, built in
  rather than installed, and carries no payload; the only thing it shares with
  a widget is that the agent is the one asking. Reusing `WidgetCommand` would
  have bought a `surface` identifier and a close report it has no use for.
- **D-CONV-3 The service answers, not the frontend.** Every widget command is
  answered by the frontend, which is right: an open produces a surface and a
  widget name can be wrong. Nothing here can half happen. The one failure the
  agent can act on is that there is no screen, and the service is what knows
  that, so it answers `ok` or `refused { no_frontend }` on relay. That also
  makes the agent read `ok` and `refused` for the first time; before this
  every verb it sent was one-way or answered by a report.
- **D-CONV-4 `WidgetControl` became `DesktopControl`.** The control the service
  extension hands out is not widgets any more. Renamed with the event string
  (`scufris:desktop-control`) rather than growing a `conversation()` method on
  something called `WidgetControl`.

### Fixed on the way: a widget report could settle a conversation command

Caught by the test written for it, not in review. The first version put
conversation commands in `pendingWidgets`, keyed by id. The two counters are
independent, so `w-3` and `c-3` can be in flight at once, and a
`report { done, id: "c-3" }` resolved the conversation command. Split into
`pendingAnswers`; `abandon` clears both.

### Verification

- `cargo test --workspace` - 331 pass. New in the service:
  `the_conversation_window_is_relayed_and_answered_by_the_service`,
  `asking_for_the_conversation_window_with_no_screen_is_refused`,
  `a_control_client_cannot_ask_for_the_conversation_window`.
- `npm run check` - 72 pass, 5 of them new: 4 in `tests/conversation.test.ts`
  and the client round trip in `tests/service.test.ts`, which is the one that
  caught the shared-map bug.
- `cargo clippy --workspace --all-targets`, `cargo fmt --all`, `prettier` -
  clean.
- `nix flake check --offline` - pass, including the launcher check that pins
  the exact `--extension` list.

Neither the new key nor the tool has been exercised on the i3 desktop. This
needs a home-manager rebuild first.

## Closed (2026-08-27)

Eight increments, then one review round over the whole range
(`185034a..a13cb38`, recorded in `REVIEW.md`). Everything the panel
raised was queued as its own task and every one of them has landed:

- `20260827-205332` bound every string the service emits.
- `20260827-205335` finished the conversation window: focus, stacking,
  tests. The textbox draws on top and the HUD sits above it.
- `20260827-205337` settled the submission identifier and its timer.
- `20260827-205340` routed or retired what the inversion left behind.
- `20260827-205342` refreshed the review lane briefs.
- `20260827-205346` brought the documentation up to the inverted tree.
- `20260827-205350` took the eight minor findings.

Two things the review surfaced were deliberately not folded back in
here, because they are new work rather than repair:

- `20260827-212938`, an ambient signal for an unattended job. The
  removed `workerAttentionSignal` answers a different question from
  `ScufrisState` and needs a channel of its own.
- `20260827-212118`, the staging stack, which the parallel-instance
  finding raised the priority of.

The service is what runs Scufris now: `pi --mode rpc` is a client of it,
the companion is a client of it, and `scufris-ctl` is how a terminal or
a window manager reaches it.
