# Scufris

Scufris is an assistant that works with you; optional voice owns local Piper speech and a direct Kitty popup, while STT, Whisper, and desktop or i3 policy stay external.

## Quickstart

```bash
nix run .#scufris
```

Home Manager:

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

Ownership:

- Scufris owns optional Piper speech, pinned voice assets, PipeWire playback, and the direct Kitty popup service.
- Global Pi configuration owns STT, Whisper, and its endpoint.
- The desktop consumer owns i3 startup, marks, geometry, keybindings, and toggle behavior.

Development:

```bash
nix develop
npm install
npm run check
npm run dev
npm run dev:voice
```

Both development commands use system Pi, working-tree resources, and dedicated resumable development sessions. `dev:voice` requires the repository `nix develop` shell for Piper, PipeWire, and trusted model paths. It enables speech and Calm but inherits STT configuration unchanged.
