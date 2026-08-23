# Scufris

Scufris is a Pi-based assistant with a project workflow engine, delegated agents, Dashboardd control, Calm mode, and optional local voice.

## Quickstart

```bash
nix run .#scufris
nix build .#docs
nix develop
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
