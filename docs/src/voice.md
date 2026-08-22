# Voice and desktop ownership

Voice is optional and disabled by default. The normal package excludes the speech extension, Piper, PipeWire, and voice assets from its closure.

## Scufris ownership

Voice-enabled Scufris owns:

- Speech extension composition.
- A private patched Piper 1.4.2 runtime.
- A pinned `en_US-lessac-medium` model and adjacent configuration.
- PipeWire playback.
- Trusted immutable model environment.
- A dedicated resumable popup conversation command.
- The direct Kitty launcher and `scufris-popup.service` definition.

The ordinary voice-capable launcher remains silent until speech is enabled. The popup launcher defaults speech and Calm on, then resumes its dedicated session.

## Integration

For optional voice input, configure speech-to-text in Pi. The popup preserves the Pi environment when it resumes the conversation.

Configure the desktop to start the defined service and present its stable Kitty class and instance. The desktop controls popup placement, keybindings, and toggle behavior.

## Trusted overrides

Home Manager permits Piper package, model, and configuration overrides. The package must report version 1.4.2. The model and configuration must be immutable Nix store paths, and the configuration must be adjacent to the model as `model.onnx.json`.
