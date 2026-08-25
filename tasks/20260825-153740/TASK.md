# Adopt tracing with journald and foreground logging in scufris-desktop

- STATUS: OPEN
- PRIORITY: 90
- TAGS: voice, desktop, logging

## Goal

Replace bare `eprintln!` in scufris-desktop with `tracing`. The service
logs structured fields to journald. The dev CLI shows pretty logs in the
foreground.

## Scope

- Crates: `tracing`, `tracing-subscriber` (env-filter, fmt),
  `tracing-journald`, `tracing-log` (bridges log-crate deps).
- Init: try `tracing_journald::layer()`. On failure or `--foreground`,
  use a fmt layer with ANSI when stderr is a TTY. Same binary both ways.
- `nix run .#scufris-desktop -- --foreground` gives colored logs.
- Level policy: INFO = lifecycle and state transitions only. DEBUG =
  per-request detail (whisper timings). WARN = degraded. ERROR =
  user-visible failure. `RUST_LOG` overrides everything.
- Forward the webview console into the Rust stream at DEBUG under a
  `webview` target (Tauri forwardConsole pattern).

## Verification

- A foreground run shows colored, filtered logs.
- `journalctl --user` shows structured fields from the service.
- `RUST_LOG` filters both paths.
- The webview console reaches journalctl at DEBUG.

Decided in `tasks/20260822-132001/RESEARCH.md` section 2. Build first:
it unblocks live playtesting of everything else with real observability.
