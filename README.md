# Scufris

Scufris is a Pi-based assistant with delegated project workflows and a Linux
desktop companion. A background service owns one durable conversation. Desktop
surfaces connect to it for text, voice, and widgets, so several surfaces can
share the same conversation without owning the agent.

The desktop owns recording, playback, mute, cancellation, and presentation. It
uses the shared [`ai-tools-api`](https://github.com/alexjercan/ai-tools-api) HTTP
service for Whisper transcription and Piper speech. The agent process never
starts inference, and Scufris does not own Whisper or Piper directly.

## Quickstart

Run an isolated stack from this checkout beside any deployed Scufris:

```bash
nix run .#staging -- up
```

Staging uses an existing `ai-tools-api` on `127.0.0.1:10300` by default. If this
machine does not already run one, let the staging backend own the pinned API for
this run:

```bash
SCUFRIS_STAGING_AI_TOOLS_API=managed nix run .#staging -- up
```

Both commands stay in the foreground. Press `Ctrl+C` to stop only the processes
they started. Staging keeps separate sessions, state, and runtime sockets while
reusing the current Pi login.

To test several desktop surfaces against one conversation, use separate
terminals:

```bash
nix run .#staging -- backend
nix run .#staging -- frontend left
SCUFRIS_DESKTOP_HOTKEY=Super+H nix run .#staging -- frontend right
```

For a Home Manager deployment:

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

If the surrounding Home Manager configuration already enables
`services.ai-tools-api`, Scufris reuses its host and port. Otherwise Scufris
manages its pinned fallback service. To consume an API managed outside Home
Manager, configure it explicitly:

```nix
programs.scufris.desktop.aiToolsApi = {
  manage = false;
  baseUrl = "http://127.0.0.1:10300";
};
```

See the [installation guide](docs/src/guide/installation.md) for packages and
Home Manager options, and the [staging guide](docs/src/dev/staging.md) for
split backend and frontend operation.
