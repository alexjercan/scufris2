# Add Scufris spoken response mode

- STATUS: CLOSED
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

## Implementation evidence

- Added `extensions/scufris/speech.ts` for TUI-only persisted on, off, once, and replay state; per-run prompt shaping; settled final-response extraction; compact notifications; and exact helper ownership.
- Added `scripts/scufris-speak` for bounded UTF-8 stdin, trusted model and optional config environment paths, fixed no-shell Piper and `pw-play` processes, deadlines, and exact child termination.
- Cross-project Piper 1.4.2 feedback showed that validated prose requires a final LF. The helper now appends exactly one internal LF, buffers at most 64 MiB of synthesis output, validates a non-empty RIFF/WAVE format and data structure, and starts `pw-play` only after validation.
- Added speech extension and runtime packaging in `package.json`, `nix/launcher.nix`, and `flake.nix`. Launcher checks prove popup variables stay exact and Piper plus `pw-play` are on runtime PATH.
- Added `SCUFRIS_CALM=1` startup opt-in without changing the existing Calm default.
- Removed popup-only speech and Calm variables from delegated Pi and Claude worker environments in `scripts/scufris-job`.
- Added extraction, transition, settlement, persistence, replay, mode isolation, error, helper integration, malformed audio, missing runtime, replacement, and exact cancellation tests.

## Decisions and tradeoffs

- Session custom entries store mode state. Branch, reload, and resume restore the applicable state. The environment is the fallback only when the session has no speech state.
- `/speech once` changes the mode to one-shot and consumes it when the next agent run starts. Retry and queued continuation settlement retain that run's decision.
- Extraction rejects the latest final assistant response instead of falling back to older safe speech. This prevents stale replay after malformed or unsafe output.
- Playback uses bounded in-memory WAV bytes. No audio or speech text file is created. Playback starts from the settled response without a second model call.
- The launcher always includes the fixed speech runtime because `/speech on` can enable speech after startup.

## Verification evidence

- `npm run check` - passed, including 31 TypeScript and helper integration tests.
- `python3 -m unittest discover -s tests -p 'test_*.py'` - passed, 22 tests.
- `nix develop --command bash -lc 'ruff check ... && ruff format --check ... && python3 -m py_compile ...'` - passed.
- `nix fmt -- --check .` - passed.
- `nix flake check` - passed after staging new flake source files. The initial resources check correctly failed because Git-based flake source filtering omitted unstaged new files.
- Post-sync testing exposed a fake-helper readiness race: the fixture logged startup before installing its signal handler. The fixture now installs the handler first, so cancellation assertions are deterministic.
- Review feedback regressions prove a fake Piper emits WAV only after the internal LF, and zero-byte or malformed successful synthesis never starts the fake player.
- `git diff --check` - passed.
- Live Piper playback remains for the external `nix.dotfiles` model and popup integration, as required by the accepted verification plan.
