# Overview

Scufris is a personal assistant that runs in the Pi conversation harness. It
adds a focused foreground identity, a project workflow engine with delegated
worker jobs, Dashboardd widget control, Calm transcript presentation, and
optional local speech.

Scufris ships as a Nix flake package and a Home Manager module. The default
package contains no speech code or speech runtime in its closure. Linux users
can select the separate voice-capable package or enable voice through Home
Manager.

## Components

Scufris packages four capability-owned Pi extensions:

- `workflow` is the core engine. It owns the Scufris identity, project
  workflow preferences, delegated job spawn and control, worker events,
  review, and landing.
- `voice` owns response shaping in every package and optional Piper speech
  playback in voice-capable packages.
- `calm` reduces transcript and working-state clutter.
- `dashboard` owns Dashboardd widget surfaces and their tools.

Deterministic executables called by extensions live under `tools/`. Commands
for people live under `scripts/`. Model-facing workflow and dashboard policy
lives in small skills under `skills/`.

## Responsibilities

- Pi runs the foreground conversation and supplies global configuration,
  including optional speech-to-text input.
- Scufris delegates work expected to take minutes to independent worker jobs
  in tmux sessions. The foreground conversation never blocks on them.
- Dashboardd is an external service. It supplies the widget catalog and the
  `dashboardctl` command.
- The desktop configuration owns popup placement, keybindings, and toggle
  policy.

Read the [user guide](guide/installation.md) to install and use Scufris. Read
the [developer guide](dev/architecture.md) for the complete design.
