# Overview

Scufris is an assistant that runs in the Pi conversation harness. It adds a focused identity and project workflow engine, delegated agents, Calm mode, Dashboardd control, and optional local speech.

Scufris is available as a Nix flake package and a Home Manager module. The default package has no speech code or speech runtime in its closure. Linux users can select the separate voice-capable package or enable voice through Home Manager.

## Integration responsibilities

Scufris packages four capability-owned extensions:

- `workflow` is the core engine. It owns identity, project methodology,
  delegated-agent spawn and control, review, landing, polling, and cleanup.
- `voice` owns response shaping and optional Piper speech playback.
- `calm` reduces transcript and working-state clutter.
- `dashboard` owns Dashboardd surfaces and widget tools.

Extension-called executables live under `tools/` beside their owning capability.
Human and development helpers remain under `scripts/`. Model-facing workflow and
dashboard policy remains in small skills; deterministic mechanics do not.

- Pi runs the foreground conversation and supplies global configuration, including optional speech-to-text input.
- Dashboardd supplies the widget service and `dashboardctl`.
- The desktop configuration starts and presents the popup, including geometry, keybindings, and toggle policy.
