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
  piPackage = inputs.llm-agents.packages.${pkgs.system}.pi;

  voice = {
    enable = true;
    popup.enable = true;
  };
};
```
