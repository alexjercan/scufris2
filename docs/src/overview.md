# Overview

Scufris is a personal assistant that runs in the Pi conversation harness. It
adds a focused foreground identity, a project workflow engine with delegated
worker jobs, Calm transcript presentation, a background service that owns the
conversation, and an optional desktop companion with a voice pill and a
conversation window.

Scufris ships as a Nix flake package and a Home Manager module. There is one
launcher; the speech module and the voice build variants that existed to ship
it are gone. Whether Scufris makes a sound is the tray's, and Home Manager
decides whether the companion is handed a speech command at all.

## Components

Scufris packages four capability-owned Pi extensions:

- `workflow` is the core engine. It owns the Scufris identity, the project
  agent menu, delegated job spawn and control, worker events, review, and
  landing.
- `response` emits one atomic plain response with optional Markdown details and
  widget presentation calls.
- `calm` reduces transcript and working-state clutter.
- `service` connects the foreground agent to `agent.sock`, injects
  self-contained registered-surface messages through `pi.sendUserMessage()`,
  and carries atomic responses and attention state.

`scufris-service` is the half that owns the conversation. Its crate lives under
`host/service/` in the root cargo workspace and ships as its own package with no
graphical dependency at all. It supervises one `pi --mode rpc` agent, owns the session
directory, and serves separate surface, agent, and control sockets. `scufris-ctl` talks
to it from a terminal. See [Background service](dev/service.md).

`scufris-desktop` is the desktop companion: a voice pill, a conversation
window, a widget runtime, and a tray icon, built from the same workspace and
shipped as a separate Linux package. It is a client of the service. It records,
transcribes locally, submits the words as an ordinary user message, draws the
bounded canonical conversation, and speaks only its associated live response. The conversation
is the service's, so a companion crash never stops it and a machine with no
screen still has one. See [Desktop companion](dev/desktop.md).

Deterministic executables called by extensions live under `tools/`. Commands
for people live under `scripts/`. Model-facing workflow policy lives in small
skills under `agent/skills/`.

## Responsibilities

- Pi runs the foreground conversation and supplies global configuration,
  including optional speech-to-text input.
- Scufris delegates work expected to take minutes to independent worker jobs
  in tmux sessions. The foreground conversation never blocks on them.
- The desktop configuration owns keybindings and window placement. Scufris
  ships no window manager.

Read the [user guide](guide/installation.md) to install and use Scufris. Read
the [developer guide](dev/architecture.md) for the complete design.
