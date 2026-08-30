# Consume ai-tools-api for desktop speech

- STATUS: CLOSED
- PRIORITY: 80
- TAGS: desktop, nix, speech

## Ask

Replace Scufris-owned Whisper and Piper inference with the published
`ai-tools-api` v0.1.1 HTTP API. Use the shared loopback API on port 10300 when
provided. Let the Scufris flake provide the pinned package/module for standalone
managed deployments. Keep recording, playback, cancellation, speech mute, and
presentation frontend-local.

## Decisions

- Pin `ai-tools-api` as a flake input and expose its package/app.
- Detect an enabled `services.ai-tools-api` option from the surrounding Home
  Manager composition without redeclaring or importing its module. If none is
  enabled, managed mode runs the pinned complete package as one hardened
  fallback unit. A deployment can set `desktop.aiToolsApi.manage = false` and
  provide another base URL.
- Use the OpenAI-compatible transcription and speech routes. Do not probe a
  port and silently create inference processes from individual frontends.
- Keep `scufris-speak` as the bounded stdin-to-local-playback process boundary,
  but make it an HTTP TTS adapter with no Piper package or model ownership.
- Remove Scufris Whisper/Piper packages, assets, units, patches, and direct
  inference checks after the API clients are covered.

## Implementation

- Pinned published `ai-tools-api` v0.1.1 at commit
  `80dd40fd2ab7538946910d7c14fb24fe496f559b` and exposed it as the
  `ai-tools-api` package and app.
- Changed desktop STT to the exact OpenAI-compatible multipart contract:
  `model=whisper-1`, `language=auto`, `response_format=json`, and bounded WAV.
  Removed whisper.cpp's `temperature` field and changed the default route to
  port 10300.
- Replaced direct Piper execution in `scufris-speak` with a bounded standard
  library HTTP client for `/v1/audio/speech`, fixed model/voice/format fields,
  RIFF/WAVE validation, local `pw-play`, exact child cancellation, and DEBUG
  request/response metadata without speech text.
- Removed Scufris-owned Piper/Whisper package files, patches, models, dev-shell
  bindings, local Whisper unit, and obsolete environment paths.
- Home Manager detects an enabled upstream `services.ai-tools-api` provider and
  derives its base URL without importing or redeclaring that module. If absent,
  managed mode defaults on and runs the pinned complete API package as one
  hardened `scufris-ai-tools-api` fallback unit. Explicit external mode creates
  no inference unit.
- Staging defaults to the deployed API. `SCUFRIS_STAGING_AI_TOOLS_API=managed`
  makes `backend` or `up` own and stop the pinned complete API process. All
  named frontends share the same STT/TTS routes.
- Rewrote installation, architecture, operation, maintenance, desktop, and
  staging documentation around the shared API boundary.

## Verification

- 312 desktop Rust tests pass, including the exact STT multipart fixture.
- 67 TypeScript/UI/helper tests pass. The speech helper tests prove exact TTS
  JSON, bounded response/error handling, WAV validation, playback, and
  exact-child cancellation.
- All 9 staging integration tests pass, including managed API ownership and
  teardown.
- Ruff and ShellCheck pass for changed helpers.
- Focused Nix checks pass for provided-provider composition, standalone managed
  fallback, external mode, desktop configuration, speech/API closure
  separation, helper packaging, staging packaging, Home Manager, and docs.
- The live deployed v0.1.1 API is active on ports 10300/10301. Real synthesis
  returned valid mono PCM WAV, the source speech adapter accepted and passed it
  to a fake local player, and real transcription returned
  `The AI tools API migration works.` See `api-transcription.json`.
- A packaged external-mode staging backend and named `api-one` frontend reached
  `idle`, resolved both port-10300 API routes, and reused the unchanged deployed
  API PID. All staging-owned PIDs were stopped; the shared API remained active.
- `nix flake show --all-systems` evaluated Linux and Darwin outputs. Final
  `nix flake check` passed all 33 compatible x86_64-linux checks.
