# Overview

Scufris is a personal assistant that runs in the Pi conversation harness. It
adds a focused foreground identity, a project workflow engine with delegated
worker jobs, Calm transcript presentation, optional local speech, and an
optional desktop voice pill.

Scufris ships as a Nix flake package and a Home Manager module. The default
package contains no speech code or speech runtime in its closure. Linux users
can select the separate voice-capable package or enable voice through Home
Manager.

## Components

Scufris packages five capability-owned Pi extensions:

- `workflow` is the core engine. It owns the Scufris identity, project
  workflow preferences, delegated job spawn and control, worker events,
  review, and landing.
- `voice` owns response shaping in every package and optional Piper speech
  playback in voice-capable packages.
- `calm` reduces transcript and working-state clutter.
- `desktop` serves the control socket in the popup process and reports one
  assistant state to the desktop companion.
- `widgets` opens small panels on the desktop while Scufris answers. The tools
  are registered from the catalog the companion announces, so the widgets the
  model can name are the ones that are installed. See [Widgets](dev/widgets.md).

`scufris-desktop` is the desktop companion: a voice pill, a widget runtime, and
a tray icon, built from the `native/` cargo workspace and shipped as a separate
Linux package. It records, transcribes locally, and submits the words as an ordinary
user message. The conversation stays in the popup Pi process, so a companion
crash never stops it. See [Desktop companion](dev/desktop.md).

`scufris-service` is the headless half, built from the same workspace and
shipped as its own package with no graphical dependency. It supervises one
`pi --mode rpc` agent, owns the session directory, and serves the socket every
surface connects to. `scufris-ctl` talks to it from a terminal. See
[Background service](dev/service.md).

Deterministic executables called by extensions live under `tools/`. Commands
for people live under `scripts/`. Model-facing workflow policy lives in small
skills under `skills/`.

## Responsibilities

- Pi runs the foreground conversation and supplies global configuration,
  including optional speech-to-text input.
- Scufris delegates work expected to take minutes to independent worker jobs
  in tmux sessions. The foreground conversation never blocks on them.
- The desktop configuration owns popup placement, keybindings, and toggle
  policy.

Read the [user guide](guide/installation.md) to install and use Scufris. Read
the [developer guide](dev/architecture.md) for the complete design.
