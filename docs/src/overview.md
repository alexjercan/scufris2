# Overview

Scufris is an assistant that runs in the Pi conversation harness. It adds a focused identity, Calm mode, delegated work, Dashboardd widget control, and optional local speech.

Scufris is available as a Nix flake package and a Home Manager module. The default package has no speech code or speech runtime in its closure. Linux users can select the separate voice-capable package or enable voice through Home Manager.

## Integration responsibilities

Scufris packages its launch composition, extensions, skills, delegated-job helpers, optional local Piper speech, and direct Kitty popup service.

- Pi runs the foreground conversation and supplies global configuration, including optional speech-to-text input.
- Dashboardd supplies the widget service and `dashboardctl`.
- The desktop configuration starts and presents the popup, including geometry, keybindings, and toggle policy.
