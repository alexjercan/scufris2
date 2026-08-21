# Move voice runtime and popup ownership into Scufris

- STATUS: IN_PROGRESS
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

## Breaking-change policy

Do not preserve the current accidental always-on Piper composition. Do not add aliases for the later `services.localVoice` interface. That interface belongs to another repository and will be deleted in phase two.

Do not modify `nix.dotfiles` in this task. Its currently pinned generation must continue running until phase two.

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
