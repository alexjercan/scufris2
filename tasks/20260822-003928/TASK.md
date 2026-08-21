# Move voice runtime and popup ownership into Scufris

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: voice, nix, architecture

## Goal

Make voice an optional Scufris product feature. Move Piper and the direct Kitty popup service into the Scufris package and Home Manager module. Leave global STT, Whisper, and i3 integration for the later `nix.dotfiles` migration.

## Accepted ownership

Scufris owns:

- Optional speech extension composition.
- A private patched Piper 1.4.2 package.
- The pinned `en_US-lessac-medium` model and adjacent config.
- PipeWire playback runtime.
- Trusted Piper model environment.
- A dedicated resumable popup conversation command.
- The direct Kitty popup launcher and `scufris-popup.service`.
- Stable popup class, instance, initial title, and service identity.

Scufris does not own:

- `pi-voice-stt`, FFmpeg capture, Whisper, or the STT endpoint.
- i3 marks, geometry, startup policy, keybindings, ownership query, or toggle behavior.
- Cloud speech, RPC, a native frontend, or tmux.

The later `nix.dotfiles` phase will consume the Scufris popup interface, keep global STT, and own i3 presentation.

## Required Home Manager interface

Support this primary configuration:

```nix
programs.scufris = {
  enable = true;
  piPackage = config.programs.pi.coding-agent.finalPackage;

  voice = {
    enable = true;
    popup.enable = true;
  };
};
```

Requirements:

- `voice.enable` defaults to false.
- `voice.popup.enable` defaults to false and requires voice.
- Voice options permit trusted package/model/config overrides without mutable downloads.
- Popup options expose session directory, terminal package, class, instance, and initial title.
- Expose read-only final package/launcher and stable service identity needed by a separate i3 consumer. Keep the interface narrow.
- The module defines the popup user service but does not enable or start it. The desktop consumer owns startup policy.
- Popup launcher sets speech and Calm defaults, then resumes the dedicated session.
- It inherits global Pi/STT environment. It does not know or set a Whisper endpoint or STT config path.

## Package composition

- Normal Scufris excludes `speech.ts`, Piper, the voice model, and PipeWire when voice is disabled.
- Voice-enabled Scufris includes `speech.ts`, the private patched Piper, PipeWire playback, and trusted model/config environment.
- Ordinary voice-capable `scufris` launches remain silent until `/speech` enables output.
- Only the popup launcher defaults `SCUFRIS_SPEECH=1` and `SCUFRIS_CALM=1`.
- Do not globally override `pkgs.piper-tts`.
- Preserve exact no-shell process ownership and the existing validated WAV path.
- Provide a standalone voice-capable package or equivalent flake checkable output when it makes the interface clearer.

## Development and process role

- Replace the unclear `SCUFRIS_FOREGROUND=1` marker with
  `SCUFRIS_ROLE=orchestrator` everywhere. Do not preserve the old marker.
- Identity and speech behavior require the orchestrator role.
- Delegated workers remove the role from their environment.
- Replace `npm run pi` with `npm run dev` and `npm run dev:voice`.
- Keep package scripts small. Use one checked-in executable helper with a
  `--voice` switch and Bash arrays for extension and skill arguments.
- Both development commands load working-tree Scufris source through the
  system Pi and use a dedicated development session directory.
- `dev:voice` gets Piper, PipeWire, and trusted model paths from `nix develop`,
  enables speech and Calm, and fails clearly outside the prepared shell.
- The development runner inherits global STT configuration. It must not know or
  set a Whisper endpoint or `PI_STT_CONFIG` path.
- Add exact development composition and worker-environment tests.

## Breaking-change policy

Do not preserve the current accidental always-on Piper composition. Do not
preserve `SCUFRIS_FOREGROUND`, `npm run pi`, or aliases for the later
`services.localVoice` interface. That interface belongs to another repository
and will be deleted in phase two.

Do not modify `nix.dotfiles` in this task. Its currently pinned generation must
continue running until phase two.

## Definition of done

- The Home Manager interface above evaluates and builds.
- Disabled Scufris closure excludes speech, Piper, PipeWire, and voice models.
- Voice-enabled closure includes only the private patched Piper runtime and pinned voice assets.
- Piper stdout synthesis produces a complete non-empty RIFF/WAVE accepted by `scufris-speak`.
- Popup service runs Kitty directly with stable identity and a dedicated resumable session.
- Popup unit is defined but has no automatic target installation or i3 dependency.
- Popup and normal launcher environment behavior is tested exactly.
- Existing delegation, widgets, project discovery, Calm defaults, and noninteractive modes remain correct.
- README documents feature ownership and the desktop-consumer boundary.
- Task evidence and retro record decisions and checks.

## Verification

- `npm run check`.
- Focused launcher, closure, Home Manager evaluation, popup unit, and real Piper fixture checks.
- `nix flake check`.
- `git diff --check`.
- No live Home Manager activation in this phase.

## Implementation evidence

- Split packaged resources and launch composition. Default Scufris omits the speech extension and helper. `scufris-voice` adds them with Python, PipeWire, trusted model environment, and the private Piper package.
- Added a private reduced Piper 1.4.2 derivation with the close-before-stdout-copy patch. The global `pkgs.piper-tts` remains unchanged. Pinned `en_US-lessac-medium` model and adjacent config hashes match the audited voice deployment.
- Added Home Manager `programs.scufris.voice` and nested popup options. Model and config overrides must be Nix store paths. The module exposes public documented read-only final package, popup launcher, and `scufris-popup` service identity. A separate test module consumes all three outputs.
- Added one direct Kitty popup launcher. It creates the dedicated session directory, defaults speech and Calm on, preserves inherited Pi and STT environment, and runs voice-capable Scufris with `--session-dir` and `--continue`.
- Added `scufris-popup.service` without `Install`, target wants, startup policy, desktop criteria, or i3 configuration. Stable class, instance, initial title, and service name default to `Scufris`, `scufris-popup`, `Scufris`, and `scufris-popup`.
- Removed speech from default npm package composition. Replaced the old development scripts with one array-based `scripts/scufris-dev` runner behind `npm run dev` and `npm run dev:voice`.
- Replaced the foreground marker with `SCUFRIS_ROLE=orchestrator` in launchers, identity, speech, tests, and durable architecture records. Delegated workers strip the role.
- Added a dedicated resumable development session and exact normal and voice development composition. The development runner preserves configured project roots or sets the accepted launcher default. The Nix development shell supplies private Piper, PipeWire, and trusted model paths. Voice development enables speech and Calm and inherits STT unchanged.
- Moved substantial fixtures and check construction to focused `nix/checks.nix`; `flake.nix` now contains package and output composition.
- Expanded README with the primary Home Manager interface, Scufris/STT/i3 ownership boundary, and development requirements.
- Kept `scripts/scufris-speak` unchanged. Exact no-shell child ownership, bounded in-memory WAV handling, validation, and cancellation behavior remain intact.

## Decisions and tradeoffs

- Use separate normal and voice resource outputs. Merely omitting the speech CLI argument would still retain `speech.ts` in the default resource closure.
- Keep trusted model and config paths adjacent because Piper 1.4.2 ignores its config argument while loading. Reject non-adjacent overrides before activation.
- Build Piper without training, HTTP, or alignment extras. This keeps the voice closure on the required synthesis runtime.
- Expose the direct popup launcher, not an i3 toggle or ownership query. The later desktop consumer can read stable class, instance, service name, and launcher without transferring presentation ownership.
- Define the popup unit only when Scufris, voice, and popup are enabled. Omit all install targets so evaluation and activation do not start it.
- Keep stable consumer outputs public even though they are computed and read-only. Home Manager `internal` would hide the accepted cross-repository interface.

## Verification evidence

- `sprout sync scufris-voice-feature` - passed before implementing the accepted development-role scope update; feature advanced from `584343f` to master `72df74b`.
- Post-commit `sprout sync scufris-voice-feature` - passed, already up to date. Post-sync reruns of all checks below passed.
- `npm install && npm run check` - passed: strict TypeScript, 33 tests, and Prettier, including exact working-tree development composition and process-role behavior.
- Focused launcher, resource, closure, Home Manager, external consumer interface, popup unit, development shell, and worker-environment checks - passed.
- `nix develop --command npm run dev:voice -- user-argument` with a fake system Pi - passed. It used working-tree resources, the dedicated session directory, private Piper and PipeWire shell runtime, speech and Calm defaults, and unchanged inherited STT.
- Real private Piper 1.4.2 fixture - passed. `scufris-speak` accepted complete non-empty RIFF/WAVE synthesis and sent it to the exact fake `pw-play` sink.
- Direct closure inspection - default had no Piper, PipeWire, Lessac assets, or speech resources; voice had only the private Piper output, PipeWire, and pinned model/config assets.
- Initial `npm run check` before dependency installation failed because `tsc` was absent. No product defect.
- Initial real fixture execution used the helper's `/usr/bin/env` shebang inside the Nix sandbox, where `/usr/bin` is absent. Invoking the unchanged helper through packaged Python fixed the fixture without changing runtime behavior.
- `python3 -m unittest discover -s tests -p 'test_*.py'` - passed, 22 tests.
- Ruff check, Ruff format check, ShellCheck, and Python bytecode compilation for all helpers and tests - passed. The first post-scope-update format check found one worker-test line wrapping difference; formatting it fixed the check.
- `nix fmt -- --check .` - passed.
- `nix flake check` - passed, including the real Piper, closure, launcher, Home Manager, popup interface, and uninstalled unit checks.
- `git diff --check` - passed.
- No Home Manager activation was run.
