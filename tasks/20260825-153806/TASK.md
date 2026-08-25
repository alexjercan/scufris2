# Wake word activation via openWakeWord service

- STATUS: OPEN
- PRIORITY: 40
- TAGS: voice, desktop, wakeword

## Goal

Voice activation through the start action Super+D already runs. Do not
build this before the hotkey path saturates: wake misses are the top
documented cause of voice abandonment.

## Scope

- Engine: openWakeWord as a separate systemd user service (nixpkgs
  `wyoming-openwakeword`). Rustpotter is the pure-Rust fallback if a
  Python service chafes. Porcupine and Snowboy are disqualified.
- Shape (wyoming-satellite): the wake service owns its own PipeWire
  capture stream and on detection pokes the companion over the control
  channel, which runs the same start action Super+D runs.
- Posture: off by default, explicit toggle, persistent bar privacy
  indicator while enabled, distinct pill state while streaming.
- Nix: a Home Manager option following the whisper-server precedent; the
  default package stays wake-word-free.

## Verification

- Detection opens the pill identically to Super+D.
- The toggle stops the capture stream, and the bar indicator tracks it.
- The default package closure does not contain the wake service.

Decided in `tasks/20260822-132001/RESEARCH.md` section 4.
