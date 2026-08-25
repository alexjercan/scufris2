# Verification

Date: 2026-08-25. Binary: `desktop/target/debug/scufris-desktop` 0.4.0.

## A foreground run shows colored, filtered logs

`SCUFRIS_STT_ENDPOINT='notaurl' ./target/debug/scufris-desktop --foreground`
exits 1 and prints through the fmt layer (timestamp, level, target):

```
2026-08-25T13:01:23.023735Z ERROR scufris_desktop: SCUFRIS_STT_ENDPOINT must be an http or https URL
```

ANSI is on when stderr is a terminal (`IsTerminal`); the captured run above
went through a pipe, so it is shown plain. `RUST_LOG=warn` still shows the
ERROR line; `RUST_LOG=off` silences it.

## journalctl shows structured fields from the service path

The same failing run without `--foreground` reaches the user journal.
`journalctl --user -t scufris-desktop -o verbose` shows:

```
PRIORITY=3
SYSLOG_IDENTIFIER=scufris-desktop
TARGET=scufris_desktop
CODE_FILE=scufris-desktop/src/main.rs
CODE_LINE=68
MESSAGE=SCUFRIS_STT_ENDPOINT must be an http or https URL
```

## RUST_LOG filters both paths

- Foreground: `RUST_LOG=warn` keeps the ERROR line, shown above.
- Journald: `RUST_LOG=off` plus the same failing run leaves
  `journalctl --user -t scufris-desktop --since "-5s"` empty
  ("No entries").

The filter is one `EnvFilter` built before the layer split, so it cannot
diverge between the paths.

## The webview console reaches journalctl at DEBUG

Covered in code: `pill.js` wraps `console.debug|log|info|warn|error` plus
`error` and `unhandledrejection` events into the `pill_log` command, which
logs `debug!(target: "webview", level = %level, ...)`. Forwarding swallows
its own failures so a rejected invoke cannot forward itself forever.
Watching it live in journalctl needs a running pill session; that is the
one item left for live playtesting.

## Checks run

- `cargo test` (desktop workspace): 106 tests pass, including the new
  `logging::tests::a_second_init_reports_instead_of_panicking` and
  `state::tests::phase_names_are_stable_and_free_of_transcripts`.
- `cargo clippy --all-targets`: no warnings.
- `cargo fmt --check`: clean.
- `npx prettier --check .`: clean.
- `nix build .#scufris-desktop`: consumes the updated `Cargo.lock`
  (tracing, tracing-subscriber, tracing-journald, tracing-log and their
  closure) and runs the Rust tests in its check phase (three `test
result: ok` lines in the build log). The first attempt failed with
  "file not found for module `logging`" because a flake build sees only
  git-tracked files; `git add desktop/scufris-desktop/src/logging.rs`
  fixed it. The packaged binary answers `--version` and takes the
  `--foreground` tracing path (exit 1 on the invalid-endpoint probe).

## Level policy in the code

- INFO: `starting` (version, socket, stt, hotkey, after the WebKit
  re-exec so it logs once), `stopping`, every phase change (one log point
  inside `App::decide` under the companion lock), `daemon connected`,
  assistant state changes.
- DEBUG: whisper timing and sizes in `stt::transcribe`, daemon welcome
  and per-submission answers in `App::observe`, the webview console.
- WARN: degraded paths (render trouble, focus restore, tombstone
  fallback, missing hooks, restart budget, rejected daemon message,
  `daemon disconnected`).
- ERROR: user-visible failures (abandoned interaction, storage clear
  failure, transcription failure, hook spawn failure, fatal startup).
- Transcripts never reach the log at any level; only byte counts do.
