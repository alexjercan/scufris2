# Scufris as a service: the architecture inversion

- STATUS: IN_PROGRESS
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
5. The HUD, when we want it.

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
