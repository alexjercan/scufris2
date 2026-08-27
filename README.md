# Scufris

Scufris is a Pi-based assistant with a project workflow engine, delegated
agents, Calm mode, a background service that owns the conversation, and a
desktop companion with a voice pill and a conversation window.

## Quickstart

Scufris is a background service with clients. `scufris-service` owns the
conversation; a terminal and the desktop companion are two ways to reach it.

Start the service in one terminal. It supervises the first `scufris` on `PATH`;
from a fresh checkout, name one:

```bash
nix run .#scufris-service
nix run .#scufris-service -- \
  --agent "$(nix build --no-link --print-out-paths .#scufris)/bin/scufris"
```

Talk to it from any other terminal:

```bash
nix run .#scufris-ctl -- state              # what Scufris is doing
nix run .#scufris-ctl -- send "hello"       # say something
nix run .#scufris-ctl -- watch              # follow the conversation
nix run .#scufris-ctl -- debug              # open the session in this terminal
```

The rest of the flake:

```bash
nix run .#scufris-desktop   # the pill, the conversation window, and the tray
nix run .#scufris           # one Pi session with the Scufris extensions
nix build .#docs
nix develop
```

Home Manager:

```nix
programs.scufris = {
  enable = true;
  piPackage = inputs.llm-agents.packages.${pkgs.system}.pi;

  voice.enable = true;
  service.enable = true;

  desktop.enable = true;
};
```

The service logs to journald: `journalctl --user -t scufris-service -f`.

See the [installation guide](docs/src/guide/installation.md) for the options,
and the [background service](docs/src/dev/service.md) chapter for the socket.
