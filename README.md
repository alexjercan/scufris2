# Scufris

Scufris is a Pi-based assistant with delegated project workflows, a background
service that owns the conversation, and a Linux desktop companion for voice,
conversation, and widgets.

## Quickstart

Run the complete stack from this checkout in an isolated staging environment:

```bash
nix run .#staging -- up
```

Staging uses the current Pi login, keeps its own sessions and runtime paths, and
runs beside any deployed Scufris. Press `Ctrl+C` to stop it.

For a Home Manager deployment with the defaults:

```nix
programs.scufris = {
  enable = true;
  service.enable = true;

  desktop = {
    enable = true;
    speech.enable = true;
  };
};
```

This uses the default Pi package and project roots. The desktop enables the
pinned `ai-tools-api` service by default and uses its loopback transcription and
speech routes. Set `desktop.aiToolsApi.manage = false` when another deployment
already owns the configured API endpoint.
