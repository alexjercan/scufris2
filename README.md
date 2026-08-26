# Scufris

Scufris is a Pi-based assistant with a project workflow engine, delegated agents, Calm mode, optional local voice, and a desktop voice pill.

## Quickstart

```bash
nix run .#scufris
nix run .#scufris-desktop
nix build .#docs
nix develop
```

Home Manager:

```nix
programs.scufris = {
  enable = true;
  piPackage = inputs.llm-agents.packages.${pkgs.system}.pi;

  voice = {
    enable = true;
    popup.enable = true;
  };

  desktop.enable = true;
};
```

`desktop.enable` adds the voice pill and tray companion. It requires the popup,
because the popup Pi process serves the control socket it talks to. `Super+D`
opens the pill and starts recording.
