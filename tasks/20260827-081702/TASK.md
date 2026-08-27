# Scufris as a service: the architecture inversion

- STATUS: OPEN
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

## Open decisions, awaiting Alex

- **D1** Does the service spawn the frontend, or is the frontend its own
  systemd unit wanted by `graphical-session.target`? Recommended: its own
  unit. Ownership by protocol, not by process parentage.
- **D2** Rust, TypeScript, or TypeScript embedding `AgentSession`?
  Recommended: Rust, in the existing workspace, driving `pi --mode rpc`.
  Couples to Pi's protocol rather than Pi's internals.
- **D3** Rename `desktop/` to `native/` with three crates? Recommended:
  yes, once, before there are two things to move.
- **D4** Does the Kitty popup survive as a detach target? Recommended:
  yes, and retire it only when the HUD earns it.
- **D5** Does shaped speech travel as a `say` command or as a field on a
  transcript entry? Recommended: a command, because speaking has a
  lifetime and the frontend must be able to refuse it.

## Biggest risk

The Pi TUI has no replacement. RPC mode has no terminal interface, and
the HUD will not match model cycling, slash commands, forking, and
scrollback for a long time. The mitigation is `scufris-ctl detach`: stop
the service's agent child, run an ordinary `pi --continue` against the
same session directory, then `attach`. Never two writers.
