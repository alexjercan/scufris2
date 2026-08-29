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

Scufris packages six capability-owned Pi extensions:

- `workflow` is the core engine. It owns the Scufris identity, the project
  agent menu, delegated job spawn and control, worker events, review, and
  landing.
- `response` owns response shaping, and decides which paragraph of an answer is
  worth saying aloud. Saying it belongs to the companion.
- `calm` reduces transcript and working-state clutter.
- `service` connects the foreground agent to `scufris-service` as its one
  `agent` client, and carries the answer, the spoken paragraph, and the widget
  requests.
- `widgets` opens small panels on the desktop while Scufris answers. The tools
  are registered from the catalog the companion announces, so the widgets the
  model can name are the ones that are installed. See [Widgets](dev/widgets.md).
- `conversation` puts the companion's own conversation window up and down. It
  is the one window Scufris can ask for that is not a widget.

`scufris-service` is the half that owns the conversation. Its crate lives under
`host/service/` in the root cargo workspace and ships as its own package with no
graphical dependency at all. It supervises one `pi --mode rpc` agent, owns the session
directory, and serves the socket every surface connects to. `scufris-ctl` talks
to it from a terminal. See [Background service](dev/service.md).

`scufris-desktop` is the desktop companion: a voice pill, a conversation
window, a widget runtime, and a tray icon, built from the same workspace and
shipped as a separate Linux package. It is a client of the service. It records,
transcribes locally, submits the words as an ordinary user message, draws what
was said, and speaks whatever paragraph the agent asks it to. The conversation
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
