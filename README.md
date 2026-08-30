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

For a Home Manager deployment that consumes the existing `ai-tools-api` on
port 10300:

```nix
programs.scufris = {
  enable = true;
  service.enable = true;

  desktop = {
    enable = true;
    speech.enable = true;
    aiToolsApi.manage = false;
  };
};
```

This uses the default Pi package and project roots. The desktop sends
transcription and speech requests to `http://127.0.0.1:10300`, while the
existing `ai-tools-api` deployment remains the single owner of Whisper and
Piper.
