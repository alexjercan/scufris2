# Add Scufris spoken response mode

- STATUS: IN_PROGRESS
- PRIORITY: 100
- TAGS: voice, speech, tui

## Goal

Add Scufris-only spoken response mode for the persistent Kitty popup while preserving normal detailed visual output.

## Accepted design

- Keep Pi TUI. Do not build a native or RPC frontend in this task.
- Speech is active only when `SCUFRIS_SPEECH=1` or enabled through `/speech`; normal Pi and ordinary Scufris launches remain silent.
- Add `/speech on`, `/speech off`, `/speech once`, and `/speech replay`.
- In speech mode, append a narrow per-turn instruction: the first final-answer paragraph is short natural prose in complete sentences with no Markdown, bullets, paths, hashes, URLs, or code. Normal visual Markdown details can follow after one blank line.
- Wait for `agent_settled`. Never speak intermediate assistant messages, tool-call turns, thinking, tool output, or worker progress.
- Extract and bound only the first plain prose paragraph from the final assistant response. Fail closed when no safe paragraph exists.
- Add a private helper `scripts/scufris-speak`. It accepts bounded UTF-8 text only through stdin and invokes Piper followed by PipeWire playback without a shell.
- Trusted runtime environment supplies exact Piper model and optional config paths. No model-facing path, command, voice, or desktop operation is added.
- Track exact child processes. New user input, explicit speech cancellation or replacement, reload, and shutdown stop only recorded playback processes. Never use broad process cleanup.
- `/speech replay` derives the last safe paragraph from session state; no durable speech file or second model call.
- Speech errors are compact UI notifications and never fail the completed assistant turn.
- The popup launcher can opt into Calm mode through a narrow environment setting. Do not change normal Calm defaults.

## Cross-project runtime contract

`nix.dotfiles` will launch the popup with:

```text
SCUFRIS_SPEECH=1
SCUFRIS_CALM=1
SCUFRIS_PIPER_MODEL=<trusted Nix store model>
SCUFRIS_PIPER_CONFIG=<trusted Nix store config>
```

The Scufris launcher must preserve these variables and include Piper and `pw-play` in the speech-enabled runtime PATH. The popup passes Pi session arguments for a dedicated persisted Scufris conversation.

## Definition of done

- Speech mode changes only final response style and TTS behavior.
- The first paragraph is spoken after settlement while full visual detail remains visible.
- On, off, once, and replay behavior is deterministic across new turns, reload, and session resume.
- Intermediate and unsafe content is never spoken.
- Playback replacement and shutdown target exact owned processes only.
- Missing Piper, model, audio device, malformed output, and cancellation fail safely.
- Print, JSON, RPC, normal Pi, workers, and non-speech Scufris remain silent.
- Tests cover extraction, mode transitions, settlement, exact process lifecycle, errors, and launcher composition.
- Documentation and task evidence match the implementation.

## Verification

- `npm run check`.
- Focused helper integration tests with fake Piper and playback executables.
- `nix flake check`.
- `git diff --check`.
- Live Piper playback after the Nix integration lands.
