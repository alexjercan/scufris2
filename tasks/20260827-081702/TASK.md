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

- `scufris-ctl detach` stops the agent child and frees the session
  directory, so an ordinary `pi --continue` in any terminal takes over
  with the full TUI. `attach` restarts the child. The service reports
  `detached` as a state. No unit and no popup required.
- Pi appends the session as JSONL and publishes the path as
  `PI_SESSION_FILE`, so `tail -f` is a raw live transcript with no second
  writer.

## Increments

1. The service, headless. Rename the workspace to `native/`. Rust crate
   supervising one `pi --mode rpc`, owning the session directory, mapping
   events to `ScufrisState`, holding the transcript ring, answering
   extension UI requests, serving v3 with three roles. `scufris-ctl send
| state | abort | detach | attach`. Nothing graphical changes.
2. The switch, one commit. Extension becomes a client, frontend becomes a
   client, piper moves into the frontend, and `desktop.sock`,
   `command.rs`, `keys.rs`, the socket lock, `SCUFRIS_DAEMON`, protocol
   v2, the popup unit and its options are deleted.
3. The textbox. Review state and Enter-while-recording deleted with it.
4. Listening is one rule: barge-in, steer, and a key for abort.
5. The HUD, when we want it.
